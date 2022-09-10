use crate::pvk::{VkCommandPool, VkDevice, VkDeviceProperties, VkQueue};
use ash::{prelude::VkResult, vk};

const FFT_SIZE: u32 = 1024 * 2;

#[derive(Debug)]
pub(crate) struct VkImage {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub memory_requirements: vk::MemoryRequirements,
}

impl VkImage {
    pub fn new(
        device: &VkDevice,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        image_create_info: &vk::ImageCreateInfo,
        image_memory_flags: vk::MemoryPropertyFlags,
    ) -> VkResult<Self> {
        let image = unsafe { device.create_image(image_create_info, None) }?;
        let memory_reqs = unsafe { device.get_image_memory_requirements(image) };

        let memory = device.alloc_memory(memory_properties, memory_reqs, image_memory_flags)?;
        unsafe { device.bind_image_memory(image, memory, 0) }?;
        Ok(Self {
            image,
            memory,
            memory_requirements: memory_reqs,
        })
    }
}

#[derive(Debug)]
pub(crate) struct VkTexture {
    pub image: VkImage,
    pub image_view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub usage_flags: vk::ImageUsageFlags,
    pub format: vk::Format,
}

impl VkTexture {
    pub fn new(
        device: &VkDevice,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        image_create_info: &vk::ImageCreateInfo,
        image_memory_flags: vk::MemoryPropertyFlags,
        sampler_create_info: &vk::SamplerCreateInfo,
    ) -> VkResult<Self> {
        let image = VkImage::new(
            device,
            memory_properties,
            image_create_info,
            image_memory_flags,
        )?;
        let image_view_info = vk::ImageViewCreateInfo::builder()
            .image(image.image)
            .format(image_create_info.format)
            .view_type(vk::ImageViewType::TYPE_2D)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let image_view = unsafe { device.create_image_view(&image_view_info, None) }?;
        let sampler = unsafe { device.create_sampler(sampler_create_info, None) }?;

        Ok(Self {
            image,
            image_view,
            sampler,
            usage_flags: image_create_info.usage,
            format: image_create_info.format,
        })
    }

    pub fn resize(
        &mut self,
        device: &VkDevice,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        width: u32,
        height: u32,
    ) -> VkResult<()> {
        self.destroy(device);
        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };
        let image_create_info = vk::ImageCreateInfo::builder()
            .format(self.format)
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .array_layers(1)
            .mip_levels(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(self.usage_flags)
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

        *self = Self::new(
            device,
            memory_properties,
            &image_create_info,
            image_memory_flags,
            &sampler_create_info,
        )?;

        Ok(())
    }

    pub fn destroy(&mut self, device: &VkDevice) {
        unsafe {
            device.destroy_sampler(self.sampler, None);
            device.destroy_image_view(self.image_view, None);
            device.destroy_image(self.image.image, None);
            device.free_memory(self.image.memory, None);
        }
    }
}

pub(crate) struct FftTexture<'a> {
    pub texture: VkTexture,
    staging_buffer: vk::Buffer,
    staging_buffer_memory: vk::DeviceMemory,
    mapped_memory: &'a mut [f32],
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,
}

