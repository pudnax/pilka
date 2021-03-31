use crate::{
    ash::version::{DeviceV1_0, InstanceV1_0},
    ash::vk,
    device::{RawDevice, VkDevice},
    instance::{VkInstance, VkQueues},
};
use ash::prelude::VkResult;
use ktx::{Ktx, KtxInfo};
use std::path::Path;
use std::sync::Arc;

pub struct VkTexture {
    device: Arc<RawDevice>,
    image: vk::Image,
    view: vk::ImageView,
    image_layout: vk::ImageLayout,
    memory: vk::DeviceMemory,
    width: u32,
    height: u32,
    mip_levels: u32,
    layer_count: u32,
    descriptor: vk::DescriptorImageInfo,
    sampler: vk::Sampler,
}

impl VkTexture {
    #[allow(clippy::clippy::too_many_arguments)]
    pub fn from_ktx<P: AsRef<Path>>(
        filename: P,
        instance: &VkInstance,
        device: &VkDevice,
        format: vk::Format,
        command_pool: &vk::CommandPool,
        copy_queue: &vk::Queue,
        image_usage_flags: vk::ImageUsageFlags,
        image_layout: vk::ImageLayout,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let ktx_image = Ktx::new(std::fs::read(filename)?);

        let ktx_image_width = ktx_image.pixel_width();
        let ktx_image_height = ktx_image.pixel_height();
        let mip_levels = ktx_image.mipmap_levels();

        let ktx_texture = ktx_image.textures().flatten().copied().collect::<Vec<_>>();
        let ktx_texture_size = ktx_texture.len() as u64;
        let ktx_offsets: Vec<u64> = ktx_image
            .textures()
            .map(|tex| tex.len())
            .scan(0, |state, x| {
                *state += x;
                Some(*state as _)
            })
            .collect();

        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(device.physical_device) };

        let cmd_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(*command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let copy_cmd = unsafe { device.allocate_command_buffers(&cmd_buffer_allocate_info) }?[0];

        // Flags?
        let begin_info = vk::CommandBufferBeginInfo::builder();
        unsafe { device.begin_command_buffer(copy_cmd, &begin_info) }?;

        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(ktx_texture_size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let staging_buffer = unsafe { device.create_buffer(&buffer_create_info, None) }?;

        let staging_buffer_mem_reqs =
            unsafe { device.get_buffer_memory_requirements(staging_buffer) };

        let staging_buffer_memory = device.alloc_memory(
            &memory_properties,
            staging_buffer_mem_reqs,
            vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT
                | vk::MemoryPropertyFlags::HOST_CACHED,
        )?;
        unsafe { device.bind_buffer_memory(staging_buffer, staging_buffer_memory, 0) }?;

        let data = unsafe {
            std::slice::from_raw_parts_mut(
                device.map_memory(
                    staging_buffer_memory,
                    0,
                    staging_buffer_mem_reqs.size,
                    vk::MemoryMapFlags::empty(),
                )? as *mut u8,
                staging_buffer_mem_reqs.size as usize,
            )
        };
        data.copy_from_slice(&ktx_texture);
        unsafe { device.unmap_memory(staging_buffer_memory) };

        let buffer_copy_regions: Vec<_> = (0..mip_levels)
            .map(|i| {
                vk::BufferImageCopy::builder()
                    .image_subresource(
                        vk::ImageSubresourceLayers::builder()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .mip_level(i as _)
                            .base_array_layer(0)
                            .layer_count(1)
                            .build(),
                    )
                    .image_extent(vk::Extent3D {
                        width: 1.max(ktx_image_width >> i),
                        height: 1.max(ktx_image_height >> i),
                        depth: 1,
                    })
                    .buffer_offset(ktx_offsets[i as usize])
                    .build()
            })
            .collect();

        let image_create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .mip_levels(mip_levels)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .extent(vk::Extent3D {
                width: ktx_image_width,
                height: ktx_image_height,
                depth: 1,
            })
            .usage(image_usage_flags | vk::ImageUsageFlags::TRANSFER_DST);

        let image = unsafe { device.create_image(&image_create_info, None) }?;
        let image_mem_reqs = unsafe { device.get_image_memory_requirements(image) };

        let image_memory = device.alloc_memory(
            &memory_properties,
            image_mem_reqs,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        unsafe { device.bind_image_memory(image, image_memory, 0) }?;

        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(mip_levels)
            .layer_count(1)
            .build();

        device.set_image_layout_with_subresource(
            copy_cmd,
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            subresource_range,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::ALL_COMMANDS,
        );

        unsafe {
            device.cmd_copy_buffer_to_image(
                copy_cmd,
                staging_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &buffer_copy_regions,
            )
        };

        device.set_image_layout_with_subresource(
            copy_cmd,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            image_layout,
            subresource_range,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::ALL_COMMANDS,
        );

        device.flush_cmd_buffer(&copy_cmd, &copy_queue, command_pool, true)?;

        unsafe {
            device.free_memory(staging_buffer_memory, None);
            device.destroy_buffer(staging_buffer, None);
        }

        let sampler_create_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .mip_lod_bias(0.)
            .compare_op(vk::CompareOp::NEVER)
            .min_lod(0.)
            .max_lod(mip_levels as _)
            .max_anisotropy(1.0)
            .anisotropy_enable(false)
            .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE);
        let sampler = unsafe { device.create_sampler(&sampler_create_info, None) }?;

        let view_create_info = vk::ImageViewCreateInfo::builder()
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(vk::ComponentMapping {
                r: vk::ComponentSwizzle::R,
                g: vk::ComponentSwizzle::G,
                b: vk::ComponentSwizzle::B,
                a: vk::ComponentSwizzle::A,
            })
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(mip_levels)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            )
            .image(image);
        let image_view = unsafe { device.create_image_view(&view_create_info, None) }?;

        let descriptor = vk::DescriptorImageInfo::builder()
            .sampler(sampler)
            .image_view(image_view)
            .image_layout(image_layout)
            .build();

        Ok(Self {
            device: Arc::clone(&device.device),
            image,
            view: image_view,
            image_layout,
            memory: image_memory,
            width: ktx_image_width,
            height: ktx_image_width,
            mip_levels,
            layer_count: 1,
            sampler,
            descriptor,
        })
    }

