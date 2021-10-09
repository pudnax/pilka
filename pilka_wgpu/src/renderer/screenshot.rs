use std::num::{NonZeroU32, NonZeroU64};

use pilka_types::{dispatch_optimal_size, ImageDimentions};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    Device,
};

use super::SUBGROUP_SIZE;

#[repr(C)]
#[derive(Debug)]
struct Uniforms {
    samples: u32,
}

impl Uniforms {
    fn as_slice(&self) -> &[u8] {
        let len = std::mem::size_of::<Self>();
        let ptr: *const _ = self;
        unsafe { std::slice::from_raw_parts(ptr as *const u8, len) }
    }
}

struct BindingResources {
    src_texture_bind_group: wgpu::BindGroup,
    dst_texture_bind_group: wgpu::BindGroup,
    dst_texture: wgpu::Texture,
}

// Texshiter
pub struct ScreenshotCtx {
    pipeline: wgpu::ComputePipeline,
    pub image_dimentions: ImageDimentions,
    sampler_bind_group: wgpu::BindGroup,
    src_texture_bind_group_layout: wgpu::BindGroupLayout,
    dst_texture_bind_group_layout: wgpu::BindGroupLayout,
    binding_resources: Option<BindingResources>,

    // uniforms: Uniforms,
    // uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,

    data: wgpu::Buffer,
}

impl ScreenshotCtx {
    pub const DST_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
        let new_dims = ImageDimentions::new(width, height, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        if new_dims.linear_size() > self.image_dimentions.linear_size() {
            let image_dimentions =
                ImageDimentions::new(width, height, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);

            self.data = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Screen mapped Buffer"),
                size: image_dimentions.linear_size(),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: true,
            });
        }
        self.binding_resources = None;
        self.image_dimentions = new_dims;
    }

    fn get_binding_resources(
        device: &Device,
        image_dimentions: &ImageDimentions,
        src_texture_view: &wgpu::TextureView,
        src_texture_bind_group_layout: &wgpu::BindGroupLayout,
        dst_texture_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> BindingResources {
        let dst_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("DST Capture Texture"),
            size: wgpu::Extent3d {
                width: image_dimentions.width,
                height: image_dimentions.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DST_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        });
        let dst_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DST Capture Bind Group"),
            layout: dst_texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &dst_texture.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            }],
        });
        let src_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SRC Capture Bind Group"),
            layout: src_texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(src_texture_view),
            }],
        });

        BindingResources {
            src_texture_bind_group,
            dst_texture_bind_group,
            dst_texture,
        }
    }

    pub fn new(device: &Device, width: u32, height: u32) -> Self {
        let src_texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("SRC Capture Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });
        let sampler_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Sampler Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler {
                        filtering: true,
                        comparison: false,
                    },
                    count: None,
                }],
            });
        let dst_texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("DST Capture Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: Self::DST_FORMAT,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                }],
            });
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Uniform Capture Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        // min_binding_size: None,
                        min_binding_size: Some(
                            NonZeroU64::new(std::mem::size_of::<Uniforms>() as _).unwrap(),
                        ),
                    },
                    count: None,
                }],
            });

        let uniforms = Uniforms { samples: 1 };
        let uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Capture Uniform Buffer"),
            contents: uniforms.as_slice(),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Capture Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

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

        let sampler_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sampler Bind Group"),
            layout: &sampler_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&sampler),
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Capture Pipeline Layout"),
            bind_group_layouts: &[
                &src_texture_bind_group_layout,
                &sampler_bind_group_layout,
                &dst_texture_bind_group_layout,
                &uniform_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Capture Pipeleine"),
            layout: Some(&layout),
            module: &device.create_shader_module(&wgpu::ShaderModuleDescriptor {
                label: Some("Capture Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("./shader.wgsl").into()),
            }),
            entry_point: "main",
        });

        let image_dimentions =
            ImageDimentions::new(width, height, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);

        let data = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Screen mapped Buffer"),
            size: image_dimentions.linear_size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            image_dimentions,
            sampler_bind_group,
            src_texture_bind_group_layout,
            dst_texture_bind_group_layout,
            binding_resources: None,

            // uniforms,
            // uniform_buffer,
            uniform_bind_group,

            data,
        }
    }

    pub fn capture_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src_texture_view: &wgpu::TextureView,
    ) -> (Vec<u8>, ImageDimentions) {
        let binding_resources = self.binding_resources.get_or_insert_with(|| {
            Self::get_binding_resources(
                device,
                &self.image_dimentions,
                src_texture_view,
                &self.src_texture_bind_group_layout,
                &self.dst_texture_bind_group_layout,
            )
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Capture Encoder"),
        });

        {
            let mut capture_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Capture Pass"),
            });

            capture_pass.set_pipeline(&self.pipeline);
            capture_pass.set_bind_group(0, &binding_resources.src_texture_bind_group, &[]);
            capture_pass.set_bind_group(1, &self.sampler_bind_group, &[]);
            capture_pass.set_bind_group(2, &binding_resources.dst_texture_bind_group, &[]);
            capture_pass.set_bind_group(3, &self.uniform_bind_group, &[]);

            capture_pass.dispatch(
                dispatch_optimal_size(self.image_dimentions.width, SUBGROUP_SIZE),
                dispatch_optimal_size(self.image_dimentions.height, SUBGROUP_SIZE),
                1,
            );
        }

        let copy_size = wgpu::Extent3d {
            width: self.image_dimentions.width,
            height: self.image_dimentions.height,
            depth_or_array_layers: 1,
        };
        encoder.copy_texture_to_buffer(
            binding_resources.dst_texture.as_image_copy(),
            wgpu::ImageCopyBuffer {
                buffer: &self.data,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        NonZeroU32::new(self.image_dimentions.padded_bytes_per_row).unwrap(),
                    ),
                    rows_per_image: None,
                },
            },
            copy_size,
        );

        queue.submit(std::iter::once(encoder.finish()));

        (
            self.data
                .slice(0..self.image_dimentions.linear_size())
                .get_mapped_range()
                .to_vec(),
            self.image_dimentions,
        )
    }
}
