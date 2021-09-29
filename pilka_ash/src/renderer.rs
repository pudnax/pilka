mod images;
mod screenshot;

pub use screenshot::ImageDimentions;

use crate::pvk::{
    utils::{any_as_u8_slice, return_aligned},
    *,
};
use ash::{
    prelude::VkResult,
    vk::{self, Extent3D, PhysicalDeviceType, SubresourceLayout},
};
use std::{
    collections::{HashMap, HashSet},
    ffi::CStr,
    hash::Hash,
    io::Write,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use images::{FftTexture, VkTexture};
use screenshot::{Frame, ScreenshotCtx};

fn graphics_desc_set_leyout(device: &VkDevice) -> VkResult<Vec<vk::DescriptorSetLayout>> {
    let descriptor_set_layout = {
        let descriptor_set_layout_binding_descs = [
            vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(2)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(3)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(4)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build(),
        ];
        let descriptor_set_layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&descriptor_set_layout_binding_descs);
        unsafe { device.create_descriptor_set_layout(&descriptor_set_layout_info, None) }?
    };

    let sampler_descriptor_set_layout = {
        let descriptor_set_layout_binding_descs = [vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build()];
        let descriptor_set_layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&descriptor_set_layout_binding_descs);
        unsafe { device.create_descriptor_set_layout(&descriptor_set_layout_info, None) }?
    };

    let fft_descriptor_set_layout = {
        let descriptor_set_layout_binding_descs = [vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .build()];
        let descriptor_set_layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&descriptor_set_layout_binding_descs);
        unsafe { device.create_descriptor_set_layout(&descriptor_set_layout_info, None) }?
    };

    Ok(vec![
        descriptor_set_layout,
        sampler_descriptor_set_layout,
        fft_descriptor_set_layout,
    ])
}

fn compute_desc_set_leyout(device: &VkDevice) -> VkResult<Vec<vk::DescriptorSetLayout>> {
    let descriptor_set_layout = {
        let descriptor_set_layout_binding_descs = [
            vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .build(),
        ];
        let descriptor_set_layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&descriptor_set_layout_binding_descs);
        unsafe { device.create_descriptor_set_layout(&descriptor_set_layout_info, None) }?
    };

    let fft_descriptor_set_layout = {
        let descriptor_set_layout_binding_descs = [vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .build()];
        let descriptor_set_layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&descriptor_set_layout_binding_descs);
        unsafe { device.create_descriptor_set_layout(&descriptor_set_layout_info, None) }?
    };

    Ok(vec![descriptor_set_layout, fft_descriptor_set_layout])
}

pub struct ContiniousHashMap<K, V>(HashMap<K, HashSet<V>>);

impl<K, V> Deref for ContiniousHashMap<K, V> {
    type Target = HashMap<K, HashSet<V>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K, V> DerefMut for ContiniousHashMap<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<K: Eq + Hash, V: Eq + Hash> ContiniousHashMap<K, V> {
    fn push_value(&mut self, key: K, value: V) {
        self.0.entry(key).or_insert(HashSet::new()).insert(value);
    }
}

/// The main struct that holds all render primitives
///
/// Rust documentation states for FIFO drop order for struct fields.
/// Or in the other words it's the same order that they're declared.
pub struct PilkaRender<'a> {
    pub paused: bool,

    descriptor_pool: vk::DescriptorPool,
    pub descriptor_sets: Vec<vk::DescriptorSet>,
    pub descriptor_sets_compute: Vec<vk::DescriptorSet>,
    descriptor_set_layouts: Vec<vk::DescriptorSetLayout>,
    descriptor_set_layouts_compute: Vec<vk::DescriptorSetLayout>,

    fft_texture: FftTexture<'a>,

    previous_frame: VkTexture,
    generic_texture: VkTexture,
    dummy_texture: VkTexture,
    float_texture1: VkTexture,
    float_texture2: VkTexture,

    sampler: vk::Sampler,

    pub screenshot_ctx: ScreenshotCtx<'a>,
    pub push_constant: PushConstant,

    pub scissors: vk::Rect2D,
    pub viewport: vk::Viewport,
    pub resolution: vk::Extent2D,

    pub shader_set: ContiniousHashMap<PathBuf, usize>,
    pub compiler: shaderc::Compiler,

    pub rendering_complete_semaphore: vk::Semaphore,
    pub present_complete_semaphore: vk::Semaphore,
    pub command_pool: VkCommandPool,
    pub command_pool_transfer: VkCommandPool,

    pub pipeline_cache: vk::PipelineCache,
    pub pipelines: Vec<Pipeline>,
    pub render_pass: VkRenderPass,

    pub framebuffers: Vec<vk::Framebuffer>,
    pub swapchain: VkSwapchain,
    pub surface: VkSurface,

    pub device_properties: VkDeviceProperties,

