use crate::device::{RawDevice, VkDevice};
use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct CommandBuffer {
    pub command_buffer: vk::CommandBuffer,
    pub fence: vk::Fence,
}

// TODO(#13): Call vkResetCommandPool before reusing it in another frame.
//
// Otherwise the pool will keep on growing until you run out of memory
pub struct CommandBufferPool {
    pub pool: vk::CommandPool,
    pub command_buffers: Vec<CommandBuffer>,
    pub device: Arc<RawDevice>,
    pub active_command_buffer: usize,
}

impl CommandBufferPool {
    unsafe fn create_fence(&self, signaled: bool) -> VkResult<vk::Fence> {
        let device = &self.device;
        let mut flags = vk::FenceCreateFlags::empty();
        if signaled {
            flags |= vk::FenceCreateFlags::SIGNALED;
        }
        Ok(device.create_fence(&vk::FenceCreateInfo::builder().flags(flags).build(), None)?)
    }

    unsafe fn create_semaphore(&self) -> VkResult<vk::Semaphore> {
        let device = &self.device;
        Ok(device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?)
    }

    pub fn record_submit_commandbuffer<F: FnOnce(&VkDevice, vk::CommandBuffer)>(
        &mut self,
        device: &VkDevice,
        submit_queue: vk::Queue,
        wait_mask: &[vk::PipelineStageFlags],
        wait_semaphores: &[vk::Semaphore],
        signal_semaphores: &[vk::Semaphore],
        f: F,
    ) {
        let submit_fence = self.command_buffers[self.active_command_buffer].fence;
        let command_buffer = self.command_buffers[self.active_command_buffer].command_buffer;

        unsafe {
            device
                .wait_for_fences(&[submit_fence], true, std::u64::MAX)
                .expect("Wait for fences failed.")
        };
        unsafe {
            device
                .reset_fences(&[submit_fence])
                .expect("Reset fences failed.")
        };

        unsafe {
            device
                .reset_command_buffer(
                    command_buffer,
                    vk::CommandBufferResetFlags::RELEASE_RESOURCES,
                )
                .expect("Reset command buffer failed.")
        };

        let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            device
                .begin_command_buffer(command_buffer, &command_buffer_begin_info)
                .expect("Begin cammandbuffer.")
        };
        f(device, command_buffer);
        unsafe {
            device
                .end_command_buffer(command_buffer)
                .expect("End commandbuffer")
        };

        let command_buffers = vec![command_buffer];

        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_mask)
            .command_buffers(&command_buffers)
            .signal_semaphores(signal_semaphores);

        unsafe {
            device
                .queue_submit(submit_queue, &[submit_info.build()], submit_fence)
                .expect("Queue submit failed.")
        };

        self.active_command_buffer = (self.active_command_buffer + 1) % self.command_buffers.len();
    }
}

impl Drop for CommandBufferPool {
    fn drop(&mut self) {
        unsafe {
            for command_buffer in &self.command_buffers {
                self.device.destroy_fence(command_buffer.fence, None);
            }

            self.device.destroy_command_pool(self.pool, None);
        }
    }
}
