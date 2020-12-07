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

    use std::{
        borrow::Cow,
        ffi::{CStr, CString},
    };

    /// The entry point for vulkan application.
    pub struct VkInstance {
        pub entry: ash::Entry,
        pub instance: ash::Instance,
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
                _dbg_loader,
                _dbg_callbk,
            }
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
