#![warn(unsafe_op_in_unsafe_fn)]
#![feature(array_methods)]
#![feature(format_args_capture)]

use color_eyre::Result;
use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Path, PathBuf},
};

use raw_window_handle::HasRawWindowHandle;
use wgpu::{
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, ComputePipeline, Device, PrimitiveState,
    PrimitiveTopology, Queue, RenderPipeline, ShaderModule, Surface, TextureFormat, TextureUsages,
};

use naga::{
    back::spv::PipelineOptions,
    front::{self, glsl, wgsl},
    valid::{Capabilities, ValidationFlags, Validator},
    Module,
};

struct ShaderCompiler {
    wgsl: wgsl::Parser,
    glsl: glsl::Parser,
    validator: Validator,
    out: Vec<u32>,
}

impl ShaderCompiler {
    const SUPPORTED_SOURCES: &'static [&'static str] = &["glsl", "wgsl", "spv"];

    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_supported(&self, path: impl AsRef<Path>) -> bool {
        path.as_ref()
            .extension()
            .map(|ext| Self::SUPPORTED_SOURCES.contains(&ext.to_str().unwrap()))
            .is_some()
    }

    // TODO: Move outside
    pub fn from_path(
        &mut self,
        path: impl AsRef<Path>,
        stage: naga::ShaderStage,
    ) -> Option<Cow<[u32]>> {
        let file = || std::fs::read_to_string(&path).unwrap();
        match path.as_ref().extension() {
            Some(ext) => match ext.to_str() {
                Some("wgsl") => self.parse_wgsl(file(), stage),
                Some("glsl" | "frag" | "vert") => self.parse_glsl(file(), stage),
                Some("spv") => self.parse_spv(file().as_bytes(), stage),
                _ => None,
            },
            None => None,
        }
        .map(|x| x.into())
    }

    fn wgsl_to_wgsl(&mut self, source: impl AsRef<str>) -> Option<std::borrow::Cow<str>> {
        let module = match self.wgsl.parse(source.as_ref()) {
            Ok(m) => m,
            Err(e) => {
                e.emit_to_stderr(source.as_ref());
                return None;
            }
        };
        let module_info = self.validator.validate(&module).unwrap();
        Some(std::borrow::Cow::Owned(
            naga::back::wgsl::write_string(&module, &module_info).unwrap(),
        ))
    }

    fn compile(&mut self, module: Module, stage: naga::ShaderStage) -> &[u32] {
        let module_info = self.validator.validate(&module).unwrap();
        let mut writer = naga::back::spv::Writer::new(&naga::back::spv::Options::default())
            .expect("Failed to create spirv writer");

        self.out.clear();
        writer
            .write(
                &module,
                &module_info,
                Some(&PipelineOptions {
                    shader_stage: stage,
                    entry_point: "main".into(),
                }),
                &mut self.out,
            )
            .expect("Failed to write spirv");
        &self.out
    }

    pub fn parse_wgsl(
        &mut self,
        source: impl AsRef<str>,
        stage: naga::ShaderStage,
    ) -> Option<&[u32]> {
        let module = match self.wgsl.parse(source.as_ref()) {
            Ok(m) => m,
            Err(e) => {
                e.emit_to_stderr(source.as_ref());
                return None;
            }
        };
        Some(self.compile(module, stage))
    }
    pub fn parse_glsl(
        &mut self,
        source: impl AsRef<str>,
        stage: naga::ShaderStage,
    ) -> Option<&[u32]> {
        let module = match self
            .glsl
            .parse(&glsl::Options::from(stage), source.as_ref())
        {
            Ok(m) => m,
            Err(span) => {
                for e in span {
                    eprintln!("Glsl error: {e}");
                }
                return None;
            }
        };
        Some(self.compile(module, stage))
    }
    pub fn parse_spv(&mut self, data: &[u8], stage: naga::ShaderStage) -> Option<&[u32]> {
        let module = match naga::front::spv::parse_u8_slice(data, &front::spv::Options::default()) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Spir-V error {e}");
                return None;
            }
        };
        Some(self.compile(module, stage))
    }
}

