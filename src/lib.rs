#![allow(clippy::new_without_default)]
#![allow(clippy::too_many_arguments)]

pub mod default_shaders;
mod device;
mod input;
mod instance;
mod pipeline_arena;
mod recorder;
mod shader_compiler;
mod surface;
mod swapchain;
mod texture_arena;
mod watcher;

use std::{
    fs::File,
    io,
    mem::ManuallyDrop,
    ops::{Add, Rem, Sub},
    path::Path,
    sync::Arc,
    time::Duration,
};

pub use self::{
    device::{Device, HostBufferTyped, RawDevice},
    input::Input,
    instance::Instance,
    pipeline_arena::*,
    recorder::{RecordEvent, Recorder},
    shader_compiler::ShaderCompiler,
    surface::Surface,
    swapchain::Swapchain,
    texture_arena::*,
    watcher::Watcher,
};

use anyhow::{bail, Context};
use ash::vk::{self, DeviceMemory};
use gpu_alloc::{GpuAllocator, MapError, MemoryBlock};
use gpu_alloc_ash::AshMemoryDevice;
use parking_lot::Mutex;

pub const SHADER_DUMP_FOLDER: &str = "shader_dump";
pub const SHADER_FOLDER: &str = "shaders";
pub const VIDEO_FOLDER: &str = "recordings";
pub const SCREENSHOT_FOLDER: &str = "screenshots";

pub const COLOR_SUBRESOURCE_MASK: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: vk::REMAINING_MIP_LEVELS,
    base_array_layer: 0,
    layer_count: vk::REMAINING_ARRAY_LAYERS,
};

// pub fn align_to(value: u64, alignment: u64) -> u64 {
//     (value + alignment - 1) & !(alignment - 1)
// }

pub fn align_to<T>(value: T, alignment: T) -> T
where
    T: Add<Output = T> + Copy + Default + PartialEq<T> + Rem<Output = T> + Sub<Output = T>,
{
    let remainder = value % alignment;
    if remainder == T::default() {
        value
    } else {
        value + alignment - remainder
    }
}

pub fn dispatch_optimal(len: u32, subgroup_size: u32) -> u32 {
    let padded_size = (subgroup_size - len % subgroup_size) % subgroup_size;
    (len + padded_size) / subgroup_size
}

pub fn create_folder<P: AsRef<Path>>(name: P) -> io::Result<()> {
    match std::fs::create_dir(name) {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }

    Ok(())
}

pub fn print_help() {
    println!();
    println!("- `F1`:   Print help");
    println!("- `F2`:   Toggle play/pause");
    println!("- `F3`:   Pause and step back one frame");
    println!("- `F4`:   Pause and step forward one frame");
    println!("- `F5`:   Restart playback at frame 0 (`Time` and `Pos` = 0)");
    println!("- `F6`:   Print parameters");
    println!("- `F10`:  Save shaders");
    println!("- `F11`:  Take Screenshot");
    println!("- `F12`:  Start/Stop record video");
    println!("- `ESC`:  Exit the application");
    println!("- `Arrows`: Change `Pos`\n");
}

#[derive(Debug)]
pub struct Args {
    pub inner_size: Option<(u32, u32)>,
    pub record_time: Option<Duration>,
}

pub fn parse_args() -> anyhow::Result<Args> {
    let mut inner_size = None;
    let mut record_time = None;
    let args = std::env::args().skip(1).step_by(2);
    for (flag, value) in args.zip(std::env::args().skip(2).step_by(2)) {
        match flag.trim() {
            "--record" => {
                let time = match value.split_once('.') {
                    Some((sec, ms)) => {
                        let seconds = sec.parse()?;
                        let millis: u32 = ms.parse()?;
                        Duration::new(seconds, millis * 1_000_000)
                    }
                    None => Duration::from_secs(value.parse()?),
                };
                record_time = Some(time)
            }
            "--size" => {
                let (w, h) = value
                    .split_once('x')
                    .context("Failed to parse window size: Missing 'x' delimiter")?;
                inner_size = Some((w.parse()?, h.parse()?));
            }
            _ => {}
        }
    }

    Ok(Args {
        record_time,
        inner_size,
    })
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PushConstant {
    pub pos: [f32; 3],
    pub time: f32,
    pub wh: [f32; 2],
    pub mouse: [f32; 2],
    pub mouse_pressed: u32,
    pub frame: u32,
    pub time_delta: f32,
    pub record_time: f32,
}

impl Default for PushConstant {
    fn default() -> Self {
        Self {
            pos: [0.; 3],
            time: 0.,
            wh: [1920.0, 1020.],
            mouse: [0.; 2],
            mouse_pressed: false as _,
            frame: 0,
            time_delta: 1. / 60.,
            record_time: 10.,
        }
    }
}

impl std::fmt::Display for PushConstant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let time = Duration::from_secs_f32(self.time);
        let time_delta = Duration::from_secs_f32(self.time_delta);
        write!(
            f,
            "position:\t{:?}\n\
             time:\t\t{:#.2?}\n\
             time delta:\t{:#.3?}, fps: {:#.2?}\n\
             width, height:\t{:?}\nmouse:\t\t{:.2?}\n\
             frame:\t\t{}\nrecord_period:\t{}\n",
            self.pos,
            time,
            time_delta,
            1. / self.time_delta,
            self.wh,
            self.mouse,
            self.frame,
            self.record_time
        )
    }
}

