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
use pilka_types::ShaderInfo;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    fmt::Display,
    hash::Hash,
    ops::{Deref, DerefMut, Index},
    path::{Path, PathBuf},
};

use raw_window_handle::HasRawWindowHandle;
use wgpu::{
    Adapter, BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, ComputePipeline, Device,
    PipelineLayout, PrimitiveState, PrimitiveTopology, Queue, RenderPipeline, Surface,
    SurfaceConfiguration, Texture, TextureFormat, TextureView,
};

use naga::{
    back::spv::PipelineOptions,
    front::{self, glsl, wgsl},
    valid::{Capabilities, ValidationFlags, Validator},
    Module,
};

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

impl<K, V> ContiniousHashMap<K, V> {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

impl<K: Eq + Hash, V: Eq + Hash> ContiniousHashMap<K, V> {
    pub fn push_value(&mut self, key: K, value: V) {
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
    ) -> Option<Cow<str>> {
        dbg!("Hello?");
        let file = || std::fs::read_to_string(&path).unwrap();
        let module = match path.as_ref().extension() {
            Some(ext) => match ext.to_str() {
                Some("wgsl") => self.parse_wgsl(file(), stage),
                Some("glsl" | "frag" | "vert" | "comp") => self.parse_glsl(file(), stage),
                Some("spv") => self.parse_spv(file().as_bytes(), stage),
                _ => None,
            },
            None => None,
        }
        .unwrap();
        let module_info = self.validator.validate(&module).unwrap();
        Some(std::borrow::Cow::Owned(
            naga::back::wgsl::write_string(&module, &module_info).unwrap(),
        ))
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
    ) -> Option<Module> {
        match self.wgsl.parse(source.as_ref()) {
            Ok(m) => Some(m),
            Err(e) => {
                dbg!(&e);
                e.emit_to_stderr(source.as_ref());
                return None;
            }
        }
    }
    pub fn parse_glsl(
        &mut self,
        source: impl AsRef<str>,
        stage: naga::ShaderStage,
    ) -> Option<Module> {
        dbg!("boop");
        match self
            .glsl
            .parse(&glsl::Options::from(stage), source.as_ref())
        {
            Ok(m) => Some(m),
            Err(span) => {
                println!("Got here");
                dbg!(&span);
                for e in span {
                    eprintln!("Glsl error: {e}");
                }
                return None;
            }
        }
    }
    pub fn parse_spv(&mut self, data: &[u8], stage: naga::ShaderStage) -> Option<Module> {
        match naga::front::spv::parse_u8_slice(data, &front::spv::Options::default()) {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!("Spir-V error {e}");
                return None;
            }
        }
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

enum ShaderInfo_Duuuuuh {
    Glsl(PathBuf),
    Wgsl(PathBuf),
    SpirV(Vec<u32>),
}

// TODO: Put layout into Pipeline as field
enum PipelineKind {
    Render {
        pipeline: RenderPipeline,
        vs: ShaderInfo,
        fs: ShaderInfo,
        target_format: wgpu::TextureFormat,
    },
    Compute {
        pipeline: ComputePipeline,
        cs: ShaderInfo,
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
                    let m = match shader_compiler.from_path(&fs.path, naga::ShaderStage::Fragment) {
                        Some(f) => f,
                        None => color_eyre::eyre::bail!("Duh"),
                    };

                    device.create_shader_module(&wgpu::ShaderModuleDescriptor {
                        label: Some("FS"),
                        source: wgpu::ShaderSource::Wgsl(m),
                    })
                };