    pub fn new(
        instance: &VkInstance,
        device: &VkDevice,
        width: u32,
        height: u32,
        command_pool: &vk::CommandPool,
        queues: &VkQueues,
        format: vk::Format,
    ) -> VkResult<Self> {
        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(device.physical_device) };

        let mut queue_indices = vec![];
        let sharing_concurrent = queues.graphics_queue.index != queues.compute_queue.index;
        let image_create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .sharing_mode(if sharing_concurrent {
                vk::SharingMode::CONCURRENT
            } else {
                vk::SharingMode::EXCLUSIVE
            })
            .queue_family_indices({
                if sharing_concurrent {
                    queue_indices.extend_from_slice(&[
                        queues.graphics_queue.index,
                        queues.compute_queue.index,
                    ]);
                }
                &queue_indices
            })
            .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::STORAGE);

        let image = unsafe { device.create_image(&image_create_info, None) }?;
        let image_mem_reqs = unsafe { device.get_image_memory_requirements(image) };

        let image_memory = device.alloc_memory(
            &memory_properties,
            image_mem_reqs,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        unsafe { device.bind_image_memory(image, image_memory, 0) }?;

        let cmd_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(*command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let layout_cmd = unsafe { device.allocate_command_buffers(&cmd_buffer_allocate_info) }?[0];

        let begin_info = vk::CommandBufferBeginInfo::builder();
        unsafe { device.begin_command_buffer(layout_cmd, &begin_info) }?;

        let image_layout = vk::ImageLayout::GENERAL;
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1)
            .build();
        device.set_image_layout_with_subresource(
            layout_cmd,
            image,
            vk::ImageLayout::UNDEFINED,
            image_layout,
            subresource_range,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::ALL_COMMANDS,
        );

        device.flush_cmd_buffer(
            &layout_cmd,
            &queues.graphics_queue.queue,
            &command_pool,
            true,
        )?;

        let sampler_create_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_BORDER)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_BORDER)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_BORDER)
            .mip_lod_bias(0.)
            .compare_op(vk::CompareOp::NEVER)
            .min_lod(0.)
            // Duh
            .max_lod(1.)
            .max_anisotropy(1.0)
            .anisotropy_enable(false)
            .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE);
        let sampler = unsafe { device.create_sampler(&sampler_create_info, None) }?;

        let view_create_info = vk::ImageViewCreateInfo::builder()
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(vk::ComponentMapping {
                r: vk::ComponentSwizzle::R,
                g: vk::ComponentSwizzle::G,
                b: vk::ComponentSwizzle::B,
                a: vk::ComponentSwizzle::A,
            })
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            )
            .image(image);
        let image_view = unsafe { device.create_image_view(&view_create_info, None) }?;

        let descriptor = vk::DescriptorImageInfo::builder()
            .sampler(sampler)
            .image_view(image_view)
            .image_layout(image_layout)
            .build();

        Ok(Self {
            device: Arc::clone(&device.device),
            image,
            view: image_view,
            image_layout,
            memory: image_memory,
            width,
            height,
            mip_levels: 1,
            layer_count: 1,
            sampler,
            descriptor,
        })
    }
}

impl Drop for VkTexture {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image_view(self.view, None);
            self.device.destroy_image(self.image, None);
            self.device.destroy_sampler(self.sampler, None);
            self.device.free_memory(self.memory, None);
        }
    }
}