impl Default for ShaderCompiler {
    fn default() -> Self {
        let validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        Self {
            wgsl: wgsl::Parser::new(),
            glsl: glsl::Parser::default(),
            validator,
            out: Vec::new(),
        }
    }
}

enum ShaderInfo {
    Glsl(PathBuf),
    Wgsl(PathBuf),
    SpirV(Vec<u32>),
}

// TODO: Put layout into Pipeline as field
enum Pipeline {
    Render {
        pipeline: RenderPipeline,
        vs: PathBuf,
        fs: PathBuf,
    },
    Compute {
        pipeline: ComputePipeline,
        cs: PathBuf,
    },
}

impl Pipeline {
    fn rebuild(&mut self, device: &Device, shader_compiler: &mut ShaderCompiler) -> Result<()> {
        match self {
            Self::Render { pipeline, vs, fs } => {
                let fs_module = {
                    let m =
                        match shader_compiler.wgsl_to_wgsl(std::fs::read_to_string(&fs).unwrap()) {
                            Some(f) => f,
                            None => color_eyre::eyre::bail!("Duh"),
                        };

                    device.create_shader_module(&wgpu::ShaderModuleDescriptor {
                        label: Some("FS"),
                        source: wgpu::ShaderSource::Wgsl(m),
                    })
                };

                let vs_module = {
                    let m =
                        match shader_compiler.wgsl_to_wgsl(std::fs::read_to_string(&vs).unwrap()) {
                            Some(f) => f,
                            None => color_eyre::eyre::bail!("Duh"),
                        };
                    device.create_shader_module(&wgpu::ShaderModuleDescriptor {
                        label: Some("VS"),
                        source: wgpu::ShaderSource::Wgsl(m),
                    })
                };
                *pipeline = make_render_pipeline(
                    &device,
                    &fs_module,
                    &vs_module,
                    wgpu::TextureFormat::Bgra8UnormSrgb,
                    None,
                );
            }
            Self::Compute { pipeline, cs } => {
                let cs_module = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
                    label: Some("VS"),
                    source: wgpu::ShaderSource::Wgsl(std::fs::read_to_string(&cs)?.into()),
                });
                *pipeline = make_compute_pipeline(&device, &cs_module, None);
            }
        }
        Ok(())
    }
}

// TODO: make static
trait Descriptor<'a, const N: usize> {
    const DESC: [BindGroupLayoutDescriptor<'a>; N];

    fn binding_group(device: &Device) -> [BindGroupLayout; N] {
        Self::DESC.map(|x| device.create_bind_group_layout(&x))
    }
}

#[derive(Debug)]
struct RenderPipelineLayoutInfo;
impl<'a> RenderPipelineLayoutInfo {
    const N: usize = 1;
    const DESC: [BindGroupLayoutDescriptor<'a>; Self::N] = [
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Render Bind Group Layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler {
                        filtering: true,
                        comparison: true,
                    },
                    count: None,
                },
            ],
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
}

impl<'a> Descriptor<'a, { Self::N }> for RenderPipelineLayoutInfo {
    const DESC: [BindGroupLayoutDescriptor<'a>; Self::N] = Self::DESC;
}

fn make_compute_pipeline(
    device: &Device,
    cs_module: &ShaderModule,
    label: Option<&str>,
) -> wgpu::ComputePipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &ComputePipelineLayoutInfo::binding_group(device).each_ref(),
        push_constant_ranges: &[wgpu::PushConstantRange {
            stages: wgpu::ShaderStages::COMPUTE,
            range: 0..PushConstant::size(),
        }],
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label,
        layout: Some(&pipeline_layout),
        module: cs_module,
        entry_point: "main",
    })
}