impl<'a> FftTexture<'a> {
    pub fn new(
        device: &VkDevice,
        device_properties: &VkDeviceProperties,
        command_pool: &VkCommandPool,
    ) -> VkResult<Self> {
        let extent = vk::Extent3D {
            width: FFT_SIZE,
            height: 1,
            depth: 1,
        };
        let image_create_info = vk::ImageCreateInfo::builder()
            .format(vk::Format::R32_SFLOAT)
            .image_type(vk::ImageType::TYPE_1D)
            .extent(extent)
            .array_layers(1)
            .mip_levels(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(
                vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::TRANSFER_DST,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image_memory_flags = vk::MemoryPropertyFlags::DEVICE_LOCAL;
        let sampler_create_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(false)
            .max_anisotropy(0.);
        let image = VkImage::new(
            device,
            &device_properties.memory,
            &image_create_info,
            image_memory_flags,
        )?;
        let image_view_info = vk::ImageViewCreateInfo::builder()
            .image(image.image)
            .format(image_create_info.format)
            .view_type(vk::ImageViewType::TYPE_1D)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let image_view = unsafe { device.create_image_view(&image_view_info, None) }?;
        let sampler = unsafe { device.create_sampler(&sampler_create_info, None) }?;
        let texture = VkTexture {
            image,
            sampler,
            image_view,
            usage_flags: image_create_info.usage,
            format: image_create_info.format,
        };

        let size = (FFT_SIZE as usize * std::mem::size_of::<f32>()) as u64;
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let staging_buffer = unsafe { device.create_buffer(&buffer_create_info, None) }?;

        let staging_buffer_mem_reqs =
            unsafe { device.get_buffer_memory_requirements(staging_buffer) };

        let staging_buffer_memory = device.alloc_memory(
            &device_properties.memory,
            staging_buffer_mem_reqs,
            vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT
                | vk::MemoryPropertyFlags::HOST_CACHED,
        )?;
        unsafe { device.bind_buffer_memory(staging_buffer, staging_buffer_memory, 0) }?;

        let mapped_memory = unsafe {
            std::slice::from_raw_parts_mut::<f32>(
                device.map_memory(
                    staging_buffer_memory,
                    0,
                    staging_buffer_mem_reqs.size,
                    vk::MemoryMapFlags::empty(),
                )? as _,
                FFT_SIZE as _,
            )
        };

        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(command_pool.pool)
            .level(vk::CommandBufferLevel::PRIMARY);

        let command_buffer =
            unsafe { device.allocate_command_buffers(&command_buffer_allocate_info) }?[0];

        let fence = device.create_fence(true)?;

        Ok(Self {
            texture,
            staging_buffer,
            staging_buffer_memory,
            mapped_memory,
            command_buffer,
            fence,
        })
    }

    pub fn update(
        &mut self,
        data: &[f32],
        device: &VkDevice,
        submit_queue: &VkQueue,
    ) -> VkResult<()> {
        let regions = [vk::BufferImageCopy {
            image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            image_extent: vk::Extent3D {
                width: FFT_SIZE,
                height: 1,
                depth: 1,
            },
            buffer_offset: 0,
            buffer_row_length: FFT_SIZE,
            buffer_image_height: 1,
            image_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                layer_count: 1,
                base_array_layer: 0,
                mip_level: 0,
            },
        }];
        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        let submit_fence = self.fence;
        let command_buffer = self.command_buffer;

        unsafe { device.wait_for_fences(&[submit_fence], true, std::u64::MAX) }?;
        unsafe { device.reset_fences(&[submit_fence]) }?;

        unsafe {
            device.reset_command_buffer(
                command_buffer,
                vk::CommandBufferResetFlags::RELEASE_RESOURCES,
            )
        }?;

        let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe { device.begin_command_buffer(command_buffer, &command_buffer_begin_info) }?;

        let image = self.texture.image.image;
        let barrier = |old_layout, new_layout, sq, dq| {
            device.set_image_layout_with_subresource(
                command_buffer,
                image,
                old_layout,
                new_layout,
                subresource_range,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                Some(sq),
                Some(dq),
            )
        };

        barrier(
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            submit_queue.index,
            submit_queue.index,
        );
        self.mapped_memory.copy_from_slice(data);
        unsafe {
            device.cmd_copy_buffer_to_image(
                command_buffer,
                self.staging_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &regions,
            );
        }
        barrier(
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::GENERAL,
            submit_queue.index,
            submit_queue.index,
        );

        unsafe { device.end_command_buffer(command_buffer) }?;

        let command_buffers = [command_buffer];

        let submit_info = vk::SubmitInfo::builder().command_buffers(&command_buffers);

        unsafe { device.queue_submit(submit_queue.queue, &[submit_info.build()], submit_fence) }?;

        Ok(())
    }

    pub fn destroy(&mut self, device: &VkDevice) {
        unsafe {
            device.destroy_fence(self.fence, None);
            self.texture.destroy(device);
            device.free_memory(self.staging_buffer_memory, None);
            device.destroy_buffer(self.staging_buffer, None);
        }
    }
}