                let vs_module = {
                    let m = match shader_compiler.from_path(&vs.path, naga::ShaderStage::Vertex) {
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
                        entry_point: vs.entry_point.to_str()?,
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
                        entry_point: fs.entry_point.to_str()?,
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
                    source: wgpu::ShaderSource::Wgsl(std::fs::read_to_string(&cs.path)?.into()),
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

enum Binding {
    Texture,
    Sampler,
    #[allow(dead_code)]
    Fft,
}

impl<const N: usize> Index<Binding> for [BindGroupLayout; N] {
    type Output = BindGroupLayout;

    fn index(&self, index: Binding) -> &Self::Output {
        match index {
            Binding::Texture => &self[0],
            Binding::Sampler => &self[1],
            Binding::Fft => &self[2],
        }
    }
}

fn create_textures(
    device: &Device,
    extent: wgpu::Extent3d,
) -> (Vec<Texture>, Vec<TextureView>, BindGroup, BindGroup) {
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
        layout: &RenderPipelineLayoutInfo::binding_group(device)[Binding::Texture],
        entries: &entries,
    });

    let storage_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Compute Bind Group"),
        layout: &ComputePipelineLayoutInfo::binding_group(device)[Binding::Texture],
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
    queue: Queue,
    pipelines: Vec<Pipeline>,
    pipeline_descriptors: Vec<Pipeline>,
    pub shader_set: ContiniousHashMap<PathBuf, usize>,
    format: TextureFormat,
    push_constant_ranges: u32,

    extent: wgpu::Extent3d,

    textures: Vec<Texture>,
    texture_views: Vec<TextureView>,
    sampler: wgpu::Sampler,

    sampled_texture_bind_group: BindGroup,
    storage_texture_bind_group: BindGroup,
    sampler_bind_group: BindGroup,

    shader_compiler: ShaderCompiler,

    pub paused: bool,
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

    pub async fn new(
        window: &impl HasRawWindowHandle,
        push_constant_ranges: u32,
        width: u32,
        height: u32,
    ) -> Result<Self> {
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
            width,
            height,
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
            compare: Some(wgpu::CompareFunction::Always),
            ..Default::default()
        });
        let sampler_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Render Bind Group"),
            layout: &RenderPipelineLayoutInfo::binding_group(&device)[Binding::Sampler],
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&sampler),
            }],
        });

        let shader_compiler = ShaderCompiler::default();

        Ok(Self {
            adapter,
            device,
            surface,
            pipelines: Vec::new(),
            pipeline_descriptors: Vec::new(),
            shader_set: ContiniousHashMap::new(),
            queue,
            format,
            push_constant_ranges,

            textures,
            texture_views,
            sampler,

            extent,

            sampled_texture_bind_group,
            storage_texture_bind_group,
            sampler_bind_group,

            shader_compiler,

            paused: false,
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
            let (textures, texture_views, sampled_texture_bind_group, storage_texture_bind_group) =
                create_textures(&self.device, self.extent);

            self.textures = textures;
            self.texture_views = texture_views;
            self.sampled_texture_bind_group = sampled_texture_bind_group;
            self.storage_texture_bind_group = storage_texture_bind_group;
        }
    }

    pub fn push_compute_pipeline(&mut self, cs: ShaderInfo, includes: &[PathBuf]) -> Result<()> {
        let cs_path = cs.path.canonicalize()?;
        let pipeline_number = self.pipelines.len();

        self.shader_set.push_value(cs_path.clone(), pipeline_number);

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
                        .from_path(&cs_path, naga::ShaderStage::Compute)
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
        fs: ShaderInfo,
        vs: ShaderInfo,
        includes: &[PathBuf],
    ) -> Result<()> {
        let fs_path = fs.path.canonicalize()?;
        let vs_path = vs.path.canonicalize()?;

        let pipeline_number = self.pipelines.len();

        self.shader_set.push_value(fs_path.clone(), pipeline_number);
        self.shader_set.push_value(vs_path.clone(), pipeline_number);

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
                        .from_path(&fs_path, naga::ShaderStage::Fragment)
                        .unwrap(),
                ),
            });

        let vs_module = self
            .device
            .create_shader_module(&wgpu::ShaderModuleDescriptor {
                label: Some("VS"),
                source: wgpu::ShaderSource::Wgsl(
                    self.shader_compiler
                        .from_path(&vs_path, naga::ShaderStage::Vertex)
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

        // TODO: Provide entry point
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
            self.textures[0].as_image_copy(),
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
                                    r: 0.0,
                                    g: 0.0,
                                    b: 0.0,
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
                PipelineKind::Compute { ref pipeline, .. } if !self.paused => {
                    let mut compute_pass =
                        encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some(&format!("Compute Pass {}", i)),
                        });
                    compute_pass.set_push_constants(0, push_constant);
                    compute_pass.set_pipeline(pipeline);
                    compute_pass.set_bind_group(0, &self.storage_texture_bind_group, &[]);
                    compute_pass.dispatch(0, 0, 0);
                }
                PipelineKind::Compute { .. } => {}
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        self.device.stop_capture();
        Ok(())
    }

    pub fn rebuild_pipelines(&mut self, paths: &[PathBuf]) -> Result<()> {
        for path in paths {
            if let Some(pipeline_indexes) = self.shader_set.get(path) {
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

    pub fn wait_idle(&self) {
        self.device.poll(wgpu::Maintain::Wait)
    }
}
