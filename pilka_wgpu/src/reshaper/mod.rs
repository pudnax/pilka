use wgpu::util::{BufferInitDescriptor, DeviceExt};

/// Reshapes a texture from its original size, sample_count and format to the destination size,
/// sample_count and format.
///
/// The `src_texture` must have the `TextureUsage::SAMPLED` enabled.
///
/// The `dst_texture` must have the `TextureUsage::RENDER_ATTACHMENT` enabled.
#[derive(Debug)]
pub struct Reshaper {
    _vs_mod: wgpu::ShaderModule,
    _fs_mod: wgpu::ShaderModule,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
struct Vertex {
    pub position: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone)]
struct Uniforms {
    sample_count: u32,
}

impl Reshaper {
    /// Construct a new `Reshaper`.
    pub fn new(
        device: &wgpu::Device,
        src_texture: &wgpu::TextureView,
        src_sample_count: u32,
        src_sample_type: wgpu::TextureSampleType,
        dst_sample_count: u32,
        dst_format: wgpu::TextureFormat,
    ) -> Self {
        // Load shader modules.
        let vs_mod = shader_from_spirv_bytes(device, include_bytes!("shaders/vert.spv"));
        let fs_mod = match src_sample_count {
            1 => shader_from_spirv_bytes(device, include_bytes!("shaders/frag.spv")),
            2 => shader_from_spirv_bytes(device, include_bytes!("shaders/frag_msaa2.spv")),
            4 => shader_from_spirv_bytes(device, include_bytes!("shaders/frag_msaa4.spv")),
            8 => shader_from_spirv_bytes(device, include_bytes!("shaders/frag_msaa8.spv")),
            16 => shader_from_spirv_bytes(device, include_bytes!("shaders/frag_msaa16.spv")),
            _ => shader_from_spirv_bytes(device, include_bytes!("shaders/frag_msaa.spv")),
        };

        // Create the sampler for sampling from the source texture.
        // let sampler_desc = wgpu::SamplerBuilder::new().into_descriptor();
        // let sampler_filtering = wgpu::sampler_filtering(&sampler_desc);
        // let sampler = device.create_sampler(&sampler_desc);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Sampler"),
            address_mode_u: wgpu::AddressMode::MirrorRepeat,
            address_mode_v: wgpu::AddressMode::MirrorRepeat,
            address_mode_w: wgpu::AddressMode::MirrorRepeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: -100.,
            lod_max_clamp: 100.,
            compare: None,
            anisotropy_clamp: None,
            border_color: None,
        });
        let sampler_filtering = true;

        // Create the render pipeline.
        let bind_group_layout =
            bind_group_layout(device, src_sample_count, src_sample_type, sampler_filtering);
        let pipeline_layout = pipeline_layout(device, &bind_group_layout);
        let render_pipeline = render_pipeline(
            device,
            &pipeline_layout,
            &vs_mod,
            &fs_mod,
            dst_sample_count,
            dst_format,
        );

        // Create the uniform buffer to pass the sample count if we don't have an unrolled resolve
        // fragment shader for it.
        let uniform_buffer = {
            let uniforms = Uniforms {
                sample_count: src_sample_count,
            };
            let uniforms_bytes = uniforms_as_bytes(&uniforms);
            let usage = wgpu::BufferUsages::UNIFORM;

            device.create_buffer_init(&BufferInitDescriptor {
                label: None,
                contents: uniforms_bytes,
                usage,
            })
        };

        // Create the bind group.
        let bind_group = bind_group(
            device,
            &bind_group_layout,
            src_texture,
            &sampler,
            &uniform_buffer,
        );

        // Create the vertex buffer.
        let vertices_bytes = vertices_as_bytes(&VERTICES[..]);
        let vertex_usage = wgpu::BufferUsages::VERTEX;
        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: None,
            contents: vertices_bytes,
            usage: vertex_usage,
        });

        Reshaper {
            _vs_mod: vs_mod,
            _fs_mod: fs_mod,
            bind_group_layout,
            bind_group,
            render_pipeline,
            sampler,
            uniform_buffer,
            vertex_buffer,
        }
    }

    /// Given an encoder, submits a render pass command for writing the source texture to the
    /// destination texture.
    pub fn encode_render_pass(
        &self,
        dst_texture: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Capture Pass"),
            color_attachments: &[wgpu::RenderPassColorAttachment {
                view: dst_texture,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.1,
                        a: 1.0,
                    }),
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        let vertex_range = 0..VERTICES.len() as u32;
        let instance_range = 0..1;
        render_pass.draw(vertex_range, instance_range);
    }
}

const VERTICES: [Vertex; 4] = [
    Vertex {
        position: [-1.0, 1.0],
    },
    Vertex {
        position: [-1.0, -1.0],
    },
    Vertex {
        position: [1.0, 1.0],
    },
    Vertex {
        position: [1.0, -1.0],
    },
];