#[derive(Debug)]
struct ComputePipelineLayoutInfo;
impl<'a> ComputePipelineLayoutInfo {
    const N: usize = 1;
    const DESC: [BindGroupLayoutDescriptor<'a>; Self::N] = [
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Compute Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::ReadWrite,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::ReadWrite,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::ReadWrite,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::ReadWrite,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::ReadWrite,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        },
        // wgpu::BindGroupLayoutDescriptor {
        //     label: Some("fft Texture Bind Group Layout"),
        //     entries: &[wgpu::BindGroupLayoutEntry {
        //         binding: 0,
        //         visibility: wgpu::ShaderStages::FRAGMENT,
        //         ty: wgpu::BindingType::StorageTexture {
        //             access: wgpu::StorageTextureAccess::ReadWrite,
        //             format: wgpu::TextureFormat::Rgba32Float,
        //             view_dimension: wgpu::TextureViewDimension::D2,
        //         },
        //         count: None,
        //     }],
        // },
    ];
}

impl<'a> Descriptor<'a, { ComputePipelineLayoutInfo::N }> for ComputePipelineLayoutInfo {
    const DESC: [BindGroupLayoutDescriptor<'a>; ComputePipelineLayoutInfo::N] = Self::DESC;
}

fn make_render_pipeline(
    device: &Device,
    fs_module: &ShaderModule,
    vs_module: &ShaderModule,
    format: TextureFormat,
    label: Option<&str>,
) -> wgpu::RenderPipeline {
    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &RenderPipelineLayoutInfo::binding_group(device).each_ref(),
        push_constant_ranges: &[wgpu::PushConstantRange {
            stages: wgpu::ShaderStages::VERTEX_FRAGMENT,
            range: 0..PushConstant::size(),
        }],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label,
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vs_module,
            entry_point: "main",
            buffers: &[],
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            clamp_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &fs_module,
            entry_point: "main",
            targets: &[wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            }],
        }),
    })
}

// TODO: Can't get size from a surface
pub struct State {
    device: Device,
    surface: Surface,
    queue: Queue,
    pipelines: Vec<Pipeline>,
    pipeline_descriptors: Vec<Pipeline>,
    shader_set: HashMap<PathBuf, usize>,
    format: TextureFormat,
    push_constant: PushConstant,

    previous_frame: wgpu::Texture,
    generic_texture: wgpu::Texture,
    dummy_texture: wgpu::Texture,
    float_texture1: wgpu::Texture,
    float_texture2: wgpu::Texture,

    sampler: wgpu::Sampler,

    render_bind_group: BindGroup,
    compute_bind_group: BindGroup,

    shader_compiler: ShaderCompiler,
}

