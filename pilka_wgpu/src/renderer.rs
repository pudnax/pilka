mod blitter;
mod screenshot;

use blitter::Blitter;
use screenshot::ScreenshotCtx;

use std::{fmt::Display, ops::Index, path::PathBuf};

use color_eyre::Result;
use pilka_types::{
    dispatch_optimal_size, ContiniousHashMap, Frame, ImageDimentions, PushConstant,
    ShaderCreateInfo, Uniform,
};
use pollster::block_on;
use raw_window_handle::HasRawWindowHandle;
use wgpu::{
    Adapter, BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, ComputePipeline, Device,
    PrimitiveState, PrimitiveTopology, Queue, RenderPipeline, Surface, Texture, TextureFormat,
    TextureView,
};

pub(crate) const SUBGROUP_SIZE: u32 = 16;
const NUM_SAMPLES: u32 = 4;

pub enum Pipeline {
    Render(RenderPipeline),
    Compute(ComputePipeline),
}

enum Binding {
    Uniform,
    Texture,
    Sampler,
}

#[derive(Debug)]
struct RenderPipelineLayoutInfo<const N: usize>([BindGroupLayout; N]);
impl<'a> RenderPipelineLayoutInfo<3> {
    const DESC: [BindGroupLayoutDescriptor<'a>; 3] = [
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture Render Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
            ],
        },
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Sampler Render Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            }],
        },
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform Render Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        },
        // wgpu::BindGroupLayoutDescriptor {
        //     label: Some("Fft Texture Bind Group Layout"),
        //     entries: &[wgpu::BindGroupLayoutEntry {
        //         binding: 0,
        //         visibility: wgpu::ShaderStages::FRAGMENT,
        //         ty: wgpu::BindingType::Texture {
        //             multisampled: false,
        //             sample_type: wgpu::TextureSampleType::Float { filterable: true },
        //             view_dimension: wgpu::TextureViewDimension::D2,
        //         },
        //         count: None,
        //     }],
        // },
    ];

    pub fn new(device: &Device) -> Self {
        Self(Self::DESC.map(|x| device.create_bind_group_layout(&x)))
    }
}

impl<const N: usize> Index<Binding> for RenderPipelineLayoutInfo<N> {
    type Output = BindGroupLayout;

    fn index(&self, index: Binding) -> &Self::Output {
        match index {
            Binding::Texture => &self.0[0],
            Binding::Sampler => &self.0[1],
            Binding::Uniform => &self.0[2],
        }
    }
}

#[derive(Debug)]
struct ComputePipelineLayoutInfo<const N: usize>([BindGroupLayout; N]);
impl<'a> ComputePipelineLayoutInfo<2> {
    const DESC: [BindGroupLayoutDescriptor<'a>; 2] = [
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture Compute Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::ReadWrite,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::ReadWrite,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::ReadWrite,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::ReadWrite,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::ReadWrite,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        },
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform Compute Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        },
        // wgpu::BindGroupLayoutDescriptor {
        //     label: Some("fft Texture Bind Group Layout"),
        //     entries: &[wgpu::BindGroupLayoutEntry {
        //         binding: 0,
        //         visibility: wgpu::ShaderStages::COMPUTE,
        //         ty: wgpu::BindingType::StorageTexture {
        //             access: wgpu::StorageTextureAccess::ReadWrite,
        //             format: wgpu::TextureFormat::Rgba32Float,
        //             view_dimension: wgpu::TextureViewDimension::D2,
        //         },
        //         count: None,
        //     }],
        // },
    ];

    pub fn new(device: &Device) -> Self {
        Self(Self::DESC.map(|x| device.create_bind_group_layout(&x)))
    }
}

impl<const N: usize> Index<Binding> for ComputePipelineLayoutInfo<N> {
    type Output = BindGroupLayout;

    fn index(&self, index: Binding) -> &Self::Output {
        match index {
            Binding::Texture => &self.0[0],
            Binding::Uniform => &self.0[1],
            _ => panic!(),
        }
    }
}