    pub queues: VkQueues,
    pub device: VkDevice,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PushConstant {
    pub pos: [f32; 3],
    pub time: f32,
    pub wh: [f32; 2],
    pub mouse: [f32; 2],
    pub mouse_pressed: vk::Bool32,
    pub frame: u32,
    pub time_delta: f32,
    pub record_period: f32,
}

impl PushConstant {
    unsafe fn as_slice(&self) -> &[u8] {
        unsafe { any_as_u8_slice(self) }
    }
}

// TODO: Make proper ms -> sec converion
impl std::fmt::Display for PushConstant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "position:\t{:?}\ntime:\t\t{:.2}\ntime delta:\t{:.3} ms, fps: {:.2}\nwidth, height:\t{:?}\nmouse:\t\t{:.2?}\nframe:\t\t{}\nrecord_period:\t{}\n",
            self.pos, self.time, self.time_delta * 1000., 1. / self.time_delta,
            self.wh, self.mouse, self.frame, self.record_period
        )
    }
}

impl<'a> PilkaRender<'a> {
    pub fn get_device_name(&self) -> Result<&str, std::str::Utf8Error> {
        unsafe { CStr::from_ptr(self.device_properties.properties.device_name.as_ptr()) }.to_str()
    }
    pub fn get_device_type(&self) -> PhysicalDeviceType {
        self.device_properties.properties.device_type
    }
    pub fn get_vendor_name(&self) -> &str {
        match self.device_properties.properties.vendor_id {
            0x1002 => "AMD",
            0x1010 => "ImgTec",
            0x10DE => "NVIDIA Corporation",
            0x13B5 => "ARM",
            0x5143 => "Qualcomm",
            0x8086 => "INTEL Corporation",
            _ => "Unknown vendor",
        }
    }
    pub fn get_vulkan_version_name(&self) -> VkResult<String> {
        match self
            .device
            .instance
            .entry
            .try_enumerate_instance_version()?
        {
            Some(version) => {
                let major = version >> 22;
                let minor = (version >> 12) & 0x3ff;
                let patch = version & 0xfff;
                Ok(format!("{}.{}.{}", major, minor, patch))
            }
            None => Ok("1.0.0".to_string()),
        }
    }

    pub fn new<W: HasRawWindowHandle>(window: &W) -> Result<Self, Box<dyn std::error::Error>> {
        let validation_layers = if cfg!(debug_assertions) {
            vec!["VK_LAYER_KHRONOS_validation\0"]
        } else {
            vec![]
        };
        // let validation_layers = vec!["VK_LAYER_KHRONOS_validation\0"];
        let extention_names = ash_window::enumerate_required_extensions(window)?;
        let instance = VkInstance::new(&validation_layers, &extention_names)?;

        let surface = instance.create_surface(window)?;

        let (device, device_properties, queues) =
            instance.create_device_and_queues(Some(&surface))?;

        device.name_queue(queues.graphics_queue.queue, "Graphics Queue")?;
        device.name_queue(queues.transfer_queue.queue, "Transfer Queue")?;
        device.name_queue(queues.compute_queue.queue, "Compute Queue")?;

        let surface_resolution = surface.resolution(&device)?;

        let swapchain_loader = device.instance.create_swapchain_loader(&device);

        let swapchain = device.create_swapchain(swapchain_loader, &surface, &queues)?;

        let command_pool_transfer = device
            .create_vk_command_pool(queues.transfer_queue.index, swapchain.images.len() as u32)?;

        let mut command_pool = device
            .create_vk_command_pool(queues.graphics_queue.index, swapchain.images.len() as u32)?;
        for &image in &swapchain.images {
            command_pool.record_submit_commandbuffer(
                &device,
                queues.graphics_queue.queue,
                &[],
                &[],
                &[],
                |device, command_buffer| {
                    device.set_image_layout(
                        command_buffer,
                        image,
                        vk::ImageLayout::UNDEFINED,
                        vk::ImageLayout::PRESENT_SRC_KHR,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::TRANSFER,
                    );
                },
            )?;
        }

        let render_pass = device.create_vk_render_pass(swapchain.format())?;

        let present_complete_semaphore = device.create_semaphore()?;
        let rendering_complete_semaphore = device.create_semaphore()?;

        device.name_semaphore(present_complete_semaphore, "Present Compelete Semaphore")?;
        device.name_semaphore(rendering_complete_semaphore, "Render Complete Semaphore")?;

        let framebuffers = swapchain.create_framebuffers(
            (surface_resolution.width, surface_resolution.height),
            &render_pass,
            &device,
        )?;

        let (viewport, scissors, extent) = {
            let surface_resolution = surface.resolution(&device)?;
            (
                vk::Viewport {
                    x: 0.0,
                    y: surface_resolution.height as f32,
                    width: surface_resolution.width as f32,
                    height: -(surface_resolution.height as f32),
                    min_depth: 0.0,
                    max_depth: 1.0,
                },
                vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: surface_resolution,
                },
                surface_resolution,
            )
        };

        let compiler = shaderc::Compiler::new().unwrap();

        let push_constant = PushConstant {
            pos: [0.; 3],
            wh: surface.resolution_slice(&device)?,
            mouse: [0.; 2],
            time: 0.,
            time_delta: 0.166666,
            mouse_pressed: false as _,
            frame: 0,
            record_period: 10.,
        };

        let pipeline_cache_create_info = vk::PipelineCacheCreateInfo::builder();
        let pipeline_cache =
            unsafe { device.create_pipeline_cache(&pipeline_cache_create_info, None) }?;