impl State {
    pub async fn new(window: &impl HasRawWindowHandle) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);

        let surface = unsafe { instance.create_surface(window) };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
            })
            .await
            .unwrap();

        let format = surface.get_preferred_format(&adapter).unwrap();
        // let format = wgpu::TextureFormat::Bgra8Unorm;
        let limits = adapter.limits();
        let features = adapter.features();
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Device"),
                    limits,
                    features,
                },
                None,
            )
            .await?;

        let width = 1920;
        let height = 720;

        let make_texture = |label, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                usage: wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
            })
        };

        let previous_frame =
            make_texture("Previous Frame Texxture", wgpu::TextureFormat::Rgba8Unorm);
        let generic_texture = make_texture("Generic Texture", wgpu::TextureFormat::Rgba8Unorm);
        let dummy_texture = make_texture("Dummy Texture", wgpu::TextureFormat::Rgba8Unorm);
        let float_texture1 = make_texture("Float Texture 1", wgpu::TextureFormat::Rgba32Float);
        let float_texture2 = make_texture("Float Texture 2", wgpu::TextureFormat::Rgba32Float);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::Always),
            ..Default::default()
        });

        let push_constant = PushConstant::default();

        let render_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Render Bind Group"),
            layout: &RenderPipelineLayoutInfo::binding_group(&device)[0],
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &previous_frame.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &generic_texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &dummy_texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        &float_texture1.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        &float_texture2.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Compute Bind Group"),
            layout: &ComputePipelineLayoutInfo::binding_group(&device)[0],
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &previous_frame.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &generic_texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &dummy_texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        &float_texture1.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        &float_texture2.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
            ],
        });

        let shader_compiler = ShaderCompiler::default();

        Ok(Self {
            device,
            surface,
            pipelines: Vec::new(),
            pipeline_descriptors: Vec::new(),
            shader_set: HashMap::new(),
            queue,
            format,
            push_constant,

            previous_frame,
            generic_texture,
            dummy_texture,
            float_texture1,
            float_texture2,

            sampler,

            render_bind_group,
            compute_bind_group,

            shader_compiler,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface.configure(
                &self.device,
                &wgpu::SurfaceConfiguration {
                    usage: TextureUsages::RENDER_ATTACHMENT,
                    format: self.format,
                    width,
                    height,
                    present_mode: wgpu::PresentMode::Fifo,
                },
            );
        }
    }

    // TODO: Rename `dependencies` to `includes`
    pub fn push_render_pipeline(
        &mut self,
        fs: PathBuf,
        vs: PathBuf,
        dependencies: &[PathBuf],
    ) -> Result<()> {
        let fs = fs.canonicalize()?;
        let vs = vs.canonicalize()?;

        let pipeline_number = self.pipelines.len();

        self.shader_set.insert(fs.clone(), pipeline_number);
        self.shader_set.insert(vs.clone(), pipeline_number);

        for deps in dependencies {
            self.shader_set
                .insert(deps.canonicalize()?, pipeline_number);
        }

        let fs_module = self
            .device
            .create_shader_module(&wgpu::ShaderModuleDescriptor {
                label: Some("FS"),
                source: wgpu::ShaderSource::Wgsl(
                    self.shader_compiler
                        .wgsl_to_wgsl(std::fs::read_to_string(&fs).unwrap())
                        .unwrap(),
                ),
            });

        let vs_module = self
            .device
            .create_shader_module(&wgpu::ShaderModuleDescriptor {
                label: Some("VS"),
                source: wgpu::ShaderSource::Wgsl(
                    self.shader_compiler
                        .wgsl_to_wgsl(std::fs::read_to_string(&vs).unwrap())
                        .unwrap(),
                ),
            });

        let pipeline =
            make_render_pipeline(&self.device, &fs_module, &vs_module, self.format, None);

        self.pipelines.push(Pipeline::Render { pipeline, fs, vs });

        Ok(())
    }

    pub fn render(&self) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_frame()?.output;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        for (i, pipeline) in self.pipelines.iter().enumerate() {
            match pipeline {
                Pipeline::Render { pipeline, .. } => {
                    let label = format!("Render Pass {}", i);
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some(&label),
                        color_attachments: &[wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.1,
                                    g: 0.2,
                                    b: 0.3,
                                    a: 1.0,
                                }),
                                store: true,
                            },
                        }],
                        depth_stencil_attachment: None,
                    });
                    render_pass.set_pipeline(&pipeline);
                    render_pass.set_push_constants(
                        wgpu::ShaderStages::VERTEX_FRAGMENT,
                        0,
                        self.push_constant.as_bytes(),
                    );
                    render_pass.set_bind_group(0, &self.render_bind_group, &[]);
                    render_pass.draw(0..3, 0..1);
                }
                Pipeline::Compute { pipeline, .. } => {
                    let mut compute_pass =
                        encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some(&format!("Compute Pass {}", i)),
                        });
                    compute_pass.set_push_constants(0, self.push_constant.as_bytes());
                    compute_pass.set_pipeline(pipeline);
                    compute_pass.set_bind_group(0, &self.compute_bind_group, &[]);
                    compute_pass.dispatch(0, 0, 0);
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        self.device.stop_capture();
        Ok(())
    }

    pub fn rebuild_pipelines(&mut self, paths: &[PathBuf]) -> Result<()> {
        for path in paths {
            match self.shader_set.get(path) {
                Some(&pipeline_index) => match self.pipelines[pipeline_index]
                    .rebuild(&self.device, &mut self.shader_compiler)
                {
                    Ok(_) => {
                        println!("Success!");
                    }
                    Err(_) => {
                        println!("Booooo!");
                    }
                },
                None => {}
            }
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
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

impl PushConstant {
    fn size() -> u32 {
        std::mem::size_of::<Self>() as _
    }

    fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}

/// # Safety
/// Until you're using it on not ZST or DST it's fine
pub unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts((p as *const T) as *const _, std::mem::size_of::<T>()) }
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
