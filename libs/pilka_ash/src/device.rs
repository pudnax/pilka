use ash::{
    extensions::khr,
    prelude::VkResult,
    version::{DeviceV1_0, InstanceV1_0},
    vk, Device,
};
use std::sync::Arc;

use crate::{
    command_pool::VkCommandPool,
    instance::{VkInstance, VkQueues},
    renderpass_and_pipeline::VkRenderPass,
    surface::VkSurface,
    swapchain::VkSwapchain,
    utils,
};

pub struct VkDevice {
    pub device: Arc<RawDevice>,
    pub physical_device: vk::PhysicalDevice,
    pub memory_properties: vk::PhysicalDeviceMemoryProperties,
}

pub struct RawDevice {
    device: Device,
}

impl std::fmt::Debug for RawDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RawDevice btw")
    }
}

impl RawDevice {
    pub fn new(device: Device) -> Self {
        Self { device }
    }

    pub fn handle(&self) -> vk::Device {
        self.device.handle()
    }
}

impl std::ops::Deref for RawDevice {
    type Target = Device;

    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

#[derive(Debug)]
pub struct VkDeviceProperties {
    pub memory: vk::PhysicalDeviceMemoryProperties,
    pub features: vk::PhysicalDeviceFeatures,
    pub properties: vk::PhysicalDeviceProperties,
}

impl std::ops::Deref for VkDevice {
    type Target = ash::Device;

    fn deref(&self) -> &Self::Target {
        &self.device.device
    }
}

// Do not do this, you lack!
//
// impl std::ops::DerefMut for VkDevice {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         Arc::get_mut(&mut self.device)
//     }
// }

impl VkDevice {
    pub fn get_device_properties(&self, instance: &VkInstance) -> VkDeviceProperties {
        let (properties, features, memory) = unsafe {
            let properties = instance
                .instance
                .get_physical_device_properties(self.physical_device);
            let features = instance
                .instance
                .get_physical_device_features(self.physical_device);
            let memory = instance
                .instance
                .get_physical_device_memory_properties(self.physical_device);
            (properties, features, memory)
        };

        VkDeviceProperties {
            memory,
            features,
            properties,
        }
    }

    pub fn create_fence(&self, signaled: bool) -> VkResult<vk::Fence> {
        let device = &self.device;
        let mut flags = vk::FenceCreateFlags::empty();
        if signaled {
            flags |= vk::FenceCreateFlags::SIGNALED;
        }
        unsafe { device.create_fence(&vk::FenceCreateInfo::builder().flags(flags).build(), None) }
    }

    pub fn create_semaphore(&self) -> VkResult<vk::Semaphore> {
        let device = &self.device;
        unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }
    }

    pub fn create_vk_command_pool(
        &self,
        queue_family_index: u32,
        num_command_buffers: u32,
    ) -> VkResult<VkCommandPool> {
        let pool_create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);