        let mut need2steps = false;
        let format_props = unsafe {
            device
                .instance
                .get_physical_device_format_properties(device.physical_device, swapchain.format)
        };
        let blit_linear = format_props
            .linear_tiling_features
            .contains(vk::FormatFeatureFlags::BLIT_DST);
        let blit_optimal = format_props
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::BLIT_DST);
        if !blit_linear && blit_optimal {
            need2steps = true
        }
        let screenshot_ctx = ScreenshotCtx::init(
            &device,
            &device_properties.memory,
            &command_pool,
            extent,
            swapchain.format,
            need2steps,
        )?;

        let fft_texture = FftTexture::new(&device, &device_properties, &command_pool_transfer)?;
        let screen_sized_texture = |format| -> VkResult<VkTexture> {
            let extent = vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            };
            let image_create_info = vk::ImageCreateInfo::builder()
                .format(format)
                .image_type(vk::ImageType::TYPE_2D)
                .extent(extent)
                .array_layers(1)
                .mip_levels(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(
                    vk::ImageUsageFlags::TRANSFER_DST
                        | vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::STORAGE,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let image_memory_flags = vk::MemoryPropertyFlags::DEVICE_LOCAL;

            let sampler_create_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .address_mode_w(vk::SamplerAddressMode::REPEAT)
                .anisotropy_enable(false)
                .max_anisotropy(0.);

            VkTexture::new(
                &device,
                &device_properties.memory,
                &image_create_info,
                image_memory_flags,
                &sampler_create_info,
            )
        };
        let previous_frame = screen_sized_texture(vk::Format::R8G8B8A8_UNORM)?;
        let generic_texture = screen_sized_texture(vk::Format::R8G8B8A8_UNORM)?;
        let dummy_texture = screen_sized_texture(vk::Format::R8G8B8A8_UNORM)?;
        let float_texture1 = screen_sized_texture(vk::Format::R32G32B32A32_SFLOAT)?;
        let float_texture2 = screen_sized_texture(vk::Format::R32G32B32A32_SFLOAT)?;

        let sampler = {
            let sampler_create_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .address_mode_w(vk::SamplerAddressMode::REPEAT)
                .anisotropy_enable(false)
                .max_anisotropy(0.);
            unsafe { device.create_sampler(&sampler_create_info, None) }?
        };

        device.name_image(previous_frame.image.image, "Previous Frame Texture")?;
        device.name_image(generic_texture.image.image, "Generic Texture")?;
        device.name_image(dummy_texture.image.image, "Dummy Texture")?;
        device.name_image(float_texture1.image.image, "Float Texture 1")?;
        device.name_image(float_texture2.image.image, "Float Texture 2")?;
        device.name_image(fft_texture.texture.image.image, "FFT Texture")?;
        {
            let images = [
                previous_frame.image.image,
                fft_texture.texture.image.image,
                generic_texture.image.image,
                dummy_texture.image.image,
                float_texture1.image.image,
                float_texture2.image.image,
            ];
            command_pool.record_submit_commandbuffer(
                &device,
                queues.graphics_queue.queue,
                &[],
                &[],
                &[],
                |device, command_buffer| {
                    for &image in &images {
                        device.set_image_layout(
                            command_buffer,
                            image,
                            vk::ImageLayout::UNDEFINED,
                            vk::ImageLayout::GENERAL,
                            vk::PipelineStageFlags::TRANSFER,
                            vk::PipelineStageFlags::TRANSFER,
                        );
                    }
                },
            )?;
        }

        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLED_IMAGE,
                descriptor_count: 24,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                descriptor_count: 16,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLER,
                descriptor_count: 16,
            },
        ];
        let descriptor_pool_info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets(8)
            .pool_sizes(&pool_sizes);
        let descriptor_pool =
            unsafe { device.create_descriptor_pool(&descriptor_pool_info, None) }?;

        let descriptor_set_layouts_graphics = graphics_desc_set_leyout(&device)?;
        let descriptor_sets = {
            let descriptor_set_allocate_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&descriptor_set_layouts_graphics);
            unsafe { device.allocate_descriptor_sets(&descriptor_set_allocate_info) }?
        };

        let descriptor_set_layouts_compute = compute_desc_set_leyout(&device)?;
        let descriptor_sets_compute = {
            let descriptor_set_allocate_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&descriptor_set_layouts_compute);
            unsafe { device.allocate_descriptor_sets(&descriptor_set_allocate_info) }?
        };

        let image_infos: &[&[vk::DescriptorImageInfo]] = &[
            &[
                vk::DescriptorImageInfo {
                    image_layout: vk::ImageLayout::GENERAL,
                    image_view: previous_frame.image_view,
                    sampler: previous_frame.sampler,
                },
                vk::DescriptorImageInfo {
                    image_layout: vk::ImageLayout::GENERAL,
                    image_view: generic_texture.image_view,
                    sampler: generic_texture.sampler,
                },
                vk::DescriptorImageInfo {
                    image_layout: vk::ImageLayout::GENERAL,
                    image_view: dummy_texture.image_view,
                    sampler: dummy_texture.sampler,
                },
                vk::DescriptorImageInfo {
                    image_layout: vk::ImageLayout::GENERAL,
                    image_view: float_texture1.image_view,
                    sampler: float_texture1.sampler,
                },
                vk::DescriptorImageInfo {
                    image_layout: vk::ImageLayout::GENERAL,
                    image_view: float_texture2.image_view,
                    sampler: float_texture2.sampler,
                },
            ],
            &[vk::DescriptorImageInfo {
                sampler,
                ..Default::default()
            }],
            &[vk::DescriptorImageInfo {
                image_layout: vk::ImageLayout::GENERAL,
                image_view: fft_texture.texture.image_view,
                sampler: fft_texture.texture.sampler,
            }],
        ];
        let desc_types = &[
            vk::DescriptorType::SAMPLED_IMAGE,
            vk::DescriptorType::SAMPLER,
            vk::DescriptorType::SAMPLED_IMAGE,
        ];

        for (descset, (image_infos, &desc_type)) in descriptor_sets
            .iter()
            .zip(image_infos.iter().zip(desc_types))
        {
            for (i, image_info) in image_infos.iter().enumerate() {
                let desc_sets_write = [vk::WriteDescriptorSet::builder()
                    .dst_set(*descset)
                    .dst_binding(i as _)
                    .dst_array_element(0)
                    .descriptor_type(desc_type)
                    .image_info(&[*image_info])
                    .build()];
                unsafe { device.update_descriptor_sets(&desc_sets_write, &[]) };
            }
        }

        let image_infos = &[image_infos[0], image_infos[2]];
        let desc_types = &[
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::STORAGE_IMAGE,
        ];

        for (descset, (image_infos, &desc_type)) in descriptor_sets_compute
            .iter()
            .zip(image_infos.iter().zip(desc_types))
        {
            for (i, image_info) in image_infos.iter().enumerate() {
                let desc_sets_write = [vk::WriteDescriptorSet::builder()
                    .dst_set(*descset)
                    .dst_binding(i as _)
                    .dst_array_element(0)
                    .descriptor_type(desc_type)
                    .image_info(&[*image_info])
                    .build()];
                unsafe { device.update_descriptor_sets(&desc_sets_write, &[]) };
            }
        }

        Ok(Self {
            paused: false,

            device,
            queues,

            device_properties,

            surface,
            swapchain,
            framebuffers,

            render_pass,
            pipelines: vec![],
            pipeline_cache,

            command_pool_transfer,
            command_pool,
            present_complete_semaphore,
            rendering_complete_semaphore,

            shader_set: ContiniousHashMap(HashMap::new()),
            compiler,

            viewport,
            scissors,
            resolution: extent,

            push_constant,
            screenshot_ctx,

            sampler,

            float_texture1,
            float_texture2,
            previous_frame,
            generic_texture,
            dummy_texture,

            fft_texture,

            descriptor_pool,
            descriptor_sets,
            descriptor_sets_compute,
            descriptor_set_layouts: descriptor_set_layouts_graphics,
            descriptor_set_layouts_compute,
        })
    }

    pub fn render(&mut self) -> VkResult<()> {
        let (present_index, is_suboptimal) = match unsafe {
            self.swapchain.swapchain_loader.acquire_next_image(
                self.swapchain.swapchain,
                std::u64::MAX,
                self.present_complete_semaphore,
                vk::Fence::null(),
            )
        } {
            Ok((index, check)) => (index, check),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) | Err(vk::Result::SUBOPTIMAL_KHR) => {
                println!("Oooopsie~ Get out-of-date swapchain in first time");
                self.resize()?;
                return Ok(());
            }
            Err(e) => panic!("{}", e),
        };
        if is_suboptimal {
            self.resize()?;
        }

        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 1.0, 0.0],
            },
        }];

        let viewport = self.viewport;
        let scissors = self.scissors;
        let push_constant = self.push_constant;
        let descriptor_sets = &self.descriptor_sets;
        let present_image = self.swapchain.images[present_index as usize];
        let prev_frame = self.previous_frame.image.image;
        let extent = vk::Extent3D {
            width: self.resolution.width,
            height: self.resolution.height,
            depth: 1,
        };

        let mut executed_compute_semaphores = vec![];

        unsafe { self.device.queue_wait_idle(self.queues.compute_queue.queue) }?;

        for undefined_pipeline in &self.pipelines {
            match undefined_pipeline {
                Pipeline::Compute(ref pipeline) => {
                    let cmd_buf = pipeline.command_buffer;
                    unsafe {
                        self.device.reset_command_buffer(
                            cmd_buf,
                            vk::CommandBufferResetFlags::RELEASE_RESOURCES,
                        )
                    }?;
                    unsafe {
                        let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
                            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
                        self.device
                            .begin_command_buffer(cmd_buf, &command_buffer_begin_info)?;

                        if self.paused {
                            self.device.blit_image(
                                cmd_buf,
                                present_image,
                                vk::ImageLayout::PRESENT_SRC_KHR,
                                prev_frame,
                                vk::ImageLayout::GENERAL,
                                extent,
                            );

                            self.device.cmd_bind_pipeline(
                                cmd_buf,
                                vk::PipelineBindPoint::COMPUTE,
                                pipeline.pipeline,
                            );
                            self.device.cmd_push_constants(
                                cmd_buf,
                                pipeline.pipeline_layout,
                                vk::ShaderStageFlags::COMPUTE,
                                0,
                                push_constant.as_slice(),
                            );
                            self.device.cmd_bind_descriptor_sets(
                                cmd_buf,
                                vk::PipelineBindPoint::COMPUTE,
                                pipeline.pipeline_layout,
                                0,
                                &self.descriptor_sets_compute,
                                &[],
                            );

                            const ALIGN: u32 = 16;
                            self.device.cmd_dispatch(
                                cmd_buf,
                                return_aligned(extent.width, ALIGN) / ALIGN,
                                return_aligned(extent.height, ALIGN) / ALIGN,
                                1,
                            );
                        }
                        self.device.end_command_buffer(cmd_buf)?;

                        let command_buffers = [cmd_buf];
                        let wait_semaphores = [];
                        let signal_semaphores = [pipeline.semaphore];
                        let compute_submit_info = [vk::SubmitInfo::builder()
                            .command_buffers(&command_buffers)
                            .wait_dst_stage_mask(&[vk::PipelineStageFlags::COMPUTE_SHADER])
                            .wait_semaphores(&wait_semaphores)
                            .signal_semaphores(&signal_semaphores)
                            .build()];
                        self.device.queue_submit(
                            self.queues.compute_queue.queue,
                            &compute_submit_info,
                            vk::Fence::null(),
                        )?;

                        executed_compute_semaphores.push(pipeline.semaphore);
                    }
                }
                Pipeline::Graphics(ref pipeline) => {
                    let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
                        .render_pass(*self.render_pass)
                        .framebuffer(self.framebuffers[present_index as usize])
                        .render_area(vk::Rect2D {
                            offset: vk::Offset2D { x: 0, y: 0 },
                            extent: self.surface.resolution(&self.device)?,
                        })
                        .clear_values(&clear_values);

                    let pipeline_layout = pipeline.pipeline_layout;
                    let wait_mask = &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
                    // Start command queue
                    unsafe {
                        self.command_pool.record_submit_commandbuffer(
                            &self.device,
                            self.queues.graphics_queue.queue,
                            wait_mask,
                            &[
                                executed_compute_semaphores.as_slice(),
                                &[self.present_complete_semaphore],
                            ]
                            .concat(),
                            &[self.rendering_complete_semaphore],
                            |device, draw_command_buffer| {
                                device.set_image_layout(
                                    draw_command_buffer,
                                    prev_frame,
                                    vk::ImageLayout::GENERAL,
                                    vk::ImageLayout::GENERAL,
                                    vk::PipelineStageFlags::COMPUTE_SHADER,
                                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                                );

                                device.cmd_begin_render_pass(
                                    draw_command_buffer,
                                    &render_pass_begin_info,
                                    vk::SubpassContents::INLINE,
                                );
                                device.cmd_bind_pipeline(
                                    draw_command_buffer,
                                    vk::PipelineBindPoint::GRAPHICS,
                                    pipeline.pipeline,
                                );
                                device.cmd_set_viewport(draw_command_buffer, 0, &[viewport]);
                                device.cmd_set_scissor(draw_command_buffer, 0, &[scissors]);
                                device.cmd_bind_descriptor_sets(
                                    draw_command_buffer,
                                    vk::PipelineBindPoint::GRAPHICS,
                                    pipeline.pipeline_layout,
                                    0,
                                    descriptor_sets,
                                    &[],
                                );

                                device.cmd_push_constants(
                                    draw_command_buffer,
                                    pipeline_layout,
                                    vk::ShaderStageFlags::ALL_GRAPHICS,
                                    0,
                                    push_constant.as_slice(),
                                );

                                // Or draw without the index buffer
                                device.cmd_draw(draw_command_buffer, 3, 1, 0, 0);
                                device.cmd_end_render_pass(draw_command_buffer);

                                executed_compute_semaphores.clear();
                            },
                        )?;
                    }
                }
            }
        }

        let wait_semaphores = [
            self.rendering_complete_semaphore,
            self.present_complete_semaphore,
        ];
        let swapchains = [self.swapchain.swapchain];
        let image_indices = [present_index];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        match unsafe {
            self.swapchain
                .swapchain_loader
                .queue_present(self.queues.graphics_queue.queue, &present_info)
        } {
            Ok(is_suboptimal) if is_suboptimal => {
                self.resize()?;
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) | Err(vk::Result::SUBOPTIMAL_KHR) => {
                self.resize()?;
            }
            Ok(_) => {}
            Err(e) => panic!("Unexpected error on presenting image: {}", e),
        }

        self.push_constant.frame += 1;

        Ok(())
    }

    // TODO(#17): Don't use `device_wait_idle` for resizing
    //
    // Probably Very bad! Consider waiting for approciate command buffers and fences
    // (i have no much choice of them) or restrict the amount of resizing events.
    pub fn resize(&mut self) -> VkResult<()> {
        unsafe { self.device.device_wait_idle() }?;

        self.resolution = self.surface.resolution(&self.device)?;
        let vk::Extent2D { width, height } = self.resolution;

        self.viewport = vk::Viewport {
            x: 0.,
            y: height as f32,
            width: width as f32,
            height: -(height as f32),
            min_depth: 0.0,
            max_depth: 1.0,
        };
        self.scissors = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D { width, height },
        };

        self.swapchain
            .recreate_swapchain((width, height), &self.device)?;

        for &framebuffer in &self.framebuffers {
            unsafe { self.device.destroy_framebuffer(framebuffer, None) };
        }
        for (framebuffer, present_image) in self
            .framebuffers
            .iter_mut()
            .zip(&self.swapchain.image_views)
        {
            let new_framebuffer = VkSwapchain::create_framebuffer(
                &[*present_image],
                (width, height),
                &self.render_pass,
                &self.device,
            )?;

            *framebuffer = new_framebuffer;
        }

        for &image in &self.swapchain.images {
            self.command_pool.record_submit_commandbuffer(
                &self.device,
                self.queues.graphics_queue.queue,
                &[],
                &[],
                &[],
                |device, command_buffer| {
                    device.set_image_layout(
                        command_buffer,
                        image,
                        vk::ImageLayout::UNDEFINED,
                        vk::ImageLayout::PRESENT_SRC_KHR,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::TRANSFER,
                    );
                },
            )?;
        }

        self.previous_frame
            .resize(&self.device, &self.device_properties.memory, width, height)?;
        self.generic_texture
            .resize(&self.device, &self.device_properties.memory, width, height)?;
        self.dummy_texture
            .resize(&self.device, &self.device_properties.memory, width, height)?;
        self.float_texture1
            .resize(&self.device, &self.device_properties.memory, width, height)?;
        self.float_texture2
            .resize(&self.device, &self.device_properties.memory, width, height)?;

        self.device
            .name_image(self.previous_frame.image.image, "Previous Frame Texture")?;
        self.device
            .name_image(self.generic_texture.image.image, "Generic Texture")?;
        self.device
            .name_image(self.dummy_texture.image.image, "Dummy Texture")?;
        self.device
            .name_image(self.float_texture1.image.image, "Float Texture 1")?;
        self.device
            .name_image(self.float_texture2.image.image, "Float Texture 2")?;
        {
            let images = [
                self.previous_frame.image.image,
                self.generic_texture.image.image,
                self.dummy_texture.image.image,
                self.float_texture1.image.image,
                self.float_texture2.image.image,
            ];
            self.command_pool.record_submit_commandbuffer(
                &self.device,
                self.queues.graphics_queue.queue,
                &[],
                &[],
                &[],
                |device, command_buffer| {
                    for &image in &images {
                        device.set_image_layout(
                            command_buffer,
                            image,
                            vk::ImageLayout::UNDEFINED,
                            vk::ImageLayout::GENERAL,
                            vk::PipelineStageFlags::TRANSFER,
                            vk::PipelineStageFlags::TRANSFER,
                        );
                    }
                },
            )?;
        }

        let image_infos = [
            vk::DescriptorImageInfo {
                image_layout: vk::ImageLayout::GENERAL,
                image_view: self.previous_frame.image_view,
                sampler: self.previous_frame.sampler,
            },
            vk::DescriptorImageInfo {
                image_layout: vk::ImageLayout::GENERAL,
                image_view: self.generic_texture.image_view,
                sampler: self.generic_texture.sampler,
            },
            vk::DescriptorImageInfo {
                image_layout: vk::ImageLayout::GENERAL,
                image_view: self.dummy_texture.image_view,
                sampler: self.dummy_texture.sampler,
            },
            vk::DescriptorImageInfo {
                image_layout: vk::ImageLayout::GENERAL,
                image_view: self.float_texture1.image_view,
                sampler: self.float_texture1.sampler,
            },
            vk::DescriptorImageInfo {
                image_layout: vk::ImageLayout::GENERAL,
                image_view: self.float_texture2.image_view,
                sampler: self.float_texture2.sampler,
            },
        ];
        let desc_types = &[vk::DescriptorType::SAMPLED_IMAGE];

        for (descset, &desc_type) in self.descriptor_sets.iter().zip(desc_types) {
            for (i, image_info) in image_infos.iter().enumerate() {
                let desc_sets_write = [vk::WriteDescriptorSet::builder()
                    .dst_set(*descset)
                    .dst_binding(i as _)
                    .dst_array_element(0)
                    .descriptor_type(desc_type)
                    .image_info(&[*image_info])
                    .build()];
                unsafe { self.device.update_descriptor_sets(&desc_sets_write, &[]) };
            }
        }

        let desc_types = &[vk::DescriptorType::STORAGE_IMAGE];

        for (descset, &desc_type) in self.descriptor_sets_compute.iter().zip(desc_types) {
            for (i, image_info) in image_infos.iter().enumerate() {
                let desc_sets_write = [vk::WriteDescriptorSet::builder()
                    .dst_set(*descset)
                    .dst_binding(i as _)
                    .dst_array_element(0)
                    .descriptor_type(desc_type)
                    .image_info(&[*image_info])
                    .build()];
                unsafe { self.device.update_descriptor_sets(&desc_sets_write, &[]) };
            }
        }

        let extent = Extent3D {
            width,
            height,
            depth: 1,
        };
        self.screenshot_ctx
            .realloc(&self.device, &self.device_properties, extent)?;

        Ok(())
    }

    pub fn push_compute_pipeline(
        &mut self,
        comp_info: ShaderInfo,
        dependencies: &[PathBuf],
    ) -> VkResult<()> {
        let pipeline_number = self.pipelines.len();
        self.shader_set
            .push_value(comp_info.name.canonicalize().unwrap(), pipeline_number);
        for deps in dependencies {
            self.shader_set
                .push_value(deps.canonicalize().unwrap(), pipeline_number);
        }

        let new_pipeline = self.make_pipeline_from_shaders(&ShaderSet::Compute(comp_info))?;
        self.pipelines.push(new_pipeline);

        Ok(())
    }

    pub fn push_render_pipeline(
        &mut self,
        vert_info: ShaderInfo,
        frag_info: ShaderInfo,
        dependencies: &[PathBuf],
    ) -> VkResult<()> {
        let pipeline_number = self.pipelines.len();
        self.shader_set
            .push_value(vert_info.name.canonicalize().unwrap(), pipeline_number);
        self.shader_set
            .push_value(frag_info.name.canonicalize().unwrap(), pipeline_number);
        for deps in dependencies {
            self.shader_set
                .push_value(deps.canonicalize().unwrap(), pipeline_number);
        }

        let new_pipeline = self.make_pipeline_from_shaders(&ShaderSet::Graphics {
            vert: vert_info,
            frag: frag_info,
        })?;
        self.pipelines.push(new_pipeline);

        Ok(())
    }

    pub fn make_pipeline_from_shaders(&mut self, shader_set: &ShaderSet) -> VkResult<Pipeline> {
        match shader_set {
            ShaderSet::Graphics {
                vert: vert_info,
                frag: frag_info,
            } => {
                let vert_module = create_shader_module(
                    vert_info,
                    shaderc::ShaderKind::Vertex,
                    &mut self.compiler,
                    &self.device,
                )?;
                let frag_module = match create_shader_module(
                    frag_info,
                    shaderc::ShaderKind::Fragment,
                    &mut self.compiler,
                    &self.device,
                ) {
                    Ok(module) => module,
                    Err(e) => {
                        unsafe { self.device.destroy_shader_module(vert_module, None) };
                        return Err(e);
                    }
                };
                let shader_set = Box::new([
                    vk::PipelineShaderStageCreateInfo {
                        module: vert_module,
                        p_name: vert_info.entry_point.as_ptr(),
                        stage: vk::ShaderStageFlags::VERTEX,
                        ..Default::default()
                    },
                    vk::PipelineShaderStageCreateInfo {
                        module: frag_module,
                        p_name: frag_info.entry_point.as_ptr(),
                        stage: vk::ShaderStageFlags::FRAGMENT,
                        ..Default::default()
                    },
                ]);

                let new_pipeline = self.new_graphics_pipeline(
                    self.pipeline_cache,
                    shader_set,
                    vert_info,
                    frag_info,
                )?;

                unsafe {
                    self.device.destroy_shader_module(vert_module, None);
                    self.device.destroy_shader_module(frag_module, None);
                }

                Ok(Pipeline::Graphics(new_pipeline))
            }
            ShaderSet::Compute(comp_info) => {
                let comp_module = create_shader_module(
                    comp_info,
                    shaderc::ShaderKind::Compute,
                    &mut self.compiler,
                    &self.device,
                )?;

                let shader_stage = vk::PipelineShaderStageCreateInfo {
                    module: comp_module,
                    p_name: comp_info.entry_point.as_ptr(),
                    stage: vk::ShaderStageFlags::COMPUTE,
                    ..Default::default()
                };
                let new_pipeline = self.new_compute_pipeline(shader_stage, comp_info)?;
                self.device
                    .name_semaphore(new_pipeline.semaphore, "Compute Semaphore")?;

                unsafe {
                    self.device.destroy_shader_module(comp_module, None);
                }

                Ok(Pipeline::Compute(new_pipeline))
            }
        }
    }

    pub fn new_graphics_pipeline(
        &self,
        pipeline_cache: vk::PipelineCache,
        shader_set: Box<[vk::PipelineShaderStageCreateInfo]>,
        vs_info: &ShaderInfo,
        fs_info: &ShaderInfo,
    ) -> VkResult<VkGraphicsPipeline> {
        let device = self.device.device.clone();
        let (pipeline_layout, descriptor_set_layout) = self.create_graphics_pipeline_layout()?;

        let desc = PipelineDescriptor::new(shader_set);

        VkGraphicsPipeline::new(
            pipeline_cache,
            pipeline_layout,
            descriptor_set_layout,
            desc,
            &self.render_pass,
            vs_info.clone(),
            fs_info.clone(),
            device,
        )
    }

    pub fn new_compute_pipeline(
        &self,
        shader_set: vk::PipelineShaderStageCreateInfo,
        cs_info: &ShaderInfo,
    ) -> VkResult<VkComputePipeline> {
        let device = self.device.device.clone();
        let (pipeline_layout, descriptor_set_layout) = self.create_compute_pipeline_layout()?;

        VkComputePipeline::new(
            pipeline_layout,
            descriptor_set_layout,
            shader_set,
            cs_info.clone(),
            device,
            &self.queues,
        )
    }

    pub fn rebuild_pipelines(&mut self, paths: &[PathBuf]) -> VkResult<()> {
        unsafe { self.device.device_wait_idle() }.unwrap();
        for path in paths {
            if let Some(pipeline_indexes) = self.shader_set.get(path) {
                for pipeline_index in pipeline_indexes.clone() {
                    match self.rebuild_pipeline(pipeline_index) {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Shader compilation error:\n{}", e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn rebuild_pipeline(&mut self, index: usize) -> VkResult<()> {
        let shader_set = {
            let current_pipeline = &self.pipelines[index];
            match current_pipeline {
                Pipeline::Graphics(pipeline) => ShaderSet::Graphics {
                    vert: pipeline.vs_info.clone(),
                    frag: pipeline.fs_info.clone(),
                },
                Pipeline::Compute(pipeline) => ShaderSet::Compute(pipeline.cs_info.clone()),
            }
        };
        let new_pipeline = match self.make_pipeline_from_shaders(&shader_set) {
            Ok(res) => {
                const ESC: &str = "\x1B[";
                const RESET: &str = "\x1B[0m";
                eprint!("\r{}42m{}K{}\r", ESC, ESC, RESET);
                std::io::stdout().flush().unwrap();
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    eprint!("\r{}40m{}K{}\r", ESC, ESC, RESET);
                    std::io::stdout().flush().unwrap();
                });
                res
            }
            Err(ash::vk::Result::ERROR_UNKNOWN) => return Ok(()),
            Err(e) => return Err(e),
        };
        self.pipelines[index] = new_pipeline;

        Ok(())
    }

    pub fn create_graphics_pipeline_layout(
        &self,
    ) -> VkResult<(vk::PipelineLayout, Vec<vk::DescriptorSetLayout>)> {
        let push_constant_ranges = [vk::PushConstantRange::builder()
            .offset(0)
            .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
            .size(std::mem::size_of::<PushConstant>() as u32)
            .build()];

        let descriptor_set_layouts = graphics_desc_set_leyout(&self.device)?;

        let layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .push_constant_ranges(&push_constant_ranges)
            .set_layouts(&descriptor_set_layouts)
            .build();
        let pipeline_layout = unsafe {
            self.device
                .create_pipeline_layout(&layout_create_info, None)
        }?;

        Ok((pipeline_layout, descriptor_set_layouts))
    }

    pub fn create_compute_pipeline_layout(
        &self,
    ) -> VkResult<(vk::PipelineLayout, Vec<vk::DescriptorSetLayout>)> {
        let push_constant_ranges = [vk::PushConstantRange::builder()
            .offset(0)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .size(std::mem::size_of::<PushConstant>() as u32)
            .build()];

        let descriptor_set_layouts = compute_desc_set_leyout(&self.device)?;

        let layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .push_constant_ranges(&push_constant_ranges)
            .set_layouts(&descriptor_set_layouts)
            .build();
        let pipeline_layout = unsafe {
            self.device
                .create_pipeline_layout(&layout_create_info, None)
        }?;

        Ok((pipeline_layout, descriptor_set_layouts))
    }

    pub fn capture_frame(&mut self) -> VkResult<Frame> {
        let present_image = self.swapchain.images[self.command_pool.active_command];
        let queue = &self.queues.graphics_queue;
        self.screenshot_ctx.capture_frame(
            &self.device,
            &self.device_properties,
            present_image,
            queue,
        )
    }

    #[allow(dead_code)]
    pub fn update_fft_texture(&mut self, data: &[f32]) -> VkResult<()> {
        self.fft_texture
            .update(data, &self.device, &self.queues.transfer_queue)
    }

    pub fn screenshot_layout(&self) -> SubresourceLayout {
        self.screenshot_ctx.image_dimentions(&self.device).0
    }
    pub fn screenshot_dimentions(&self) -> ImageDimentions {
        self.screenshot_ctx.image_dimentions(&self.device).1
    }
}

impl<'a> Drop for PilkaRender<'a> {
    fn drop(&mut self) {
        unsafe {
            for layout in &self.descriptor_set_layouts {
                self.device.destroy_descriptor_set_layout(*layout, None);
            }
            for layout in &self.descriptor_set_layouts_compute {
                self.device.destroy_descriptor_set_layout(*layout, None);
            }
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);

            self.fft_texture.destroy(&self.device);

            self.device.destroy_sampler(self.sampler, None);

            self.float_texture1.destroy(&self.device);
            self.float_texture2.destroy(&self.device);
            self.previous_frame.destroy(&self.device);
            self.generic_texture.destroy(&self.device);
            self.dummy_texture.destroy(&self.device);

            self.screenshot_ctx.destroy(&self.device);
            self.device
                .destroy_pipeline_cache(self.pipeline_cache, None);

            self.device
                .destroy_semaphore(self.present_complete_semaphore, None);
            self.device
                .destroy_semaphore(self.rendering_complete_semaphore, None);

            for &framebuffer in &self.framebuffers {
                self.device.destroy_framebuffer(framebuffer, None);
            }
        }
    }
}
