use crate::ash::{
    extensions::{
        ext::DebugUtils,
        khr::{Surface, Swapchain},
    },
    prelude::VkResult,
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    vk,
};

use raw_window_handle::HasRawWindowHandle;

use std::{borrow::Cow, ffi::CStr, lazy::SyncLazy, ops::Deref, sync::Arc};

use crate::{
    device::{RawDevice, VkDevice, VkDeviceProperties},
    surface::VkSurface,
    swapchain::VkSwapchain,
};

/// Static and lazy initialized array of needed validation layers.
/// Appear only on debug builds.
static LAYERS: SyncLazy<Vec<&'static CStr>> = SyncLazy::new(|| {
    let mut layers: Vec<&'static CStr> = vec![];
    if cfg!(debug_assertions) {
        layers.push(CStr::from_bytes_with_nul(b"VK_LAYER_KHRONOS_validation\0").unwrap());
    }
    layers
});

/// Static and lazy initialized array of needed extensions.
/// Appear only on debug builds.
static EXTS: SyncLazy<Vec<&'static CStr>> = SyncLazy::new(|| {
    let mut exts: Vec<&'static CStr> = vec![];
    if cfg!(debug_assertions) {
        exts.push(DebugUtils::name());
    }
    exts
});

#[allow(unused_macros)]
macro_rules! offset_of {
    ($base:path, $field:ident) => {{
        #[allow(unused_unsafe)]
        unsafe {
            let b: $base = std::mem::zeroed();
            (&b.$field as *const _ as isize) - (&b as *const _ as isize)
        }
    }};
}

/// The entry point for vulkan application.
pub struct VkInstance {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    validation_layers: Vec<*const i8>,
    _dbg_loader: Option<ash::extensions::ext::DebugUtils>,
    _dbg_callbk: Option<vk::DebugUtilsMessengerEXT>,
}

impl VkInstance {
    pub fn new(
        window_handle: Option<&dyn raw_window_handle::HasRawWindowHandle>,
    ) -> VkResult<Self> {
        let entry = ash::Entry::new().unwrap();

        // Enumerate available vulkan API version and set 1.0.0 otherwise.
        let version = match entry.try_enumerate_instance_version()? {
            Some(version) => version,
            None => vk::make_version(2, 0, 0),
        };

        // Find approciate validation layers from available.
        let available_layers = entry.enumerate_instance_layer_properties()?;
        let validation_layers = LAYERS
            .iter()
            .map(|s| unsafe { CStr::from_ptr(s.as_ptr() as *const i8) })
            .filter_map(|lyr| {
                available_layers
                    .iter()
                    .find(|x| unsafe { CStr::from_ptr(x.layer_name.as_ptr()) } == lyr)
                    .map(|_| lyr.as_ptr())
                    .or_else(|| {
                        println!(
                            "Unable to find layer: {}, have you installed the Vulkan SDK?",
                            lyr.to_string_lossy()
                        );
                        None
                    })
            })
            .collect::<Vec<_>>();

        let surface_extensions = match window_handle {
            Some(ref handle) => ash_window::enumerate_required_extensions(*handle)?,
            None => vec![],
        };
        // Find approciate extensions from available.
        let available_exts = entry.enumerate_instance_extension_properties()?;
        let extensions = EXTS
            .iter()
            .map(|s| unsafe { CStr::from_ptr(s.as_ptr() as *const i8) })
            .chain(surface_extensions)
            .filter_map(|ext| {
                available_exts
                    .iter()
                    .find(|x| unsafe { CStr::from_ptr(x.extension_name.as_ptr()) } == ext)
                    .map(|_| ext.as_ptr())
                    .or_else(|| {
                        println!(
                            "Unable to find extension: {}, have you installed the Vulkan SDK?",
                            ext.to_string_lossy()
                        );
                        None
                    })
            })
            .collect::<Vec<_>>();

        let app_info = vk::ApplicationInfo::builder()
            .application_name(unsafe { CStr::from_ptr("Pilka".as_ptr() as *const i8) })
            .engine_name(unsafe { CStr::from_ptr("Pilka Engine".as_ptr() as *const i8) })
            .engine_version(vk::make_version(1, 1, 0))
            .api_version(version);

        let instance_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_layer_names(&validation_layers)
            .enabled_extension_names(&extensions);

        let instance = unsafe { entry.create_instance(&instance_info, None) }.unwrap();

        let (_dbg_loader, _dbg_callbk) = if cfg!(debug_assertions) {
            let dbg_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                        | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING, // | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
                )
                .message_type(vk::DebugUtilsMessageTypeFlagsEXT::all())
                .pfn_user_callback(Some(vulkan_debug_callback));
            let dbg_loader = DebugUtils::new(&entry, &instance);
            let dbg_callbk = unsafe { dbg_loader.create_debug_utils_messenger(&dbg_info, None)? };
            (Some(dbg_loader), Some(dbg_callbk))
        } else {
            (None, None)
        };

