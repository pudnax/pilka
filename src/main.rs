#![feature(once_cell)]
use ash::{
    extensions::{
        ext::DebugUtils,
        khr::{Surface, Swapchain},
    },
    prelude::VkResult,
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    vk,
};
use eyre::*;

// TODO: Make final decision about dynamic linking and it performance.
#[cfg(feature = "dynamic")]
use pilka_dyn::*;

#[cfg(not(feature = "dynamic"))]
use pilka_incremental::*;

use std::{borrow::Cow, ffi::CStr, lazy::SyncLazy};

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

fn main() -> Result<()> {
    // Initialize error hook.
    color_eyre::install()?;

    let engine_name = CStr::from_bytes_with_nul(b"Ruchka Engine\0")?;
    let app_name = CStr::from_bytes_with_nul(b"Pilka\0")?;

    let event_loop = winit::event_loop::EventLoop::new();
    let window = winit::window::Window::new(&event_loop)?;
    window.set_title(&app_name.to_string_lossy());
    let surface_extensions = ash_window::enumerate_required_extensions(&window)?;

    let entry = ash::Entry::new()?;

    // Enumerate available vulkan API version and set 1.0.0 otherwise.
    let version = match entry.try_enumerate_instance_version()? {
        Some(version) => version,
        None => vk::make_version(1, 0, 0),
    };

    // Find approciate validation layers from available.
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

    // Find approciate extensions from available.
    let exist_exts = entry.enumerate_instance_extension_properties()?;
    SyncLazy::force(&EXTS);
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
        .chain(surface_extensions.iter().map(|s| s.as_ptr()))
        .collect::<Vec<_>>();

    let app_info = vk::ApplicationInfo::builder()
        .api_version(version)
        .engine_name(engine_name)
        .engine_version(vk::make_version(0, 1, 0))
        .application_name(app_name);

    let instance_info = vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_layer_names(&validation_layers)
        .enabled_extension_names(&extensions);

    let instance = unsafe { entry.create_instance(&instance_info, None) }?;

    let (_dbg_loader, _dbg_callbk) = if cfg!(debug_assertions) {
        let dbg_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
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

    // Make surface and surface loader.
    let surface = unsafe { ash_window::create_surface(&entry, &instance, &window, None) }?;
    let surface_loader = Surface::new(&entry, &instance);

    // Acuire all availble device for this machine.
    let phys_devices = unsafe { instance.enumerate_physical_devices() }?;

    // Choose physical device assuming that we want to choose discrete GPU.
    let (physical_device, _device_properties, device_features) = {
        let mut chosen = Err(vk::Result::ERROR_INITIALIZATION_FAILED);
        for p in phys_devices {
            let properties = unsafe { instance.get_physical_device_properties(p) };
            let features = unsafe { instance.get_physical_device_features(p) };
            if properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
                chosen = Ok((p, properties, features));
            }
        }
        chosen
    }?;

    // Choose graphics and transfer queue familities.
    let queuefamilyproperties =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    let mut found_graphics_q_index = None;
    let mut found_transfer_q_index = None;
    for (index, qfam) in queuefamilyproperties.iter().enumerate() {
        if qfam.queue_count > 0 && qfam.queue_flags.contains(vk::QueueFlags::GRAPHICS) && {
            unsafe {
                surface_loader.get_physical_device_surface_support(
                    physical_device,
                    index as u32,
                    surface,
                )
            }?
        } {
            found_graphics_q_index = Some(index as u32);
        }
        if qfam.queue_count > 0
            && qfam.queue_flags.contains(vk::QueueFlags::TRANSFER)
            && (found_transfer_q_index.is_none()
                || !qfam.queue_flags.contains(vk::QueueFlags::GRAPHICS))
        {
            found_transfer_q_index = Some(index as u32);
        }
    }

    let priorities = [1.0f32];
    let queue_infos = [
        vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(found_graphics_q_index.unwrap())
            .queue_priorities(&priorities)
            .build(),
        vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(found_transfer_q_index.unwrap())
            .queue_priorities(&priorities)
            .build(),
    ];

    let device_extension_name_pointers: Vec<*const i8> = vec![Swapchain::name().as_ptr()];

    let device_info = vk::DeviceCreateInfo::builder()
        .enabled_layer_names(&validation_layers)
        .enabled_extension_names(&device_extension_name_pointers)
        .enabled_features(&device_features)
        .queue_create_infos(&queue_infos);
    let device = unsafe { instance.create_device(physical_device, &device_info, None) }?;

    let surface_capabilities = unsafe {
        surface_loader.get_physical_device_surface_capabilities(physical_device, surface)
    }?;

    let present_modes = unsafe {
        surface_loader.get_physical_device_surface_present_modes(physical_device, surface)
    }?;

    // TODO: Choose reasonable format or seive out UNDEFINED.
    let formats =
        unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface) }?[0];
    let surface_format = formats.format;

    // This swapchain of 'images' used for sending picture into the screen,
    // so we're choosing graphics queue family.
    let graphics_queue_familty_index = [found_graphics_q_index.unwrap()];
    // We've choosed `COLOR_ATTACHMENT` for the same reason like with queue famility.
    let swapchain_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
    let extent = surface_capabilities.current_extent;
    let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
        .surface(surface)
        .image_format(surface_format)
        .image_usage(swapchain_usage)
        .image_extent(extent)
        .image_color_space(formats.color_space)
        .min_image_count(
            3.max(surface_capabilities.min_image_count)
                .min(surface_capabilities.max_image_count),
        )
        .image_array_layers(surface_capabilities.max_image_array_layers)
        .queue_family_indices(&graphics_queue_familty_index)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .composite_alpha(surface_capabilities.supported_composite_alpha)
        .present_mode(present_modes[0])
        .clipped(true)
        .pre_transform(surface_capabilities.current_transform);

    let swapchain_loader = Swapchain::new(&instance, &device);
    let swapchain = unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None)? };

    Ok(())
}
