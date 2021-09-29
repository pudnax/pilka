#![warn(unsafe_op_in_unsafe_fn)]
#![feature(array_methods)]
#![feature(format_args_capture)]
#![allow(
    // We use loops for getting early-out of scope without closures.
    clippy::never_loop,
    // We don't use syntax sugar where it's not necessary.
    clippy::match_like_matches_macro,
    // Redundant matching is more explicit.
    clippy::redundant_pattern_matching,
    // Explicit lifetimes are often easier to reason about.
    clippy::needless_lifetimes,
    // No need for defaults in the internal types.
    clippy::new_without_default,
    // For some reason `rustc` can warn about these in const generics even
    // though they are required.
    unused_braces,
)]
#![warn(trivial_casts, trivial_numeric_casts, unused_extern_crates)]

use color_eyre::Result;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    hash::Hash,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};

use raw_window_handle::HasRawWindowHandle;
use wgpu::{
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, ComputePipeline, Device, PipelineLayout,
    PrimitiveState, PrimitiveTopology, Queue, RenderPipeline, Surface, SurfaceConfiguration,
    TextureFormat, TextureUsages,
};

use naga::{
    back::spv::PipelineOptions,
    front::{self, glsl, wgsl},
    valid::{Capabilities, ValidationFlags, Validator},
    Module,
};

struct ContiniousHashMap<K, V>(HashMap<K, HashSet<V>>);

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
        self.0.entry(key).or_insert_with(HashSet::new).insert(value);
    }
}

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
enum PipelineKind {
    Render {
        pipeline: RenderPipeline,
        vs: PathBuf,
        fs: PathBuf,
        target_format: wgpu::TextureFormat,
    },
    Compute {
        pipeline: ComputePipeline,
        cs: PathBuf,
    },
}

struct Pipeline {
    kind: PipelineKind,
    layout: PipelineLayout,
}

impl Pipeline {
    fn rebuild(&mut self, device: &Device, shader_compiler: &mut ShaderCompiler) -> Result<()> {
        match self {
            Pipeline {
                kind:
                    PipelineKind::Render {
                        pipeline,
                        vs,
                        fs,
                        target_format,
                    },
                layout,
            } => {
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
                *pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Render Pipeline"),
                    layout: Some(layout),
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
                            format: *target_format,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        }],
                    }),
                });
            }
            Pipeline {
                kind: PipelineKind::Compute { pipeline, cs },
                layout,
            } => {
                let cs_module = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
                    label: Some("VS"),
                    source: wgpu::ShaderSource::Wgsl(std::fs::read_to_string(&cs)?.into()),
                });
                *pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("Somepute Pipeline"),
                    layout: Some(layout),
                    module: &cs_module,
                    entry_point: "main",
                })
            }
        }
        Ok(())
    }
}

trait Descriptor<'a, const N: usize> {
    const DESC: [BindGroupLayoutDescriptor<'a>; N];

    fn binding_group(device: &Device) -> [BindGroupLayout; N] {
        Self::DESC.map(|x| device.create_bind_group_layout(&x))
    }
}

#[derive(Debug)]
struct RenderPipelineLayoutInfo;
impl<'a> RenderPipelineLayoutInfo {
    const N: usize = 2;
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
            ],
        }   ,
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Render Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler {
                    filtering: true,
                    comparison: true,
                },
                count: None,
            }],
        }
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

pub struct State {
    device: Device,
    surface: Surface,
    queue: Queue,
    pipelines: Vec<Pipeline>,
    pipeline_descriptors: Vec<Pipeline>,
    shader_set: ContiniousHashMap<PathBuf, usize>,
    format: TextureFormat,
    push_constant_ranges: u32,

    previous_frame: wgpu::Texture,
    generic_texture: wgpu::Texture,
    dummy_texture: wgpu::Texture,
    float_texture1: wgpu::Texture,
    float_texture2: wgpu::Texture,

    sampler: wgpu::Sampler,

    extent: wgpu::Extent3d,

    sampled_texture_bind_group: BindGroup,
    sampler_bind_group: BindGroup,
    storage_texture_bind_group: BindGroup,

    shader_compiler: ShaderCompiler,
}

impl State {
    pub async fn new(window: &impl HasRawWindowHandle, push_constant_ranges: u32) -> Result<Self> {
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

        let extent = wgpu::Extent3d {
            width: 1920,
            height: 720,
            depth_or_array_layers: 1,
        };

        // FIXME: `configure` Doesn't mutate surface?
        surface.configure(
            &device,
            &SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                format,
                width: extent.width,
                height: extent.height,
                present_mode: wgpu::PresentMode::Fifo,
            },
        );

        let make_texture = |label, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
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

