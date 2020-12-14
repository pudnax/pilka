use crate::device::RawDevice;
use ash::{extensions::khr::Swapchain, version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct VkSwapchain {
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_loader: Swapchain,
    pub images: Vec<vk::Image>,
    pub image_views: Vec<vk::ImageView>,
    pub format: vk::SurfaceFormatKHR,
    pub device: Arc<RawDevice>,
}

impl VkSwapchain {
    pub fn format(&self) -> vk::Format {
        self.format.format
    }
    // pub fn fill_framebuffers(
    //     &mut self,
    //     device: &RawDevice,
    //     render_pass: &vk::RenderPass,
    // ) -> VkResult<()> {
    //     for iv in &self.image_views {
    //         let iview = [*iv];
    //         let framebuffer_info = vk::FramebufferCreateInfo::builder()
    //             .render_pass(*render_pass)
    //             .attachments(&iview)
    //             .width(self.extent.width)
    //             .height(self.extent.height)
    //             .layers(1);
    //         let fb = unsafe { device.create_framebuffer(&framebuffer_info, None) }?;
    //         self.framebuffers.push(fb);
    //     }
    //     Ok(())
    // }

    // pub fn recreate_swapchain(
    //     mut self,
    //     width: u32,
    //     height: u32,
    //     insstance: &VkInstance,
    //     device: &VkDevice,
    //     queue: &VkQueues,
    //     surface: &VkSurface,
    // ) -> VkResult<()> {
    //     self.info.image_extent = vk::Extent2D { width, height };

    //     self = insstance.create_swapchain(device, surface, queue).unwrap();
    //     self.swapchain = unsafe { self.swapchain_loader.create_swapchain(&self.info, None) }?;

    //     self.images = unsafe { self.swapchain_loader.get_swapchain_images(self.swapchain)? };

    //     Ok(())
    // }
}

impl Drop for VkSwapchain {
    fn drop(&mut self) {
        unsafe {
            for &image_view in self.image_views.iter() {
                self.device.destroy_image_view(image_view, None);
            }
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None)
        };
    }
}