// We provide pre-prepared fragment shaders with unrolled resolves for common sample counts.
fn unrolled_sample_count(sample_count: u32) -> bool {
    match sample_count {
        1 | 2 | 4 | 8 | 16 => true,
        _ => false,
    }
}

fn bind_group_layout(
    device: &wgpu::Device,
    src_sample_count: u32,
    src_sample_type: wgpu::TextureSampleType,
    sampler_filtering: bool,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SRC Capture Bind Group Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler {
                    filtering: true,
                    comparison: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                    // min_binding_size: Some(
                    //     NonZeroU64::new(std::mem::size_of::<Uniforms>() as _).unwrap(),
                    // ),
                },
                count: None,
            },
        ],
    })
}
fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    texture: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    uniform_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Capture Bind Group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    })
}

fn pipeline_layout(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::PipelineLayout {
    let desc = wgpu::PipelineLayoutDescriptor {
        label: Some("nannou_reshaper"),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    };
    device.create_pipeline_layout(&desc)
}

fn render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    vs_mod: &wgpu::ShaderModule,
    fs_mod: &wgpu::ShaderModule,
    dst_sample_count: u32,
    dst_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Capture Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: vs_mod,
            entry_point: "main",
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as _,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2],
            }],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            clamp_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: dst_sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: fs_mod,
            entry_point: "main",
            targets: &[wgpu::ColorTargetState {
                format: dst_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            }],
        }),
    })
}

fn uniforms_as_bytes(uniforms: &Uniforms) -> &[u8] {
    unsafe { bytes::from(uniforms) }
}

fn vertices_as_bytes(data: &[Vertex]) -> &[u8] {
    unsafe { bytes::from_slice(data) }
}

pub fn shader_from_spirv_bytes(device: &wgpu::Device, bytes: &[u8]) -> wgpu::ShaderModule {
    let source = wgpu::util::make_spirv(bytes);
    let desc = wgpu::ShaderModuleDescriptor {
        label: Some("nannou_shader_module"),
        source,
    };
    device.create_shader_module(&desc)
}

/// The functions within this module use unsafe in order to retrieve their input as a slice of
/// bytes. This is necessary in order to upload data to the GPU via the wgpu
/// `DeviceExt::create_buffer_init` buffer constructor. This method is unsafe as the type `T` may contain
/// padding which is considered to be uninitialised memory in Rust and may potentially lead to
/// undefined behaviour.
///
/// These should be replaced in the future with something similar to `zerocopy`. Unfortunately, we
/// don't gain much benefit from using `zerocopy` in our case as `zerocopy` provides no way to
/// implement the `AsBytes` trait for generic types (e.g. `Vector*`), even with their type
/// parameters filled (e.g. `Vector2<f32>`). This means we can't derive `AsBytes` for the majority
/// of the types where we need to as `derive(AsBytes)` requires that all fields implement
/// `AsBytes`, and neither our `Vector` types or the palette color types can implement it.
///
/// There is a relatively new crate `bytemuck` which provides traits for this, however these traits
/// are `unsafe` and so we don't gain much benefit in terms of safety, especially for our simple
/// use-case. There is a `zeroable` crate that attempts to derive the `Zeroable` trait from
/// `bytemuck`, however:
/// 1. there not yet any other publicly dependent crates or public discussion around the safety of
///    the provided derives and
/// 2. we would still require implementing `Pod` unsafely.
pub mod bytes {
    pub unsafe fn from_slice<T>(slice: &[T]) -> &[u8]
    where
        T: Copy + Sized,
    {
        let len = slice.len() * std::mem::size_of::<T>();
        let ptr = slice.as_ptr() as *const u8;
        std::slice::from_raw_parts(ptr, len)
    }

    pub unsafe fn from<T>(t: &T) -> &[u8]
    where
        T: Copy + Sized,
    {
        let len = std::mem::size_of::<T>();
        let ptr: *const _ = t;
        std::slice::from_raw_parts(ptr.cast(), len)
    }

    /// This is really an astonishingly unsafe function.
    /// Please don't use it.
    pub unsafe fn to_slice<T>(slice: &[u8]) -> &[T]
    where
        T: Copy + Sized,
    {
        let size = std::mem::size_of::<T>();
        let align = std::mem::align_of::<T>();
        assert_eq!(slice.len() % size, 0, "incorrect buffer size");
        assert_eq!(
            slice.as_ptr() as usize % align,
            0,
            "incorrect buffer alignment"
        );
        let len = slice.len() / size;
        let ptr = slice.as_ptr() as *const T;
        std::slice::from_raw_parts(ptr, len)
    }
}
