#![allow(dead_code)]
#![feature(once_cell)]

pub mod ash_window {
    pub use ash_window::*;
}

pub mod ash {
    pub use ash::*;

    use crate::ash::{
        extensions::{
            ext::DebugUtils,
            khr::{Surface, Swapchain},
        },
        prelude::VkResult,
        version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    };

    use raw_window_handle::HasRawWindowHandle;

    use std::{
        borrow::Cow,
        ffi::{CStr, CString},
        lazy::SyncLazy,
        ops::Deref,
        sync::Arc,
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

            let swapchain =
                unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None)? };

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
            })
        }
    }

    pub struct VkImage {
        image: vk::Image,
        image_memory: vk::DeviceMemory,
        image_view: vk::ImageView,
        extent: vk::Extent2D,
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

    pub struct VkDevice {
        pub device: Arc<RawDevice>,
        physical_device: vk::PhysicalDevice,
    }

    pub struct RawDevice {
        device: Device,
    }

    impl std::ops::Deref for RawDevice {
        type Target = Device;

        fn deref(&self) -> &Self::Target {
            &self.device
        }
    }

    pub struct VkDeviceProperties {
        pub memory: vk::PhysicalDeviceMemoryProperties,
        pub features: vk::PhysicalDeviceFeatures,
        pub properties: vk::PhysicalDeviceProperties,
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

        pub fn create_commmand_buffer(
            &self,
            queue_family_index: u32,
            num_command_buffers: u32,
        ) -> VkResult<CommandBufferPool> {
            let pool_create_info = vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_family_index);

            let pool = unsafe { self.create_command_pool(&pool_create_info, None) }?;

            let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(num_command_buffers)
                .command_pool(pool)
                .level(vk::CommandBufferLevel::PRIMARY);

            let command_buffers =
                unsafe { self.allocate_command_buffers(&command_buffer_allocate_info) }?;

            let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);

            let command_buffers: VkResult<Vec<CommandBuffer>> = command_buffers
                .iter()
                .map(|&command_buffer| {
                    let fence = unsafe { self.create_fence(&fence_info, None) }?;
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
                device: self.device.clone(),
                active_command_buffer: 0,
            })
        }

        pub fn create_vk_render_pass(&self, swapchain: &mut VkSwapchain) -> VkResult<VkRenderPass> {
            let renderpass_attachments = [vk::AttachmentDescription::builder()
                .format(swapchain.format)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .build()];
            let color_attachment_refs = [vk::AttachmentReference::builder()
                .attachment(0)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .build()];

            let dependencies = [vk::SubpassDependency::builder()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_subpass(0)
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(
                    vk::AccessFlags::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                )
                .build()];

            let subpasses = [vk::SubpassDescription::builder()
                .color_attachments(&color_attachment_refs)
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .build()];

            // Depth textute? Never heard about it.
            let renderpass_create_info = vk::RenderPassCreateInfo::builder()
                .attachments(&renderpass_attachments)
                .subpasses(&subpasses)
                .dependencies(&dependencies);

            let renderpass = unsafe {
                self.device
                    .create_render_pass(&renderpass_create_info, None)
            }?;

            swapchain.fill_framebuffers(&self.device, &renderpass)?;

            Ok(VkRenderPass {
                render_pass: renderpass,
                device: self.device.clone(),
            })
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
        pub swapchain: vk::SwapchainKHR,
        pub swapchain_loader: Swapchain,
        pub framebuffers: Vec<vk::Framebuffer>,
        device: Arc<RawDevice>,
        pub format: vk::Format,
        images: Vec<vk::Image>,
        image_views: Vec<vk::ImageView>,
        pub extent: vk::Extent2D,
    }

    impl VkSwapchain {
        pub fn fill_framebuffers(
            &mut self,
            device: &RawDevice,
            render_pass: &vk::RenderPass,
        ) -> VkResult<()> {
            for iv in &self.image_views {
                let iview = [*iv];
                let framebuffer_info = vk::FramebufferCreateInfo::builder()
                    .render_pass(*render_pass)
                    .attachments(&iview)
                    .width(self.extent.width)
                    .height(self.extent.height)
                    .layers(1);
                let fb = unsafe { device.create_framebuffer(&framebuffer_info, None) }?;
                self.framebuffers.push(fb);
            }
            Ok(())
        }
    }

    impl Drop for VkSwapchain {
        fn drop(&mut self) {
            unsafe {
                for framebuffer in self.framebuffers.iter() {
                    self.device.device.destroy_framebuffer(*framebuffer, None);
                }
                for &image_view in self.image_views.iter() {
                    self.device.destroy_image_view(image_view, None);
                }
                self.swapchain_loader
                    .destroy_swapchain(self.swapchain, None)
            };
        }
    }

    pub struct VkRenderPass {
        pub render_pass: vk::RenderPass,
        device: Arc<RawDevice>,
    }

    impl std::ops::Deref for VkRenderPass {
        type Target = vk::RenderPass;

        fn deref(&self) -> &Self::Target {
            &self.render_pass
        }
    }

    impl std::ops::DerefMut for VkRenderPass {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.render_pass
        }
    }

    impl Drop for VkRenderPass {
        fn drop(&mut self) {
            unsafe { self.device.destroy_render_pass(self.render_pass, None) };
        }
    }

    pub struct VkPipeline {
        pub pipelines: Vec<vk::Pipeline>,
        pub graphics_pipeline: usize,
        pub pipeline_layout: vk::PipelineLayout,
        device: Arc<RawDevice>,
        pub viewports: [vk::Viewport; 1],
        pub scissors: [vk::Rect2D; 1],
    }

    impl VkPipeline {
        pub fn new(
            vertex_shader_module: vk::ShaderModule,
            fragment_shader_module: vk::ShaderModule,
            extent: vk::Extent2D,
            render_pass: &VkRenderPass,
            device: Arc<RawDevice>,
        ) -> VkResult<Self> {
            let layout_create_info = vk::PipelineLayoutCreateInfo::default();

            let pipeline_layout = unsafe {
                device
                    .device
                    .create_pipeline_layout(&layout_create_info, None)
            }?;

            let shader_entry_name = CString::new("main").unwrap();
            let shader_stage_create_infos = [
                vk::PipelineShaderStageCreateInfo {
                    module: vertex_shader_module,
                    p_name: shader_entry_name.as_ptr(),
                    stage: vk::ShaderStageFlags::VERTEX,
                    ..Default::default()
                },
                vk::PipelineShaderStageCreateInfo {
                    s_type: vk::StructureType::PIPELINE_SHADER_STAGE_CREATE_INFO,
                    module: fragment_shader_module,
                    p_name: shader_entry_name.as_ptr(),
                    stage: vk::ShaderStageFlags::FRAGMENT,
                    ..Default::default()
                },
            ];

            let vertex_input_state_info = vk::PipelineVertexInputStateCreateInfo {
                // vertex_attribute_description_count: vertex_input_attribute_descriptions.len()
                //     as u32,
                // p_vertex_attribute_descriptions: vertex_input_attribute_descriptions.as_ptr(),
                // vertex_binding_description_count: vertex_input_binding_descriptions.len() as u32,
                // p_vertex_binding_descriptions: vertex_input_binding_descriptions.as_ptr(),
                ..Default::default()
            };

            let vertex_input_assembly_state_info = vk::PipelineInputAssemblyStateCreateInfo {
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                ..Default::default()
            };
            let viewports = [vk::Viewport {
                x: 0.0,
                y: extent.height as f32,
                width: extent.width as f32,
                height: -(extent.height as f32),
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            let scissors = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            }];
            let viewport_state_info = vk::PipelineViewportStateCreateInfo::builder()
                .scissors(&scissors)
                .viewports(&viewports);

            let rasterization_info = vk::PipelineRasterizationStateCreateInfo {
                front_face: vk::FrontFace::COUNTER_CLOCKWISE,
                line_width: 1.0,
                polygon_mode: vk::PolygonMode::FILL,
                cull_mode: vk::CullModeFlags::BACK,
                ..Default::default()
            };
            let multisample_state_info = vk::PipelineMultisampleStateCreateInfo {
                rasterization_samples: vk::SampleCountFlags::TYPE_1,
                ..Default::default()
            };

            let color_blend_attachment_states = [vk::PipelineColorBlendAttachmentState {
                blend_enable: 0,
                src_color_blend_factor: vk::BlendFactor::SRC_COLOR,
                dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_DST_COLOR,
                color_blend_op: vk::BlendOp::ADD,
                src_alpha_blend_factor: vk::BlendFactor::ZERO,
                dst_alpha_blend_factor: vk::BlendFactor::ZERO,
                alpha_blend_op: vk::BlendOp::ADD,
                color_write_mask: vk::ColorComponentFlags::all(),
            }];
            let color_blend_state = vk::PipelineColorBlendStateCreateInfo::builder()
                .logic_op(vk::LogicOp::CLEAR)
                .attachments(&color_blend_attachment_states);

            let dynamic_state = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
            let dynamic_state_info =
                vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(&dynamic_state);

            let graphic_pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
                .stages(&shader_stage_create_infos)
                .vertex_input_state(&vertex_input_state_info)
                .input_assembly_state(&vertex_input_assembly_state_info)
                .viewport_state(&viewport_state_info)
                .rasterization_state(&rasterization_info)
                .multisample_state(&multisample_state_info)
                .color_blend_state(&color_blend_state)
                .dynamic_state(&dynamic_state_info)
                .layout(pipeline_layout)
                .render_pass(render_pass.render_pass);

            let graphics_pipelines = unsafe {
                device.device.create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    &[graphic_pipeline_info.build()],
                    None,
                )
            }
            .expect("Unable to create graphics pipeline");

            Ok(VkPipeline {
                pipelines: graphics_pipelines,
                graphics_pipeline: 0,
                pipeline_layout,
                device,
                viewports,
                scissors,
            })
        }
    }

    impl Drop for VkPipeline {
        fn drop(&mut self) {
            unsafe {
                for pipeline in &self.pipelines {
                    self.device.device.destroy_pipeline(*pipeline, None);
                }

                self.device
                    .device
                    .destroy_pipeline_layout(self.pipeline_layout, None);
            }
        }
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

    pub struct CommandBuffer {
        command_buffer: vk::CommandBuffer,
        fence: vk::Fence,
    }

    // TODO(#13): Call vkResetCommandPool before reusing it in another frame.
    //
    // Otherwise the pool will keep on growing until you run out of memory
    pub struct CommandBufferPool {
        pub pool: vk::CommandPool,
        pub command_buffers: Vec<CommandBuffer>,
        device: Arc<RawDevice>,
        active_command_buffer: usize,
    }

    impl CommandBufferPool {
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

        pub fn record_submit_commandbuffer<F: FnOnce(&VkDevice, vk::CommandBuffer)>(
            &mut self,
            device: &VkDevice,
            submit_queue: vk::Queue,
            wait_mask: &[vk::PipelineStageFlags],
            wait_semaphores: &[vk::Semaphore],
            signal_semaphores: &[vk::Semaphore],
            f: F,
        ) {
            let submit_fence = self.command_buffers[self.active_command_buffer].fence;
            let command_buffer = self.command_buffers[self.active_command_buffer].command_buffer;

            unsafe {
                device
                    .wait_for_fences(&[submit_fence], true, std::u64::MAX)
                    .expect("Wait for fences failed.")
            };
            unsafe {
                device
                    .reset_fences(&[submit_fence])
                    .expect("Reset fences failed.")
            };

            unsafe {
                device
                    .reset_command_buffer(
                        command_buffer,
                        vk::CommandBufferResetFlags::RELEASE_RESOURCES,
                    )
                    .expect("Reset command buffer failed.")
            };

            let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

            unsafe {
                device
                    .begin_command_buffer(command_buffer, &command_buffer_begin_info)
                    .expect("Begin cammandbuffer.")
            };
            f(device, command_buffer);
            unsafe {
                device
                    .end_command_buffer(command_buffer)
                    .expect("End commandbuffer")
            };

            let command_buffers = vec![command_buffer];

            let submit_info = vk::SubmitInfo::builder()
                .wait_semaphores(wait_semaphores)
                .wait_dst_stage_mask(wait_mask)
                .command_buffers(&command_buffers)
                .signal_semaphores(signal_semaphores);

            unsafe {
                device
                    .queue_submit(submit_queue, &[submit_info.build()], submit_fence)
                    .expect("Queue submit failed.")
            };

            self.active_command_buffer =
                (self.active_command_buffer + 1) % self.command_buffers.len();
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

    pub struct VkShaderModule {
        pub module: vk::ShaderModule,
        device: Arc<RawDevice>,
    }

    impl VkShaderModule {
        pub fn new<P: AsRef<std::path::Path>>(
            path: P,
            shader_type: shaderc::ShaderKind,
            compiler: &mut shaderc::Compiler,
            device: &VkDevice,
        ) -> VkResult<Self> {
            let shader_text = std::fs::read_to_string(&path).unwrap();
            let shader_data = compiler
                .compile_into_spirv(
                    &shader_text,
                    shader_type,
                    path.as_ref().to_str().unwrap(),
                    "main",
                    None,
                )
                .unwrap();
            let shader_data = shader_data.as_binary_u8();
            let mut shader_data = std::io::Cursor::new(shader_data);
            let shader_code = ash::util::read_spv(&mut shader_data).unwrap();
            let shader_info = vk::ShaderModuleCreateInfo::builder().code(&shader_code);

            let module = unsafe { device.create_shader_module(&shader_info, None) }?;
            Ok(VkShaderModule {
                module,
                device: device.device.clone(),
            })
        }
    }

    impl Drop for VkShaderModule {
        fn drop(&mut self) {
            unsafe { self.device.destroy_shader_module(self.module, None) };
        }
    }
}