// impl<const N: usize> Index<Binding> for [BindGroupLayout; N] {
//     type Output = BindGroupLayout;

//     fn index(&self, index: Binding) -> &Self::Output {
//         match index {
//             Binding::Uniform => &self[0],
//             Binding::Texture => &self[1],
//             Binding::Sampler => &self[2],
//         }
//     }
// }

fn create_textures(
    device: &Device,
    extent: wgpu::Extent3d,
) -> (Vec<Texture>, Vec<TextureView>, BindGroup, BindGroup) {
    puffin::profile_function!();
    let make_texture = |label, format| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: extent,
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
        })
    };
    let textures: Vec<_> = [
        ("Previous Frame Texture", wgpu::TextureFormat::Rgba8Unorm),
        ("Generic Frame Texture", wgpu::TextureFormat::Rgba8Unorm),
        ("Dummy Frame Texture", wgpu::TextureFormat::Rgba8Unorm),
        ("Float Texture 1", wgpu::TextureFormat::Rgba32Float),
        ("Float Texture 2", wgpu::TextureFormat::Rgba32Float),
    ]
    .iter()
    .map(|(label, format)| make_texture(label, *format))
    .collect();

    let texture_views: Vec<_> = textures
        .iter()
        .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
        .collect();

    let entries: Vec<_> = texture_views
        .iter()
        .enumerate()
        .map(|(i, view)| wgpu::BindGroupEntry {
            binding: i as _,
            resource: wgpu::BindingResource::TextureView(view),
        })
        .collect();

    let sampled_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Render Bind Group"),
        layout: &RenderPipelineLayoutInfo::new(device)[Binding::Texture],
        entries: &entries,
    });

    let storage_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Compute Bind Group"),
        layout: &ComputePipelineLayoutInfo::new(device)[Binding::Texture],
        entries: &entries,
    });

    (
        textures,
        texture_views,
        sampled_texture_bind_group,
        storage_texture_bind_group,
    )
}

pub struct WgpuRender {
    adapter: Adapter,
    pub device: Device,
    pub surface: Surface,
    surface_config: wgpu::SurfaceConfiguration,
    queue: Queue,
    pipelines: Vec<Pipeline>,
    pub shader_set: ContiniousHashMap<PathBuf, usize>,
    format: TextureFormat,
    push_constant_ranges: u32,

    extent: wgpu::Extent3d,

    textures: Vec<Texture>,
    texture_views: Vec<TextureView>,

    sampled_texture_bind_group: BindGroup,
    storage_texture_bind_group: BindGroup,
    sampler_bind_group: BindGroup,

    blitter: Blitter,
    screenshot_ctx: ScreenshotCtx,

    multisampled_framebuffer: wgpu::TextureView,
    smaa_target: smaa::SmaaTarget,

    pub paused: bool,

    uniform: Uniform,
    uniform_buffer: wgpu::Buffer,
    compute_uniform_bind_group: wgpu::BindGroup,
    render_uniform_bind_group: wgpu::BindGroup,
}

#[derive(Debug)]
pub struct RendererInfo {
    pub device_name: String,
    pub device_type: String,
    pub vendor_name: String,
    pub backend: String,
}

impl Display for RendererInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Vendor name: {}", self.vendor_name)?;
        writeln!(f, "Device name: {}", self.device_name)?;
        writeln!(f, "Device type: {}", self.device_type)?;
        writeln!(f, "Backend: {}", self.backend)?;
        Ok(())
    }
}

