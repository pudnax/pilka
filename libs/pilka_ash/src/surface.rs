use crate::device::VkDevice;
use ash::{extensions::khr::Surface, vk};

pub struct VkSurface {
    pub surface: vk::SurfaceKHR,
    pub surface_loader: Surface,
}

impl VkSurface {
    pub fn get_capabilities(
        &self,
        device: &VkDevice,
    ) -> Result<vk::SurfaceCapabilitiesKHR, vk::Result> {
        unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(device.physical_device, self.surface)
        }
    }
    pub fn get_present_modes(
        &self,
        device: &VkDevice,
    ) -> Result<Vec<vk::PresentModeKHR>, vk::Result> {
        unsafe {
            self.surface_loader
                .get_physical_device_surface_present_modes(device.physical_device, self.surface)
        }
    }
    pub fn get_formats(&self, device: &VkDevice) -> Result<Vec<vk::SurfaceFormatKHR>, vk::Result> {
        unsafe {
            self.surface_loader
                .get_physical_device_surface_formats(device.physical_device, self.surface)
        }
    }
    pub fn get_physical_device_surface_support(
        &self,
        device: &VkDevice,
        queuefamilyindex: usize,
    ) -> Result<bool, vk::Result> {
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
