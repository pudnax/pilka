use crate::device::{RawDevice, VkDevice};
use ash::{version::DeviceV1_0, vk};
use std::sync::Arc;

// TODO(#13): Call vkResetCommandPool before reusing it in another frame.
//
// Otherwise the pool will keep on growing until you run out of memory
pub struct VkCommandPool {
    pub pool: vk::CommandPool,
    pub active_command: usize,
    pub command_buffers: Vec<vk::CommandBuffer>,
    pub fences: Vec<vk::Fence>,
    pub device: Arc<RawDevice>,
}

impl VkCommandPool {
    pub fn record_submit_commandbuffer<F: FnOnce(&VkDevice, vk::CommandBuffer)>(
        &mut self,
        device: &VkDevice,
        submit_queue: vk::Queue,
        wait_mask: &[vk::PipelineStageFlags],
        wait_semaphores: &[vk::Semaphore],
        signal_semaphores: &[vk::Semaphore],
        f: F,
    ) {
        let submit_fence = self.fences[self.active_command];
        let command_buffer = self.command_buffers[self.active_command];

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

        self.active_command = (self.active_command + 1) % self.fences.len();
    }
}

impl Drop for VkCommandPool {
    fn drop(&mut self) {
        unsafe {
            for &fence in &self.fences {
                self.device.destroy_fence(fence, None);
            }

            self.device.destroy_command_pool(self.pool, None);
        }
    }
}