impl WgpuRender {
    pub fn get_info(&self) -> RendererInfo {
        let info = self.adapter.get_info();
        RendererInfo {
            device_name: info.name,
            device_type: self.get_device_type().to_string(),
            vendor_name: self.get_vendor_name().to_string(),
            backend: self.get_backend().to_string(),
        }
    }
    fn get_vendor_name(&self) -> &str {
        match self.adapter.get_info().vendor {
            0x1002 => "AMD",
            0x1010 => "ImgTec",
            0x10DE => "NVIDIA Corporation",
            0x13B5 => "ARM",
            0x5143 => "Qualcomm",
            0x8086 => "INTEL Corporation",
            _ => "Unknown vendor",
        }
    }
    fn get_backend(&self) -> &str {
        match self.adapter.get_info().backend {
            wgpu::Backend::Empty => "Empty",
            wgpu::Backend::Vulkan => "Vulkan",
            wgpu::Backend::Metal => "Metal",
            wgpu::Backend::Dx12 => "Dx12",
            wgpu::Backend::Dx11 => "Dx11",
            wgpu::Backend::Gl => "GL",
            wgpu::Backend::BrowserWebGpu => "Browser WGPU",
        }
    }
    fn get_device_type(&self) -> &str {
        match self.adapter.get_info().device_type {
            wgpu::DeviceType::Other => "Other",
            wgpu::DeviceType::IntegratedGpu => "Integrated GPU",
            wgpu::DeviceType::DiscreteGpu => "Discrete GPU",
            wgpu::DeviceType::VirtualGpu => "Virtual GPU",
            wgpu::DeviceType::Cpu => "CPU",
        }
    }

    pub fn new(
        window: &impl HasRawWindowHandle,
        push_constant_ranges: u32,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);

        let surface = unsafe { instance.create_surface(window) };

        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .unwrap();

