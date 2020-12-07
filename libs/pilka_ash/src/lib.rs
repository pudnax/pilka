use std::{
    borrow::Cow,
    ffi::{CStr, CString},
};

pub mod ash_window {
    pub use ash_window::*;
}
pub mod ash {
    pub use ash::*;

    pub use crate::ash::{
        extensions::{
            ext::DebugUtils,
            khr::{Surface, Swapchain},
        },
        prelude::VkResult,
        version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
        vk,
    };

    use raw_window_handle::HasRawWindowHandle;

    use std::{
        borrow::Cow,
        ffi::{CStr, CString},
        sync::Arc,
    };

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
            app_name: String,
            engine_name: String,
            layers: &[&CStr],
            extensions: &[&CStr],
        ) -> Self {
            let entry = ash::Entry::new().unwrap();

            // Enumerate available vulkan API version and set 1.0.0 otherwise.
            let version = match entry.try_enumerate_instance_version().unwrap() {
                Some(version) => version,
                None => vk::make_version(2, 0, 0),
            };

            // Find approciate validation layers from available.
            let available_layers = entry.enumerate_instance_layer_properties().unwrap();
            let validation_layers = layers
                .iter()
                .map(|s| unsafe { CStr::from_ptr(s.as_ptr() as *const i8) })
                .filter_map(|lyr| {
                    available_layers
                        .iter()
                        .find(|x| unsafe { CStr::from_ptr(x.layer_name.as_ptr()) } == lyr)
                        .map(|_| lyr.as_ptr())
                        .or_else(|| {
                            println!(
                            "Unable to find layer: {}, have you installed the Vulkan SDK.unwrap()",
                            lyr.to_string_lossy()
                        );
                            None
                        })
                })
                .collect::<Vec<_>>();

            // Find approciate extensions from available.
            let available_exts = entry.enumerate_instance_extension_properties().unwrap();
            let extensions = extensions
            .iter()
            .map(|s| unsafe { CStr::from_ptr(s.as_ptr() as *const i8) })
            .filter_map(|ext| {
                available_exts
                    .iter()
                    .find(|x| unsafe { CStr::from_ptr(x.extension_name.as_ptr()) } == ext)
                    .map(|_| ext.as_ptr())
                    .or_else(|| {
                        println!(
                            "Unable to find extension: {}, have you installed the Vulkan SDK.unwrap()",
                            ext.to_string_lossy()
                        );
                        None
                    })
            })
            .collect::<Vec<_>>();

            let app_info = vk::ApplicationInfo::builder()
                .application_name(unsafe { CStr::from_ptr(app_name.as_ptr() as *const i8) })
                .engine_name(unsafe { CStr::from_ptr(engine_name.as_ptr() as *const i8) })
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
                let dbg_callbk = unsafe {
                    dbg_loader
                        .create_debug_utils_messenger(&dbg_info, None)
                        .unwrap()
                };
                (Some(dbg_loader), Some(dbg_callbk))
            } else {
                (None, None)
            };

            Self {
                entry,
                instance,
                validation_layers,
                _dbg_loader,
                _dbg_callbk,
            }
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

    pub struct VkSurface {
        pub surface: vk::SurfaceKHR,
        pub surface_loader: Surface,
    }

    impl Drop for VkSurface {
        fn drop(&mut self) {
            unsafe { self.surface_loader.destroy_surface(self.surface, None) };
        }
    }

    // TODO: Consider about Arc
    pub struct VkDevice {
        device: Arc<RawDevice>,
        physical_device: vk::PhysicalDevice,
    }

    struct RawDevice {
        device: Device,
    }

    pub struct VkDeviceProperties {
        memory: vk::PhysicalDeviceMemoryProperties,
        features: vk::PhysicalDeviceFeatures,
        properties: vk::PhysicalDeviceProperties,
    }

    impl VkDevice {
        pub fn new(
            instance: &VkInstance,
            queue_infos: &[vk::DeviceQueueCreateInfo],
        ) -> VkResult<Self> {
            // Acuire all availble device for this machine.
            let phys_devices = unsafe { instance.instance.enumerate_physical_devices() }?;

            // Choose physical device assuming that we want to choose discrete GPU.
            let (phys_device, _device_properties, device_features) = {
                let mut chosen = Err(vk::Result::ERROR_INITIALIZATION_FAILED);
                for p in phys_devices {
                    let properties = unsafe { instance.instance.get_physical_device_properties(p) };
                    let features = unsafe { instance.instance.get_physical_device_features(p) };
                    if properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
                        chosen = Ok((p, properties, features));
                    }
                }
                chosen
            }?;
            let device_extension_name_pointers: Vec<*const i8> = vec![Swapchain::name().as_ptr()];

            let device_info = vk::DeviceCreateInfo::builder()
                .enabled_layer_names(&instance.validation_layers)
                .enabled_extension_names(&device_extension_name_pointers)
                .enabled_features(&device_features)
                .queue_create_infos(&queue_infos);
            let device = unsafe {
                instance
                    .instance
                    .create_device(phys_device, &device_info, None)
            }?;
            let device = Arc::new(RawDevice { device });
            Ok(Self {
                device,
                physical_device: phys_device,
            })
        }

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
    }

    impl Drop for RawDevice {
        fn drop(&mut self) {
            unsafe { self.device.destroy_device(None) };
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
}

unsafe fn extract_entries(sample: Vec<String>, entries: Vec<String>) -> Vec<*const i8> {
    entries
        .iter()
        .map(|s| unsafe { CStr::from_ptr(s.as_ptr() as *const i8) })
        .filter_map(|lyr| {
            sample
                .iter()
                .find(|x| unsafe { CStr::from_ptr(x.as_ptr() as *const i8) } == lyr)
                .map(|_| lyr.as_ptr())
                .or_else(|| {
                    println!(
                        "Unable to find layer: {}, have you installed the Vulkan SDK.unwrap()",
                        lyr.to_string_lossy()
                    );
                    None
                })
        })
        .collect::<Vec<_>>()
}
