use ash::{
    prelude::VkResult,
    version::{DeviceV1_0, InstanceV1_0},
    vk, Device,
};
use std::sync::Arc;

use crate::{
    command_pool::{CommandBuffer, CommandBufferPool},
    instance::VkInstance,
    renderpass_and_pipeline::VkRenderPass,
    swapchain::VkSwapchain,
};

pub struct VkDevice {
    pub device: Arc<RawDevice>,
    pub physical_device: vk::PhysicalDevice,
}

pub struct RawDevice {
    pub device: Device,
}

impl std::ops::Deref for RawDevice {
    type Target = Device;

    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

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
            properties,
            features,
        }
    }

    pub fn create_commmand_buffer(
        &self,
        queue_family_index: u32,
        num_command_buffers: u32,
    ) -> VkResult<CommandBufferPool> {
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

        let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);

        let command_buffers: VkResult<Vec<CommandBuffer>> = command_buffers
            .iter()
            .map(|&command_buffer| {
                let fence = unsafe { self.create_fence(&fence_info, None) }?;
                Ok(CommandBuffer {
                    command_buffer,
                    fence,
                })
            })
            .collect();
        let command_buffers = command_buffers?;

        Ok(CommandBufferPool {
            pool,
            command_buffers,
            device: self.device.clone(),
            active_command_buffer: 0,
        })
    }

    pub fn create_vk_render_pass(&self, swapchain: &mut VkSwapchain) -> VkResult<VkRenderPass> {
        let renderpass_attachments = [vk::AttachmentDescription::builder()
            .format(swapchain.format)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
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

        swapchain.fill_framebuffers(&self.device, &renderpass)?;

        Ok(VkRenderPass {
            render_pass: renderpass,
            device: self.device.clone(),
        })
    }
}

impl Drop for RawDevice {
    fn drop(&mut self) {
        unsafe { self.device.destroy_device(None) };
    }
}