        let format = surface
            .get_preferred_format(&adapter)
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
        let limits = adapter.limits();
        let features = adapter.features();
        let trace_dir = std::env::var("WGPU_TRACE");
        let (device, queue) = block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Device"),
                limits,
                features,
            },
            trace_dir.ok().as_ref().map(std::path::Path::new),
        ))?;
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            format,
            width: extent.width,
            height: extent.height,
            present_mode: wgpu::PresentMode::Immediate,
        };
        surface.configure(&device, &surface_config);

        let (textures, texture_views, sampled_texture_bind_group, storage_texture_bind_group) =
            create_textures(&device, extent);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            compare: None,
            ..Default::default()
        });
        let sampler_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Render Bind Group"),
            layout: &RenderPipelineLayoutInfo::new(&device)[Binding::Sampler],
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&sampler),
            }],
        });

        let blitter = Blitter::new(&device);
        let screenshot_ctx = ScreenshotCtx::new(&device, width, height);

        let multisampled_framebuffer =
            Self::create_multisample_texture(&device, format, NUM_SAMPLES, extent);

        let smaa_target = smaa::SmaaTarget::new(
            &device,
            &queue,
            width,
            height,
            format,
            smaa::SmaaMode::Smaa1X,
        );

        let uniform = Uniform::default();
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: std::mem::size_of::<Uniform>() as _,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let compute_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &ComputePipelineLayoutInfo::new(&device)[Binding::Uniform],
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let render_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &RenderPipelineLayoutInfo::new(&device)[Binding::Uniform],
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Ok(Self {
            adapter,
            device,
            surface,
            surface_config,
            pipelines: Vec::new(),
            shader_set: ContiniousHashMap::new(),
            queue,
            format,
            push_constant_ranges,

            textures,
            texture_views,

            extent,

            sampled_texture_bind_group,
            storage_texture_bind_group,
            sampler_bind_group,

            blitter,
            screenshot_ctx,

            multisampled_framebuffer,
            smaa_target,

            paused: false,

            uniform,
            uniform_buffer,
            compute_uniform_bind_group,
            render_uniform_bind_group,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        puffin::profile_function!();
        if self.extent.width == width && self.extent.height == height {
            return;
        }

        self.extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        let (textures, texture_views, sampled_texture_bind_group, storage_texture_bind_group) =
            create_textures(&self.device, self.extent);

        self.textures = textures;
        self.texture_views = texture_views;
        self.sampled_texture_bind_group = sampled_texture_bind_group;
        self.storage_texture_bind_group = storage_texture_bind_group;

        self.screenshot_ctx.resize(&self.device, width, height);

        self.multisampled_framebuffer =
            Self::create_multisample_texture(&self.device, self.format, NUM_SAMPLES, self.extent);
        self.smaa_target.resize(&self.device, width, height);
    }

    fn create_multisample_texture(
        device: &Device,
        format: wgpu::TextureFormat,
        sample_count: u32,
        extent: wgpu::Extent3d,
    ) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Multisampled Frame"),
                size: extent,
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    pub fn push_compute_pipeline(&mut self, comp: ShaderCreateInfo) -> Result<()> {
        self.pipelines.push(self.create_compute_pipeline(comp)?);

        Ok(())
    }

    pub fn push_render_pipeline(
        &mut self,
        vert: ShaderCreateInfo,
        frag: ShaderCreateInfo,
    ) -> Result<()> {
        self.pipelines
            .push(self.create_render_pipeline(vert, frag)?);

        Ok(())
    }

    pub fn rebuild_compute_pipeline(&mut self, index: usize, comp: ShaderCreateInfo) -> Result<()> {
        self.pipelines[index] = self.create_compute_pipeline(comp)?;

        Ok(())
    }

    pub fn rebuild_render_pipeline(
        &mut self,
        index: usize,
        vert: ShaderCreateInfo,
        frag: ShaderCreateInfo,
    ) -> Result<()> {
        self.pipelines[index] = self.create_render_pipeline(vert, frag)?;

        Ok(())
    }

    pub fn create_compute_pipeline(&self, cs: ShaderCreateInfo) -> Result<Pipeline> {
        let cs_module = unsafe {
            self.device
                .create_shader_module_spirv(&wgpu::ShaderModuleDescriptorSpirV {
                    label: Some("CS"),
                    source: cs.data.into(),
                })
        };

        let binding_layouts = ComputePipelineLayoutInfo::new(&self.device);
        let layouts = array_cringing(&binding_layouts.0);

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Compute Pipeline Layout"),
                bind_group_layouts: &layouts,
                push_constant_ranges: &[wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::COMPUTE,
                    range: 0..self.push_constant_ranges,
                }],
            });

        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Compute Pipeline"),
                layout: Some(&pipeline_layout),
                module: &cs_module,
                entry_point: cs.entry_point.to_str().unwrap(),
            });

        Ok(Pipeline::Compute(pipeline))
    }

    pub fn create_render_pipeline(
        &self,
        vs: ShaderCreateInfo,
        fs: ShaderCreateInfo,
    ) -> Result<Pipeline> {
        let fs_module = unsafe {
            self.device
                .create_shader_module_spirv(&wgpu::ShaderModuleDescriptorSpirV {
                    label: Some("FS"),
                    source: fs.data.into(),
                })
        };
        let vs_module = unsafe {
            self.device
                .create_shader_module_spirv(&wgpu::ShaderModuleDescriptorSpirV {
                    label: Some("VS"),
                    source: vs.data.into(),
                })
        };

        let binding_layouts = RenderPipelineLayoutInfo::new(&self.device);
        let layouts = array_cringing(&binding_layouts.0);

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &layouts,
                push_constant_ranges: &[wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    range: 0..self.push_constant_ranges,
                }],
            });

        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Render Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vs_module,
                    entry_point: vs.entry_point.to_str().unwrap(),
                    buffers: &[],
                },
                primitive: PrimitiveState {
                    topology: PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: NUM_SAMPLES,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &fs_module,
                    entry_point: fs.entry_point.to_str().unwrap(),
                    targets: &[wgpu::ColorTargetState {
                        format: self.format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    }],
                }),
                multiview: None,
            });

        Ok(Pipeline::Render(pipeline))
    }

    pub fn render(&mut self, push_constant: PushConstant) -> Result<(), wgpu::SurfaceError> {
        puffin::profile_function!();

        self.uniform = push_constant.into();
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniform));

        let mut _pre_scope =
            puffin::ProfilerScope::new("Aquiring frame", puffin::short_file_name(file!()), "");

        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(_) => {
                self.surface.configure(&self.device, &self.surface_config);
                self.surface
                    .get_current_texture()
                    .expect("Failed to acquire next surface texture")
            }
        };
        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let smaa_frame = self
            .smaa_target
            .start_frame(&self.device, &self.queue, &frame_view);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Main Encoder"),
            });

        drop(_pre_scope);

        for (i, pipeline) in self.pipelines.iter().enumerate() {
            match pipeline {
                Pipeline::Render(ref pipeline) => {
                    puffin::profile_scope!("Render Pass", format!("iteration {}", i).as_str());

                    let label = format!("Render Pass {}", i);
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some(&label),
                        color_attachments: &[wgpu::RenderPassColorAttachment {
                            view: &self.multisampled_framebuffer,
                            resolve_target: Some(&*smaa_frame),
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: true,
                            },
                        }],
                        depth_stencil_attachment: None,
                    });
                    render_pass.set_pipeline(pipeline);
                    render_pass.set_push_constants(
                        wgpu::ShaderStages::VERTEX_FRAGMENT,
                        0,
                        bytemuck::bytes_of(&push_constant),
                    );
                    render_pass.set_bind_group(0, &self.sampled_texture_bind_group, &[]);
                    render_pass.set_bind_group(1, &self.sampler_bind_group, &[]);
                    render_pass.set_bind_group(2, &self.render_uniform_bind_group, &[]);
                    render_pass.draw(0..3, 0..1);
                }
                Pipeline::Compute(ref pipeline) if !self.paused => {
                    puffin::profile_scope!("Compute Pass", format!("iteration {}", i).as_str());

                    let mut compute_pass =
                        encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some(&format!("Compute Pass {}", i)),
                        });
                    compute_pass.set_pipeline(pipeline);
                    compute_pass.set_push_constants(0, bytemuck::bytes_of(&push_constant));
                    compute_pass.set_bind_group(0, &self.storage_texture_bind_group, &[]);
                    compute_pass.set_bind_group(1, &self.compute_uniform_bind_group, &[]);
                    compute_pass.dispatch(
                        dispatch_optimal_size(self.extent.width, SUBGROUP_SIZE),
                        dispatch_optimal_size(self.extent.height, SUBGROUP_SIZE),
                        1,
                    );
                }
                Pipeline::Compute { .. } => {}
            }
        }

        {
            puffin::profile_scope!("Post-Process Antialiasing");
            drop(smaa_frame);
        }

        {
            puffin::profile_scope!("Blitting");
            self.blitter.blit_to_texture(
                &self.device,
                &mut encoder,
                &frame_view,
                &self.texture_views[0],
            );
        }

        {
            puffin::profile_scope!("Submit");
            self.queue.submit(std::iter::once(encoder.finish()));
        }
        {
            puffin::profile_scope!("Present");
            frame.present();
        }

        Ok(())
    }

    pub fn screenshot_dimentions(&self) -> ImageDimentions {
        self.screenshot_ctx.image_dimentions
    }

    pub fn capture_frame(&mut self) -> Result<Frame, wgpu::SurfaceError> {
        puffin::profile_function!();
        let frame = pollster::block_on(self.screenshot_ctx.capture_frame(
            &self.device,
            &self.queue,
            &self.textures[0],
        ));
        Ok(frame)
    }

    pub fn wait_idle(&self) {
        puffin::profile_function!();
        self.device.poll(wgpu::Maintain::Wait)
    }
}

fn array_cringing<T, const N: usize>(arr: &[T; N]) -> [&T; N] {
    let mut res = [&arr[0]; N];
    for i in 1..N {
        res[i] = &arr[i];
    }
    res
}
