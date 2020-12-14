use crate::device::VkDevice;
use ash::{extensions::khr::Surface, prelude::VkResult, vk};

pub struct VkSurface {
    pub surface: vk::SurfaceKHR,
    pub surface_loader: Surface,
}

impl VkSurface {
    pub fn get_capabilities(&self, device: &VkDevice) -> VkResult<vk::SurfaceCapabilitiesKHR> {
        unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(device.physical_device, self.surface)
        }
    }

    pub fn get_present_modes(&self, device: &VkDevice) -> VkResult<Vec<vk::PresentModeKHR>> {
        unsafe {
            self.surface_loader
                .get_physical_device_surface_present_modes(device.physical_device, self.surface)
        }
    }

    pub fn get_formats(&self, device: &VkDevice) -> VkResult<Vec<vk::SurfaceFormatKHR>> {
        unsafe {
            self.surface_loader
                .get_physical_device_surface_formats(device.physical_device, self.surface)
        }
    }

    pub fn get_physical_device_surface_support(
        &self,
        device: &VkDevice,
        queue_family_index: usize,
    ) -> VkResult<bool> {
        unsafe {
            self.surface_loader.get_physical_device_surface_support(
                device.physical_device,
                queue_family_index as u32,
                self.surface,
            )
        }
    }

    pub fn resolution(&self, device: &VkDevice) -> VkResult<vk::Extent2D> {
        Ok(self.get_capabilities(device)?.current_extent)
        // match surface_capabilities.current_extent.width {
        //     std::u32::MAX => {
        //         let window_inner = self.window.inner_size();
        //         vk::Extent2D {
        //             width: window_inner.width,
        //             height: window_inner.height,
        //         }
        //     }
        //     _ => surface_capabilities.current_extent,
        // }
    }

    pub fn resolution_slice(&self, device: &VkDevice) -> VkResult<[f32; 2]> {
        let extent = self.get_capabilities(device)?.current_extent;
        Ok([extent.width as f32, extent.height as f32])
        // match surface_capabilities.current_extent.width {
        //     std::u32::MAX => {
        //         let window_inner = self.window.inner_size();
        //         vk::Extent2D {
        //             width: window_inner.width,
        //             height: window_inner.height,
        //         }
        //     }
        //     _ => surface_capabilities.current_extent,
        // }
    }
}

impl Drop for VkSurface {
    fn drop(&mut self) {
        unsafe { self.surface_loader.destroy_surface(self.surface, None) };
    }
}
