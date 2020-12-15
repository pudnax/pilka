use crate::{
    device::{RawDevice, VkDevice},
    renderpass_and_pipeline::VkRenderPass,
};
use ash::{extensions::khr::Swapchain, prelude::VkResult, version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct VkSwapchain {
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_loader: Swapchain,
    pub images: Vec<vk::Image>,
    pub image_views: Vec<vk::ImageView>,
    pub format: vk::Format,
    pub info: vk::SwapchainCreateInfoKHR,
    pub device: Arc<RawDevice>,
}

impl VkSwapchain {
    pub fn format(&self) -> vk::Format {
        self.format
    }

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

    pub fn create_image_views(
        images: &[vk::Image],
        format: vk::Format,
        device: &VkDevice,
    ) -> VkResult<Vec<vk::ImageView>> {
        images
            .iter()
            .map(|&image| {
                let create_view_info = vk::ImageViewCreateInfo::builder()
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .components(vk::ComponentMapping {
                        // Why not BGRA?
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image(image);
                unsafe { device.create_image_view(&create_view_info, None) }
            })
            .collect::<VkResult<Vec<_>>>()
    }

    pub fn create_framebuffers(
        &self,
        (width, height): (u32, u32),
        render_pass: &VkRenderPass,
        device: &VkDevice,
    ) -> VkResult<Vec<vk::Framebuffer>> {
        self.image_views
            .iter()
            .map(|&present_image_view| {
                Self::create_framebuffer(
                    &[present_image_view],
                    (width, height),
                    render_pass,
                    device,
                )
            })
            .collect()
    }

    pub fn create_framebuffer(
        image_views: &[vk::ImageView],
        (width, height): (u32, u32),
        render_pass: &VkRenderPass,
        device: &VkDevice,
    ) -> VkResult<vk::Framebuffer> {
        let framebuffer_attachments = image_views;
        unsafe {
            device.create_framebuffer(
                &vk::FramebufferCreateInfo::builder()
                    .render_pass(render_pass.render_pass)
                    .attachments(&framebuffer_attachments)
                    .width(width)
                    .height(height)
                    .layers(1),
                None,
            )
        }
    }
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
