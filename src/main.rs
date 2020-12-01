#![feature(once_cell)]
use ash::{
    extensions::ext::DebugUtils,
    version::{
        DeviceV1_0, EntryV1_0, EntryV1_1, EntryV1_2, InstanceV1_0, InstanceV1_1, InstanceV1_2,
    },
    vk, Instance,
};
use eyre::*;
use std::ffi::CStr;

/// Static and lazy initialized array of needed validation layers.
/// Appear only on debug builds.
static LAYERS: std::lazy::SyncLazy<[&CStr; 1]> = std::lazy::SyncLazy::new(|| {
    [if cfg!(debug_assertions) {
        CStr::from_bytes_with_nul(b"VK_LAYER_KHRONOS_validation\0").unwrap()
    } else {
        CStr::from_bytes_with_nul(b"\0").unwrap()
    }]
});

/// Static and lazy initialized array of needed extensions.
/// Appear only on debug builds.
static EXTS: std::lazy::SyncLazy<[&CStr; 1]> = std::lazy::SyncLazy::new(|| {
    [if cfg!(debug_assertions) {
        DebugUtils::name()
    } else {
        CStr::from_bytes_with_nul(b"\0").unwrap()
    }]
});

fn main() -> Result<()> {
    // Initialize error hook.
    color_eyre::install()?;

    let entry = ash::Entry::new()?;

    // Enumerate available vulkan API version and set 1.0.0 otherwise.
    let version = match entry.try_enumerate_instance_version()? {
        Some(version) => version,
        None => vk::make_version(1, 0, 0),
    };

    let available_layers = entry.enumerate_instance_layer_properties()?;
    let validation_layers = LAYERS
        .iter()
        .filter_map(|&lyr| {
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

    let exist_exts = entry.enumerate_instance_extension_properties()?;
    let extensions = EXTS
        .iter()
        .filter_map(|&ext| {
            exist_exts
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

    let engine_name = CStr::from_bytes_with_nul(b"Ruchka Engine\0")?;
    let app_name = CStr::from_bytes_with_nul(b"Pilka\0")?;
    let app_info = vk::ApplicationInfo::builder()
        .api_version(version)
        .engine_name(engine_name)
        .engine_version(vk::make_version(0, 1, 0))
        .application_name(app_name);

    let instance_info = vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_layer_names(&validation_layers)
        .enabled_extension_names(&extensions);

    let instance = unsafe { entry.create_instance(&instance_info, None)? };

    Ok(())
}
