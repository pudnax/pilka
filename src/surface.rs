use std::ops::Deref;

use anyhow::Result;
use ash::{khr, vk};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use crate::device::Device;

pub struct Surface {
    loader: khr::surface::Instance,
    inner: vk::SurfaceKHR,
}

impl Deref for Surface {
    type Target = vk::SurfaceKHR;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Debug, Clone)]
pub struct SurfaceInfo {
    pub capabilities: vk::SurfaceCapabilitiesKHR,
    pub formats: Vec<vk::SurfaceFormatKHR>,
    pub present_modes: Vec<vk::PresentModeKHR>,
}

impl Surface {
    pub fn new(
        entry: &ash::Entry,
        instance: &ash::Instance,
        handle: &(impl HasDisplayHandle + HasWindowHandle),
    ) -> Result<Self> {
        let inner = unsafe {
            ash_window::create_surface(
                entry,
                instance,
                handle.display_handle()?.as_raw(),
                handle.window_handle()?.as_raw(),
                None,
            )?
        };

        let loader = khr::surface::Instance::new(entry, instance);

        Ok(Surface { inner, loader })
    }

    pub fn get_device_capabilities(&self, device: &Device) -> vk::SurfaceCapabilitiesKHR {
        unsafe {
            self.loader
                .get_physical_device_surface_capabilities(device.physical_device, self.inner)
                .unwrap()
        }
    }

    pub fn get_device_surface_support(
        &self,
        physical_device: vk::PhysicalDevice,
        queue_family_index: u32,
    ) -> bool {
        unsafe {
            self.loader
                .get_physical_device_surface_support(
                    physical_device,
                    queue_family_index,
                    self.inner,
                )
                .unwrap()
        }
    }

    pub fn info(&self, device: &Device) -> SurfaceInfo {
        let physical_device = device.physical_device;
        let formats = unsafe {
            self.loader
                .get_physical_device_surface_formats(physical_device, self.inner)
                .unwrap()
        };

        let capabilities = unsafe {
            self.loader
                .get_physical_device_surface_capabilities(physical_device, self.inner)
                .unwrap()
        };

        let present_modes = unsafe {
            self.loader
                .get_physical_device_surface_present_modes(physical_device, self.inner)
                .unwrap()
        };

        SurfaceInfo {
            capabilities,
            formats,
            present_modes,
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { self.loader.destroy_surface(self.inner, None) };
    }
}