        Ok(Self {
            entry,
            instance,
            validation_layers,
            _dbg_loader,
            _dbg_callbk,
        })
    }

    /// Make surface and surface loader.
    pub fn create_surface<W: HasRawWindowHandle>(&self, window: &W) -> VkResult<VkSurface> {
        let surface =
            unsafe { ash_window::create_surface(&self.entry, &self.instance, window, None) }?;
        let surface_loader = Surface::new(&self.entry, &self.instance);

        Ok(VkSurface {
            surface,
            surface_loader,
        })
    }

    pub fn create_device_and_queues(
        &self,
        surface: Option<&VkSurface>,
    ) -> VkResult<(VkDevice, VkDeviceProperties, VkQueues)> {
        // Acuire all availble device for this machine.
        let physical_devices = unsafe { self.enumerate_physical_devices() }?;

        // Choose physical device assuming that we want to choose discrete GPU.
        let (physical_device, device_properties, device_features) = {
            let mut chosen = Err(vk::Result::ERROR_INITIALIZATION_FAILED);
            for p in physical_devices {
                let properties = unsafe { self.get_physical_device_properties(p) };
                let features = unsafe { self.get_physical_device_features(p) };
                if properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
                    chosen = Ok((p, properties, features));
                }
            }
            chosen
        }?;
        let device_extension_name_pointers = match surface {
            Some(_) => vec![Swapchain::name().as_ptr()],
            None => vec![],
        };
        let memory = unsafe { self.get_physical_device_memory_properties(physical_device) };

        let queue_families = self.create_queue_families(physical_device, surface)?;

        let graphics_queue_index = queue_families.graphics_q_index.unwrap();
        let transfer_queue_index = queue_families.transfer_q_index.unwrap();
        let compute_queue_index = queue_families.compute_q_index.unwrap();

        let priorities = [1.0f32];

        // TODO: Don't allocate for such a thing
        let mut queue_infos = vec![
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(graphics_queue_index)
                .queue_priorities(&priorities)
                .build(),
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(transfer_queue_index)
                .queue_priorities(&priorities)
                .build(),
        ];
        if compute_queue_index != graphics_queue_index {
            queue_infos.push(
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(compute_queue_index)
                    .queue_priorities(&priorities)
                    .build(),
            );
        }

        let device_info = vk::DeviceCreateInfo::builder()
            .enabled_layer_names(&self.validation_layers)
            .enabled_extension_names(&device_extension_name_pointers)
            .enabled_features(&device_features)
            .queue_create_infos(&queue_infos);

        let device = unsafe { self.create_device(physical_device, &device_info, None) }?;
        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_index, 0) };
        let transfer_queue = unsafe { device.get_device_queue(transfer_queue_index, 0) };
        let compute_queue = unsafe { device.get_device_queue(compute_queue_index, 0) };

        let device = Arc::new(RawDevice { device });

        Ok((
            VkDevice {
                device,
                physical_device,
            },
            VkDeviceProperties {
                memory,
                properties: device_properties,
                features: device_features,
            },
            VkQueues {
                graphics_queue: (graphics_queue, graphics_queue_index),
                transfer_queue: (transfer_queue, transfer_queue_index),
                compute_queue: (compute_queue, compute_queue_index),
            },
        ))
    }

    fn create_queue_families(
        &self,
        physical_device: vk::PhysicalDevice,
        surface: Option<&VkSurface>,
    ) -> Result<QueueFamilies, vk::Result> {
        // Choose graphics and transfer queue families.
        let queuefamilyproperties =
            unsafe { self.get_physical_device_queue_family_properties(physical_device) };
        let mut found_graphics_q_index = None;
        let mut found_transfer_q_index = None;
        let mut found_compute_q_index = None;
        for (index, qfam) in queuefamilyproperties.iter().enumerate() {
            if qfam.queue_count > 0
                && qfam.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                && if let Some(surface) = surface {
                    unsafe {
                        surface.surface_loader.get_physical_device_surface_support(
                            physical_device,
                            index as u32,
                            surface.surface,
                        )
                    }?
                } else {
                    true
                }
            {
                found_graphics_q_index = Some(index as u32);
            }

            if qfam.queue_count > 0
                && qfam.queue_flags.contains(vk::QueueFlags::TRANSFER)
                && (found_transfer_q_index.is_none()
                    || !qfam.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            {
                found_transfer_q_index = Some(index as u32);
            }

            // TODO(#8): Make search for compute queue smarter.
            if qfam.queue_count > 0 && qfam.queue_flags.contains(vk::QueueFlags::COMPUTE) {
                let index = Some(index as u32);
                match (found_compute_q_index, qfam.queue_flags) {
                    (_, vk::QueueFlags::COMPUTE) => found_compute_q_index = index,
                    (None, _) => found_compute_q_index = index,
                    _ => {}
                }
            }
        }

        Ok(QueueFamilies {
            graphics_q_index: found_graphics_q_index,
            transfer_q_index: found_transfer_q_index,
            compute_q_index: found_compute_q_index,
        })
    }

    pub fn create_swapchain(
        &self,
        device: &VkDevice,
        surface: &VkSurface,
        queues: &VkQueues,
    ) -> VkResult<VkSwapchain> {
        let surface_capabilities = unsafe {
            surface
                .surface_loader
                .get_physical_device_surface_capabilities(device.physical_device, surface.surface)
        }?;

        let present_modes = unsafe {
            surface
                .surface_loader
                .get_physical_device_surface_present_modes(device.physical_device, surface.surface)
        }?;

        let format = unsafe {
            surface
                .surface_loader
                .get_physical_device_surface_formats(device.physical_device, surface.surface)
        }?[0];
        let surface_format = format.format;

        let graphics_queue_familty_index = [queues.graphics_queue.1];
        // We've choosed `COLOR_ATTACHMENT` for the same reason like with queue family.
        let swapchain_usage =
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
        let extent = surface_capabilities.current_extent;
        let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(surface.surface)
            .image_format(surface_format)
            .image_usage(swapchain_usage)
            .image_extent(extent)
            .image_color_space(format.color_space)
            .min_image_count(
                3.max(surface_capabilities.min_image_count)
                    .min(surface_capabilities.max_image_count),
            )
            .image_array_layers(surface_capabilities.max_image_array_layers)
            .queue_family_indices(&graphics_queue_familty_index)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(surface_capabilities.current_transform)
            .composite_alpha(surface_capabilities.supported_composite_alpha)
            .present_mode(present_modes[0])
            .clipped(true);

        let swapchain_loader = Swapchain::new(&self.instance, device.deref());

        let swapchain = unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None)? };

        let present_images = unsafe { swapchain_loader.get_swapchain_images(swapchain)? };
        let present_image_views = {
            present_images
                .iter()
                .map(|&image| {
                    let create_view_info = vk::ImageViewCreateInfo::builder()
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(surface_format)
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
        }?;

        Ok(VkSwapchain {
            swapchain,
            swapchain_loader,
            framebuffers: Vec::with_capacity(3),
            device: device.device.clone(),
            format: surface_format,
            images: present_images,
            image_views: present_image_views,
            extent,
            info: *swapchain_create_info,
        })
    }
}

