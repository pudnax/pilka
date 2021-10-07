use std::num::NonZeroU32;

use pilka_types::ImageDimentions;
use wgpu::{BufferView, Device};

struct BindingResources {
    src_texture_bind_group: wgpu::BindGroup,
    dst_texture_bind_group: wgpu::BindGroup,
    dst_texture: wgpu::Texture,
}

pub struct ScreenshotCtx {
    pipeline: wgpu::ComputePipeline,
    pub image_dimentions: ImageDimentions,
    sampler_bind_group: wgpu::BindGroup,
    src_texture_bind_group_layout: wgpu::BindGroupLayout,
    dst_texture_bind_group_layout: wgpu::BindGroupLayout,
    binding_resources: Option<BindingResources>,

    data: wgpu::Buffer,
}

impl ScreenshotCtx {
    pub const DST_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

    pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
        if width > self.image_dimentions.width || height > self.image_dimentions.height {
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
        self.image_dimentions =
            ImageDimentions::new(width, height, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    }

    fn get_binding_resources(
        device: &Device,
        image_dimentions: &ImageDimentions,
        src_texture: &wgpu::Texture,
        src_texture_bind_group_layout: &wgpu::BindGroupLayout,
        dst_texture_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> BindingResources {
        let dst_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("DST Capturer Texture"),
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
                resource: wgpu::BindingResource::TextureView(
                    &src_texture.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
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
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Capturer Pipeleine"),
            layout: Some(&layout),
            module: &device.create_shader_module(&wgpu::ShaderModuleDescriptor {
                label: Some("Capturer Shader"),
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
            mapped_at_creation: true,
        });

        Self {
            pipeline,
            image_dimentions,
            sampler_bind_group,
            src_texture_bind_group_layout,
            dst_texture_bind_group_layout,
            binding_resources: None,

            data,
        }
    }

    pub fn capture_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src_texture: &wgpu::Texture,
    ) -> (BufferView, ImageDimentions) {
        let binding_resources = self.binding_resources.get_or_insert_with(|| {
            Self::get_binding_resources(
                device,
                &self.image_dimentions,
                src_texture,
                &self.src_texture_bind_group_layout,
                &self.dst_texture_bind_group_layout,
            )
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Capturer Encoder"),
        });

        {
            let mut capture_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Capture Pass"),
            });

            capture_pass.set_pipeline(&self.pipeline);
            capture_pass.set_bind_group(0, &binding_resources.src_texture_bind_group, &[]);
            capture_pass.set_bind_group(1, &self.sampler_bind_group, &[]);
            capture_pass.set_bind_group(2, &binding_resources.dst_texture_bind_group, &[]);

            capture_pass.dispatch(0, 0, 0);
        }

        let source = wgpu::ImageCopyTexture {
            texture: &binding_resources.dst_texture,
            mip_level: 1,
            origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
            aspect: wgpu::TextureAspect::All,
        };
        let destination = wgpu::ImageCopyBuffer {
            buffer: &self.data,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(
                    NonZeroU32::new(self.image_dimentions.padded_bytes_per_row).unwrap(),
                ),
                rows_per_image: Some(NonZeroU32::new(self.image_dimentions.height).unwrap()),
            },
        };
        let copy_size = wgpu::Extent3d {
            width: self.image_dimentions.width,
            height: self.image_dimentions.height,
            depth_or_array_layers: 1,
        };
        encoder.copy_texture_to_buffer(source, destination, copy_size);

        queue.submit(std::iter::once(encoder.finish()));

        (
            self.data
                .slice(0..self.image_dimentions.linear_size())
                .get_mapped_range(),
            self.image_dimentions,
        )
    }
}