        let pool = unsafe { self.create_command_pool(&pool_create_info, None) }?;

        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(num_command_buffers)
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY);

        let command_buffers =
            unsafe { self.allocate_command_buffers(&command_buffer_allocate_info) }?;

        let fences: Result<_, _> = (0..num_command_buffers)
            .map(|_| self.create_fence(true))
            .collect();
        let fences = fences?;

        Ok(VkCommandPool {
            pool,
            command_buffers,
            fences,
            device: self.device.clone(),
            active_command: 0,
        })
    }

    pub fn create_vk_render_pass(&self, format: vk::Format) -> VkResult<VkRenderPass> {
        let renderpass_attachments = [vk::AttachmentDescription::builder()
            .format(format)
            .initial_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .build()];
        let color_attachment_refs = [vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build()];

        let dependencies = [vk::SubpassDependency::builder()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_subpass(0)
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )
            .build()];

        let subpasses = [vk::SubpassDescription::builder()
            .color_attachments(&color_attachment_refs)
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .build()];

        // Depth textute? Never heard about it.
        let renderpass_create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&renderpass_attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies);

        let renderpass = unsafe {
            self.device
                .create_render_pass(&renderpass_create_info, None)
        }?;

        Ok(VkRenderPass {
            render_pass: renderpass,
            device: self.device.clone(),
        })
    }

    pub fn create_swapchain(
        &self,
        swapchain_loader: khr::Swapchain,
        surface: &VkSurface,
        queues: &VkQueues,
    ) -> VkResult<VkSwapchain> {
        let surface_capabilities = surface.get_capabilities(self)?;

        let desired_image_count = {
            let n = surface_capabilities.min_image_count + 3;
            n.min(surface_capabilities.max_image_count).max(n)
        };

        let present_mode = surface
            .get_present_modes(self)?
            .iter()
            .cloned()
            .find(|&mode| mode == vk::PresentModeKHR::FIFO)
            .unwrap_or(vk::PresentModeKHR::FIFO);

        let surface_format = {
            let acceptable_formats = {
                [
                    vk::Format::R8G8B8_SRGB,
                    vk::Format::B8G8R8_SRGB,
                    vk::Format::R8G8B8A8_SRGB,
                    vk::Format::B8G8R8A8_SRGB,
                    vk::Format::A8B8G8R8_SRGB_PACK32,
                ]
            };
            surface
                .get_formats(self)?
                .into_iter()
                .find(|sfmt| acceptable_formats.contains(&sfmt.format))
                .expect("Unable to find suitable surface format.")
        };
        let format = surface_format.format;

        let pre_transform = if surface_capabilities
            .supported_transforms
            .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
        {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            surface_capabilities.current_transform
        };

        let graphics_queue_family_index = [queues.graphics_queue.index];
        // We've choosed `COLOR_ATTACHMENT` for the same reason like with queue family.
        let swapchain_usage =
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
        let extent = surface_capabilities.current_extent;
        let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(surface.surface)
            .image_format(format)
            .image_usage(swapchain_usage)
            .image_extent(extent)
            .image_color_space(surface_format.color_space)
            .min_image_count(desired_image_count)
            .image_array_layers(surface_capabilities.max_image_array_layers)
            .queue_family_indices(&graphics_queue_family_index)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(surface_capabilities.supported_composite_alpha)
            .present_mode(present_mode)
            .clipped(true);

        let swapchain = unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None)? };

        let present_images = unsafe { swapchain_loader.get_swapchain_images(swapchain)? };
        let present_image_views = VkSwapchain::create_image_views(&present_images, format, self)?;

        Ok(VkSwapchain {
            swapchain,
            swapchain_loader,
            format,
            images: present_images,
            image_views: present_image_views,
            device: self.device.clone(),
            info: swapchain_create_info.build(),
        })
    }

    pub fn alloc_memory(
        &self,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        allocation_reqs: vk::MemoryRequirements,
        flags: vk::MemoryPropertyFlags,
    ) -> VkResult<vk::DeviceMemory> {
        let memory_type_index =
            utils::find_memory_type_index(&allocation_reqs, memory_properties, flags).unwrap();
        let alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(allocation_reqs.size)
            .memory_type_index(memory_type_index);
        unsafe { self.device.allocate_memory(&alloc_info, None) }
    }

    pub fn flush_cmd_buffer(
        &self,
        cmd_buffer: &vk::CommandBuffer,
        queue: &vk::Queue,
        pool: &vk::CommandPool,
        free: bool,
    ) -> VkResult<()> {
        unsafe { self.end_command_buffer(*cmd_buffer) }?;

        let command_buffers = [*cmd_buffer];
        let submit_info = vk::SubmitInfo::builder().command_buffers(&command_buffers);

        let fence = self.create_fence(false)?;

        let submits = [submit_info.build()];
        unsafe { self.queue_submit(*queue, &submits, fence) }?;

        let fences = [fence];
        unsafe { self.wait_for_fences(&fences, true, !0) }?;

        unsafe { self.destroy_fence(fence, None) };

        if free {
            unsafe { self.free_command_buffers(*pool, &command_buffers) };
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_image_layout_with_subresource(
        &self,
        cmd_buffer: vk::CommandBuffer,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        subresource_range: vk::ImageSubresourceRange,
        src_stage_mask: vk::PipelineStageFlags,
        dst_stage_mask: vk::PipelineStageFlags,
        src_queue_family_index: Option<u32>,
        dst_queue_family_index: Option<u32>,
    ) {
        let mut image_memory_barrier = vk::ImageMemoryBarrier::builder()
            .src_queue_family_index(src_queue_family_index.unwrap_or(0))
            .dst_queue_family_index(dst_queue_family_index.unwrap_or(0))
            .old_layout(old_layout)
            .new_layout(new_layout)
            .image(image)
            .subresource_range(subresource_range);

        use vk::{AccessFlags, ImageLayout};
        image_memory_barrier.src_access_mask = match old_layout {
            ImageLayout::UNDEFINED => AccessFlags::empty(),
            ImageLayout::PREINITIALIZED => AccessFlags::HOST_WRITE,
            ImageLayout::COLOR_ATTACHMENT_OPTIMAL => AccessFlags::COLOR_ATTACHMENT_WRITE,
            ImageLayout::PRESENT_SRC_KHR => AccessFlags::MEMORY_WRITE,
            ImageLayout::GENERAL => AccessFlags::MEMORY_READ,
            ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL => {
                AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
            }
            ImageLayout::TRANSFER_SRC_OPTIMAL => AccessFlags::TRANSFER_READ,
            ImageLayout::TRANSFER_DST_OPTIMAL => AccessFlags::TRANSFER_WRITE,
            // ImageLayout::SHADER_READ_ONLY_OPTIMAL => AccessFlags::SHADER_READ,
            ImageLayout::SHADER_READ_ONLY_OPTIMAL => {
                AccessFlags::MEMORY_READ | AccessFlags::MEMORY_WRITE
            }
            _ => AccessFlags::empty(),
        };

        image_memory_barrier.dst_access_mask = match new_layout {
            ImageLayout::TRANSFER_DST_OPTIMAL => AccessFlags::TRANSFER_WRITE,
            ImageLayout::TRANSFER_SRC_OPTIMAL => AccessFlags::TRANSFER_READ,
            ImageLayout::COLOR_ATTACHMENT_OPTIMAL => AccessFlags::COLOR_ATTACHMENT_WRITE,
            ImageLayout::GENERAL => AccessFlags::MEMORY_READ,
            ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL => {
                image_memory_barrier.dst_access_mask | AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
            }
            ImageLayout::SHADER_READ_ONLY_OPTIMAL => {
                // if image_memory_barrier.src_access_mask.is_empty()
                //     && old_layout != ImageLayout::UNDEFINED
                // {
                //     image_memory_barrier.src_access_mask =
                //         AccessFlags::HOST_WRITE | AccessFlags::TRANSFER_WRITE;
                // }
                // AccessFlags::SHADER_READ | AccessFlags::MEMORY_READ | AccessFlags::MEMORY_WRITE
                AccessFlags::MEMORY_READ | AccessFlags::MEMORY_WRITE
            }
            ImageLayout::PRESENT_SRC_KHR => AccessFlags::MEMORY_WRITE,
            _ => AccessFlags::empty(),
        };

        let image_barriers = [image_memory_barrier.build()];
        unsafe {
            self.cmd_pipeline_barrier(
                cmd_buffer,
                src_stage_mask,
                dst_stage_mask,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &image_barriers,
            );
        }
    }

    pub fn set_image_layout(
        &self,
        cmd_buffer: vk::CommandBuffer,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        src_stage_mask: vk::PipelineStageFlags,
        dst_stage_mask: vk::PipelineStageFlags,
    ) {
        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        self.set_image_layout_with_subresource(
            cmd_buffer,
            image,
            old_layout,
            new_layout,
            subresource_range,
            src_stage_mask,
            dst_stage_mask,
            None,
            None,
        );
    }

    pub fn set_image_layout_all_commands(
        &self,
        cmd_buffer: vk::CommandBuffer,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) {
        self.set_image_layout(
            cmd_buffer,
            image,
            old_layout,
            new_layout,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::ALL_COMMANDS,
        );
    }

    pub fn copy_image(
        &self,
        command_buffer: vk::CommandBuffer,
        src_image: vk::Image,
        dst_image: vk::Image,
        extent: vk::Extent3D,
    ) {
        let zero_offset = vk::Offset3D::default();
        let copy_area = vk::ImageCopy::builder()
            .src_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_offset(zero_offset)
            .dst_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .dst_offset(zero_offset)
            .extent(extent)
            .build();
        unsafe {
            self.cmd_copy_image(
                command_buffer,
                src_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                dst_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[copy_area],
            )
        };
    }

    pub fn blit_image(
        &self,
        command_buffer: vk::CommandBuffer,
        src_image: vk::Image,
        dst_image: vk::Image,
        src_extent: vk::Extent3D,
        dst_extent: vk::Extent3D,
    ) {
        let src_offset = [
            vk::Offset3D { x: 0, y: 0, z: 0 },
            vk::Offset3D {
                x: src_extent.width as i32,
                y: src_extent.height as i32,
                z: src_extent.depth as i32,
            },
        ];
        let dst_offset = [
            vk::Offset3D { x: 0, y: 0, z: 0 },
            vk::Offset3D {
                x: dst_extent.width as i32,
                y: dst_extent.height as i32,
                z: dst_extent.depth as i32,
            },
        ];
        let blit_region = [vk::ImageBlit::builder()
            .src_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_array_layer: 0,
                layer_count: 1,
                mip_level: 0,
            })
            .dst_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_array_layer: 0,
                layer_count: 1,
                mip_level: 0,
            })
            .src_offsets(src_offset)
            .dst_offsets(dst_offset)
            .build()];

        unsafe {
            self.device.cmd_blit_image(
                command_buffer,
                src_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                dst_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                blit_region.as_ref(),
                vk::Filter::NEAREST,
            )
        };
    }

    pub fn create_vk_buffer_from_slice<T>(
        &self,
        usage_flags: vk::BufferUsageFlags,
        memory_prop_flags: vk::MemoryPropertyFlags,
        data: &[T],
    ) -> VkResult<VkBuffer<T>> {
        let size = (data.len() * std::mem::size_of::<T>()) as u64;
        let mut buffer = self.create_vk_buffer(usage_flags, memory_prop_flags, size)?;
        buffer.mapped = {
            let tmp = unsafe {
                std::slice::from_raw_parts_mut::<T>(
                    self.map_memory(buffer.memory, 0, size, vk::MemoryMapFlags::empty())? as _,
                    data.len(),
                )
            };
            unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), tmp.as_mut_ptr(), data.len()) };
            Some(tmp)
        };
        Ok(buffer)
    }

    pub fn create_vk_buffer<T>(
        &self,
        usage_flags: vk::BufferUsageFlags,
        memory_prop_flags: vk::MemoryPropertyFlags,
        size: vk::DeviceSize,
    ) -> VkResult<VkBuffer<T>> {
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage_flags);
        let buffer = unsafe { self.create_buffer(&buffer_create_info, None) }?;

        let memory_reqs = unsafe { self.get_buffer_memory_requirements(buffer) };

        let memory_type_index =
            utils::find_memorytype_index(&memory_reqs, &self.memory_properties, memory_prop_flags)
                .unwrap();
        let mut mem_alloc_flags = vk::MemoryAllocateFlagsInfoKHR::default();
        let alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_reqs.size)
            .memory_type_index(memory_type_index)
            .push_next({
                if usage_flags.contains(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS) {
                    mem_alloc_flags.flags = vk::MemoryAllocateFlagsKHR::DEVICE_ADDRESS_KHR;
                }
                &mut mem_alloc_flags
            });
        let buffer_memory = unsafe { self.device.allocate_memory(&alloc_info, None) }?;

        let descriptor = vk::DescriptorBufferInfo::builder()
            .offset(0)
            .buffer(buffer)
            .range(vk::WHOLE_SIZE)
            .build();

        unsafe { self.bind_buffer_memory(buffer, buffer_memory, 0) }?;

        Ok(VkBuffer {
            buffer,
            memory: buffer_memory,
            mem_reqs: memory_reqs,
            descriptor,
            mapped: None,
        })
    }
}

pub struct VkBuffer<'a, T> {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mem_reqs: vk::MemoryRequirements,
    descriptor: vk::DescriptorBufferInfo,
    mapped: Option<&'a mut [T]>,
}

impl<T> VkBuffer<'_, T> {
    pub fn map(&mut self, device: &VkDevice) -> VkResult<()> {
        self.mapped = {
            let size = self.mem_reqs.size;
            let tmp = unsafe {
                std::slice::from_raw_parts_mut::<T>(
                    device.map_memory(self.memory, 0, size, vk::MemoryMapFlags::empty())? as _,
                    size as usize,
                )
            };
            Some(tmp)
        };
        Ok(())
    }
}

impl<'a, T> VkBuffer<'a, T> {
    fn destroy(&mut self, device: &VkDevice) {
        unsafe {
            device.free_memory(self.memory, None);
            device.destroy_buffer(self.buffer, None);
        }
    }
}

impl Drop for RawDevice {
    fn drop(&mut self) {
        unsafe { self.device.destroy_device(None) };
    }
}