pub fn save_shaders<P: AsRef<Path>>(path: P) -> anyhow::Result<()> {
    let dump_folder = Path::new(SHADER_DUMP_FOLDER);
    create_folder(dump_folder)?;
    let dump_folder =
        dump_folder.join(chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string());
    create_folder(&dump_folder)?;
    let dump_folder = dump_folder.join(SHADER_FOLDER);
    create_folder(&dump_folder)?;

    if !path.as_ref().is_dir() {
        bail!("Folder wasn't supplied");
    }
    let shaders = path.as_ref().read_dir()?;

    // for path in paths {
    for shader in shaders {
        let shader = shader?.path();
        let to = dump_folder.join(shader.strip_prefix(Path::new(SHADER_FOLDER).canonicalize()?)?);
        if !to.exists() {
            std::fs::create_dir_all(&to.parent().unwrap().canonicalize()?)?;
            File::create(&to)?;
        }
        std::fs::copy(shader, &to)?;
        eprintln!("Saved: {}", &to.display());
    }

    Ok(())
}

#[derive(Debug)]
pub enum UserEvent {
    Glsl { path: std::path::PathBuf },
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ShaderSource {
    pub path: std::path::PathBuf,
    pub kind: ShaderKind,
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum ShaderKind {
    Fragment,
    Vertex,
    Compute,
}

impl From<ShaderKind> for shaderc::ShaderKind {
    fn from(value: ShaderKind) -> Self {
        match value {
            ShaderKind::Compute => shaderc::ShaderKind::Compute,
            ShaderKind::Vertex => shaderc::ShaderKind::Vertex,
            ShaderKind::Fragment => shaderc::ShaderKind::Fragment,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImageDimensions {
    pub width: usize,
    pub height: usize,
    pub padded_bytes_per_row: usize,
    pub unpadded_bytes_per_row: usize,
}

impl ImageDimensions {
    pub fn new(width: usize, height: usize, alignment: u64) -> Self {
        let channel_width = std::mem::size_of::<[u8; 4]>();
        let unpadded_bytes_per_row = width * channel_width;
        let padded_bytes_per_row = align_to(unpadded_bytes_per_row, alignment as usize);
        Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
        }
    }
}

pub struct ManagedImage {
    pub image: vk::Image,
    pub memory: ManuallyDrop<MemoryBlock<DeviceMemory>>,
    pub image_dimensions: ImageDimensions,
    pub data: Option<&'static mut [u8]>,
    pub format: vk::Format,
    device: Arc<RawDevice>,
    allocator: Arc<Mutex<GpuAllocator<DeviceMemory>>>,
}

impl ManagedImage {
    pub fn new(
        device: &Device,
        info: &vk::ImageCreateInfo,
        usage: gpu_alloc::UsageFlags,
    ) -> anyhow::Result<Self> {
        let image = unsafe { device.create_image(info, None)? };
        let memory_reqs = unsafe { device.get_image_memory_requirements(image) };
        let memory = device.alloc_memory(memory_reqs, usage)?;
        unsafe { device.bind_image_memory(image, *memory.memory(), memory.offset()) }?;
        let image_dimensions = ImageDimensions::new(
            info.extent.width as _,
            info.extent.height as _,
            memory_reqs.alignment,
        );
        Ok(Self {
            image,
            memory: ManuallyDrop::new(memory),
            image_dimensions,
            format: info.format,
            data: None,
            device: device.device.clone(),
            allocator: device.allocator.clone(),
        })
    }

    pub fn map_memory(&mut self) -> Result<&mut [u8], MapError> {
        if self.data.is_some() {
            return Err(MapError::AlreadyMapped);
        }
        let size = self.memory.size() as usize;
        let offset = self.memory.offset();
        let ptr = unsafe {
            self.memory
                .map(AshMemoryDevice::wrap(&self.device), offset, size)?
        };

        let data = unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr().cast(), size) };
        self.data = Some(data);

        Ok(self.data.as_mut().unwrap())
    }
}

impl Drop for ManagedImage {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image(self.image, None);
            {
                let mut allocator = self.allocator.lock();
                let memory = ManuallyDrop::take(&mut self.memory);
                allocator.dealloc(AshMemoryDevice::wrap(&self.device), memory);
            }
        }
    }
}

pub fn find_memory_type_index(
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    memory_type_bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_prop.memory_types[..memory_prop.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_type_bits != 0 && (memory_type.property_flags & flags) == flags
        })
        .map(|(index, _memory_type)| index as _)
}
