use std::{ffi::CString, path::PathBuf};

pub type Frame<'a> = (&'a [u8], ImageDimentions);

enum PipelineInfo {
    Rendering { frag: ShaderInfo, vert: ShaderInfo },
    Compute { comp: ShaderInfo },
}

#[derive(Hash, Debug, Clone)]
pub struct ShaderInfo {
    pub name: PathBuf,
    pub entry_point: CString,
}

impl ShaderInfo {
    pub fn new(path: PathBuf, entry_point: String) -> ShaderInfo {
        ShaderInfo {
            name: path,
            entry_point: CString::new(entry_point).unwrap(),
        }
    }
}

#[derive(Hash, Debug, Clone)]
pub enum ShaderType {
    Glsl,
    Wgsl,
    Spir,
}

#[derive(Hash, Debug, Clone)]
pub enum ShaderStage {
    Vertex = 0,
    Fragment,
    Compute,
}

#[derive(Hash, Debug, Clone)]
pub struct ShaderData {
    pub source: String,
    pub entry_point: String,
    pub ty: ShaderType,
    pub stage: ShaderStage,
}

#[derive(Debug, Clone, Copy)]
pub struct ImageDimentions {
    pub width: usize,
    pub height: usize,
    pub padded_bytes_per_row: usize,
    pub unpadded_bytes_per_row: usize,
}

impl ImageDimentions {
    pub fn new(width: usize, height: usize, align: usize) -> Self {
        let bytes_per_pixel = std::mem::size_of::<[u8; 4]>();
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let padded_bytes_per_row_padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padded_bytes_per_row_padding;
        Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
        }
    }
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
    pub record_period: f32,
}

impl PushConstant {
    unsafe fn as_slice(&self) -> &[u8] {
        any_as_u8_slice(self)
    }

    pub fn size() -> u32 {
        std::mem::size_of::<Self>() as _
    }
}

/// # Safety
/// Until you're using it on not ZST or DST it's fine
pub unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    std::slice::from_raw_parts((p as *const T) as *const _, std::mem::size_of::<T>())
}

impl Default for PushConstant {
    fn default() -> Self {
        Self {
            pos: [0.; 3],
            time: 0.,
            wh: [1920.0, 780.],
            mouse: [0.; 2],
            mouse_pressed: false as _,
            frame: 0,
            time_delta: 1. / 60.,
            record_period: 10.,
        }
    }
}

// TODO: Make proper ms -> sec converion
impl std::fmt::Display for PushConstant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "position:\t{:?}\n\
             time:\t\t{:.2}\n\
             time delta:\t{:.3} ms, fps: {:.2}\n\
             width, height:\t{:?}\nmouse:\t\t{:.2?}\n\
             frame:\t\t{}\nrecord_period:\t{}\n",
            self.pos,
            self.time,
            self.time_delta * 1000.,
            1. / self.time_delta,
            self.wh,
            self.mouse,
            self.frame,
            self.record_period
        )
    }
}
