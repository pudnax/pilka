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
        version::{DeviceV1_0, DeviceV1_1, DeviceV1_2, EntryV1_0, InstanceV1_0},
        vk,
    };

    use raw_window_handle::HasRawWindowHandle;

    use std::{
        borrow::Cow,
        ffi::{CStr, CString},
        ops::Deref,
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
        ) -> VkResult<Self> {
            let entry = ash::Entry::new().unwrap();

            // Enumerate available vulkan API version and set 1.0.0 otherwise.
            let version = match entry.try_enumerate_instance_version()? {
                Some(version) => version,
                None => vk::make_version(2, 0, 0),
            };

            // Find approciate validation layers from available.
            let available_layers = entry.enumerate_instance_layer_properties()?;
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
                                "Unable to find layer: {}, have you installed the Vulkan SDK?",
                                lyr.to_string_lossy()
                            );
                            None
                        })
                })
                .collect::<Vec<_>>();

            // Find approciate extensions from available.
            let available_exts = entry.enumerate_instance_extension_properties()?;
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
                                "Unable to find extension: {}, have you installed the Vulkan SDK?",
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
                let dbg_callbk =
                    unsafe { dbg_loader.create_debug_utils_messenger(&dbg_info, None)? };
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
        pub fn get_formats(
            &self,
            device: &VkDevice,
        ) -> Result<Vec<vk::SurfaceFormatKHR>, vk::Result> {
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

    impl std::ops::Deref for VkDevice {
        type Target = ash::Device;

        fn deref(&self) -> &Self::Target {
            &self.device.device
        }
    }

    // Do not do this, you lack!
    //
    // impl std::ops::DerefMut for VkDevice {
    //     fn deref_mut(&mut self) -> &mut Self::Target {
    //         Arc::get_mut(&mut self.device)
    //     }
    // }

    impl VkDevice {
        pub fn init_device_and_queues(
            instance: &VkInstance,
            queue_infos: &[vk::DeviceQueueCreateInfo],
            queue_families: QueueFamilies,
        ) -> VkResult<(Self, VkDeviceProperties, VkQueues)> {
            // Acuire all availble device for this machine.
            let phys_devices = unsafe { instance.instance.enumerate_physical_devices() }?;

            // Choose physical device assuming that we want to choose discrete GPU.
            let (phys_device, device_properties, device_features) = {
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
            let memory = unsafe {
                instance
                    .instance
                    .get_physical_device_memory_properties(phys_device)
            };

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

            let graphics_queue =
                unsafe { device.get_device_queue(queue_families.graphics_q_index.unwrap(), 0) };
            let transfer_queue =
                unsafe { device.get_device_queue(queue_families.transfer_q_index.unwrap(), 0) };
            let compute_queue =
                unsafe { device.get_device_queue(queue_families.compute_q_index.unwrap(), 0) };

            let device = Arc::new(RawDevice { device });

            Ok((
                Self {
                    device,
                    physical_device: phys_device,
                },
                VkDeviceProperties {
                    memory,
                    properties: device_properties,
                    features: device_features,
                },
                VkQueues {
                    graphics_queue,
                    transfer_queue,
                    compute_queue,
                },
            ))
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

    pub struct VkSwapchain {
        swapchain: vk::SwapchainKHR,
        swapchain_loader: Swapchain,
    }

    impl VkSwapchain {
        pub fn new(
            instance: &VkInstance,
            device: &VkDevice,
            surface: &VkSurface,
            queue_families: QueueFamilies,
        ) -> VkResult<Self> {
            let surface_capabilities = unsafe {
                surface
                    .surface_loader
                    .get_physical_device_surface_capabilities(
                        device.physical_device,
                        surface.surface,
                    )
            }?;

            let present_modes = unsafe {
                surface
                    .surface_loader
                    .get_physical_device_surface_present_modes(
                        device.physical_device,
                        surface.surface,
                    )
            }?;

            // TODO: Choose reasonable format or seive out UNDEFINED.
            let formats = unsafe {
                surface
                    .surface_loader
                    .get_physical_device_surface_formats(device.physical_device, surface.surface)
            }?[0];
            let surface_format = formats.format;

            // This swapchain of 'images' used for sending picture into the screen,
            // so we're choosing graphics queue family.
            let graphics_queue_familty_index = [queue_families.graphics_q_index.unwrap()];
            let present_queue = unsafe {
                device
                    .deref()
                    .get_device_queue(graphics_queue_familty_index[0], 0)
            };
            // We've choosed `COLOR_ATTACHMENT` for the same reason like with queue famility.
            let swapchain_usage =
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
            let extent = surface_capabilities.current_extent;
            let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
                .surface(surface.surface)
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
                .pre_transform(surface_capabilities.current_transform)
                .composite_alpha(surface_capabilities.supported_composite_alpha)
                .present_mode(present_modes[0])
                .clipped(true);

            let swapchain_loader = Swapchain::new(&instance.instance, device.deref());
            let swapchain =
                unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None)? };
            Ok(Self {
                swapchain,
                swapchain_loader,
            })
        }
    }

    impl Drop for VkSwapchain {
        fn drop(&mut self) {
            unsafe {
                self.swapchain_loader
                    .destroy_swapchain(self.swapchain, None)
            };
        }
    }

    pub struct VkQueues {
        pub graphics_queue: vk::Queue,
        pub transfer_queue: vk::Queue,
        pub compute_queue: vk::Queue,
    }

    #[derive(Copy, Clone)]
    pub struct QueueFamilies {
        pub graphics_q_index: Option<u32>,
        pub transfer_q_index: Option<u32>,
        pub compute_q_index: Option<u32>,
    }

    impl QueueFamilies {
        pub fn init(
            instance: &ash::Instance,
            physical_device: vk::PhysicalDevice,
            surface: &VkSurface,
        ) -> Result<QueueFamilies, vk::Result> {
            // Choose graphics and transfer queue familities.
            let queuefamilyproperties =
                unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
            let mut found_graphics_q_index = None;
            let mut found_transfer_q_index = None;
            let mut found_compute_q_index = None;
            for (index, qfam) in queuefamilyproperties.iter().enumerate() {
                if qfam.queue_count > 0 && qfam.queue_flags.contains(vk::QueueFlags::GRAPHICS) && {
                    unsafe {
                        surface.surface_loader.get_physical_device_surface_support(
                            physical_device,
                            index as u32,
                            surface.surface,
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
                // TODO: Make search for compute queue smarter.
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
    }

    pub struct CommandBuffer {
        command_buffer: vk::CommandBuffer,
        fence: vk::Fence,
    }

    pub struct CommandBufferPool {
        pub pool: vk::CommandPool,
        pub command_buffers: Vec<CommandBuffer>,
        device: Arc<Device>,
    }

    impl CommandBufferPool {
        pub fn new(
            device: Arc<Device>,
            queue_family_index: u32,
            num_command_buffers: u32,
        ) -> VkResult<Self> {
            unsafe {
                let pool_create_info = vk::CommandPoolCreateInfo::builder()
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                    .queue_family_index(queue_family_index);

                let pool = device.create_command_pool(&pool_create_info, None)?;

                let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
                    .command_buffer_count(num_command_buffers)
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY);

                let command_buffers =
                    device.allocate_command_buffers(&command_buffer_allocate_info)?;

                let fence_info =
                    vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);

                let command_buffers: VkResult<Vec<CommandBuffer>> = command_buffers
                    .iter()
                    .map(|&command_buffer| {
                        let fence = device.create_fence(&fence_info, None)?;
                        Ok(CommandBuffer {
                            command_buffer,
                            fence,
                        })
                    })
                    .collect();
                let command_buffers = command_buffers?;

                Ok(CommandBufferPool {
                    pool,
                    command_buffers,
                    device,
                })
            }
        }

        unsafe fn create_fence(&self, signaled: bool) -> VkResult<vk::Fence> {
            let device = &self.device;
            let mut flags = vk::FenceCreateFlags::empty();
            if signaled {
                flags |= vk::FenceCreateFlags::SIGNALED;
            }
            Ok(device.create_fence(&vk::FenceCreateInfo::builder().flags(flags).build(), None)?)
        }

        unsafe fn create_semaphore(&self) -> VkResult<vk::Semaphore> {
            let device = &self.device;
            Ok(device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?)
        }
    }

    impl Drop for CommandBufferPool {
        fn drop(&mut self) {
            unsafe {
                for command_buffer in &self.command_buffers {
                    self.device.destroy_fence(command_buffer.fence, None);
                }

                self.device.destroy_command_pool(self.pool, None);
            }
        }
    }
}

fn extract_entries(sample: Vec<String>, entries: Vec<String>) -> Vec<*const i8> {
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
                        "Unable to find layer: {}, have you installed the Vulkan SDK?",
                        lyr.to_string_lossy()
                    );
                    None
                })
        })
        .collect::<Vec<_>>()
}
