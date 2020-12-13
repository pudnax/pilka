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
        queuefamilyindex: usize,
    ) -> VkResult<bool> {
        unsafe {
            self.surface_loader.get_physical_device_surface_support(
                device.physical_device,
                queuefamilyindex as u32,
                self.surface,
            )
        }
    }
}

impl Drop for VkSurface {
    fn drop(&mut self) {
        unsafe { self.surface_loader.destroy_surface(self.surface, None) };
    }
}