impl std::ops::Deref for VkInstance {
    type Target = ash::Instance;

    fn deref(&self) -> &Self::Target {
        &self.instance
    }
}

impl std::ops::DerefMut for VkInstance {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.instance
    }
}

impl Drop for VkInstance {
    fn drop(&mut self) {
        if let Some(ref _dbg_loader) = self._dbg_loader {
            if let Some(_dbg_callbk) = self._dbg_callbk {
                unsafe { _dbg_loader.destroy_debug_utils_messenger(_dbg_callbk, None) };
            }
        }
        unsafe { self.instance.destroy_instance(None) };
    }
}

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = &*p_callback_data;
    let message_id_number: i32 = callback_data.message_id_number as i32;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    println!(
        "{:?}:\n{:?} [{} ({})] : {}\n",
        message_severity, message_type, message_id_name, message_id_number, message,
    );

    vk::FALSE
}
// Reasonable?
pub enum QueueType {
    Graphics(vk::Queue),
    Compute(vk::Queue),
    Transfer(vk::Queue),
}

pub struct VkQueues {
    pub graphics_queue: (vk::Queue, u32),
    pub transfer_queue: (vk::Queue, u32),
    pub compute_queue: (vk::Queue, u32),
}

#[derive(Copy, Clone)]
pub struct QueueFamilies {
    pub graphics_q_index: Option<u32>,
    pub transfer_q_index: Option<u32>,
    pub compute_q_index: Option<u32>,
}