        let sampled_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
            ],
        });

        let sampler_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Render Bind Group"),
            layout: &RenderPipelineLayoutInfo::binding_group(&device)[1],
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&sampler),
            }],
        });

        let storage_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
            shader_set: ContiniousHashMap(HashMap::new()),
            queue,
            format,
            push_constant_ranges,

            previous_frame,
            generic_texture,
            dummy_texture,
            float_texture1,
            float_texture2,

            sampler,

            extent,

            sampled_texture_bind_group,
            sampler_bind_group,
            storage_texture_bind_group,

            shader_compiler,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.extent = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            self.surface.configure(
                &self.device,
                &wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    format: self.format,
                    width,
                    height,
                    present_mode: wgpu::PresentMode::Fifo,
                },
            );

            let make_texture = |label, format| {
                self.device.create_texture(&wgpu::TextureDescriptor {
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

            self.previous_frame =
                make_texture("Previous Frame Texxture", wgpu::TextureFormat::Rgba8Unorm);
            self.generic_texture = make_texture("Generic Texture", wgpu::TextureFormat::Rgba8Unorm);
            self.dummy_texture = make_texture("Dummy Texture", wgpu::TextureFormat::Rgba8Unorm);
            self.float_texture1 = make_texture("Float Texture 1", wgpu::TextureFormat::Rgba32Float);
            self.float_texture2 = make_texture("Float Texture 2", wgpu::TextureFormat::Rgba32Float);

            self.sampled_texture_bind_group =
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Render Bind Group"),
                    layout: &RenderPipelineLayoutInfo::binding_group(&self.device)[0],
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                &self
                                    .previous_frame
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &self
                                    .generic_texture
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(
                                &self
                                    .dummy_texture
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(
                                &self
                                    .float_texture1
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(
                                &self
                                    .float_texture2
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                    ],
                });

            self.storage_texture_bind_group =
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Compute Bind Group"),
                    layout: &ComputePipelineLayoutInfo::binding_group(&self.device)[0],
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                &self
                                    .previous_frame
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &self
                                    .generic_texture
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(
                                &self
                                    .dummy_texture
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(
                                &self
                                    .float_texture1
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(
                                &self
                                    .float_texture2
                                    .create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                    ],
                });
        }
    }

    pub fn push_compute_pipeline(&mut self, cs: PathBuf, includes: &[PathBuf]) -> Result<()> {
        let cs = cs.canonicalize()?;
        let pipeline_number = self.pipelines.len();

        self.shader_set.push_value(cs.clone(), pipeline_number);

        for deps in includes {
            self.shader_set
                .push_value(deps.canonicalize()?, pipeline_number);
        }

        let cs_module = self
            .device
            .create_shader_module(&wgpu::ShaderModuleDescriptor {
                label: Some("CS"),
                source: wgpu::ShaderSource::Wgsl(
                    self.shader_compiler
                        .wgsl_to_wgsl(std::fs::read_to_string(&cs).unwrap())
                        .unwrap(),
                ),
            });

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Compute Pipeline Layout"),
                bind_group_layouts: &ComputePipelineLayoutInfo::binding_group(&self.device)
                    .each_ref(),
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
                entry_point: "main",
            });

        self.pipelines.push(Pipeline {
            kind: PipelineKind::Compute { pipeline, cs },
            layout: pipeline_layout,
        });

        Ok(())
    }

    pub fn push_render_pipeline(
        &mut self,
        fs: PathBuf,
        vs: PathBuf,
        includes: &[PathBuf],
    ) -> Result<()> {
        let fs = fs.canonicalize()?;
        let vs = vs.canonicalize()?;

        let pipeline_number = self.pipelines.len();

        self.shader_set.push_value(fs.clone(), pipeline_number);
        self.shader_set.push_value(vs.clone(), pipeline_number);

        for deps in includes {
            self.shader_set
                .push_value(deps.canonicalize()?, pipeline_number);
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

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &RenderPipelineLayoutInfo::binding_group(&self.device)
                    .each_ref(),
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
                        format: self.format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    }],
                }),
            });

        self.pipelines.push(Pipeline {
            kind: PipelineKind::Render {
                pipeline,
                fs,
                vs,
                target_format: self.format,
            },
            layout: pipeline_layout,
        });

        Ok(())
    }

    pub fn render(&self, push_constant: &[u8]) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_frame()?.output;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        encoder.copy_texture_to_texture(
            frame.texture.as_image_copy(),
            self.previous_frame.as_image_copy(),
            self.extent,
        );

        for (i, pipeline) in self.pipelines.iter().enumerate() {
            match pipeline.kind {
                PipelineKind::Render { ref pipeline, .. } => {
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
                    render_pass.set_pipeline(pipeline);
                    render_pass.set_push_constants(
                        wgpu::ShaderStages::VERTEX_FRAGMENT,
                        0,
                        push_constant,
                    );
                    render_pass.set_bind_group(0, &self.sampled_texture_bind_group, &[]);
                    render_pass.set_bind_group(1, &self.sampler_bind_group, &[]);
                    render_pass.draw(0..3, 0..1);
                }
                PipelineKind::Compute { ref pipeline, .. } => {
                    let mut compute_pass =
                        encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some(&format!("Compute Pass {}", i)),
                        });
                    compute_pass.set_push_constants(0, push_constant);
                    compute_pass.set_pipeline(pipeline);
                    compute_pass.set_bind_group(0, &self.storage_texture_bind_group, &[]);
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
            if let Some(pipeline_indexes) = self.shader_set.get(path) {
                dbg!(&pipeline_indexes);
                for &pipeline_index in pipeline_indexes {
                    match self.pipelines[pipeline_index]
                        .rebuild(&self.device, &mut self.shader_compiler)
                    {
                        Ok(_) => {
                            println!("Success!");
                        }
                        Err(_) => {
                            println!("Booooo!");
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
