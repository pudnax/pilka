use pilka_ash::ash::{prelude::VkResult, version::DeviceV1_0, ShaderInfo, *};
use pilka_ash::ash_window;
use std::{collections::HashMap, ffi::CStr, path::PathBuf};

/// The main struct that holds all render primitives
///
/// Rust documentation states for FIFO drop order for struct fields.
/// Or in the other words it's the same order that they're declared.
pub struct PilkaRender {
    pub push_constant: PushConstant,

    pub scissors: Box<[vk::Rect2D]>,
    pub viewports: Box<[vk::Viewport]>,
    pub extent: vk::Extent2D,

    pub shader_set: HashMap<PathBuf, usize>,
    pub compiler: shaderc::Compiler,

    pub rendering_complete_semaphore: vk::Semaphore,
    pub present_complete_semaphore: vk::Semaphore,
    pub command_pool: VkCommandPool,

    pub pipeline_cache: vk::PipelineCache,
    pub pipelines: Vec<VkPipeline>,
    pub render_pass: VkRenderPass,

    pub framebuffers: Vec<vk::Framebuffer>,
    pub swapchain: VkSwapchain,
    pub surface: VkSurface,

    pub device_properties: VkDeviceProperties,

    pub queues: VkQueues,
    pub device: VkDevice,
    pub instance: VkInstance,
    pub screenshot_ctx: ScreenshotCtx,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PushConstant {
    pub pos: [f32; 3],
    pub time: f32,
    pub wh: [f32; 2],
    pub mouse: [f32; 2],
}

impl PushConstant {
    unsafe fn as_slice(&self) -> &[u8] {
        std::slice::from_raw_parts((self as *const _) as *const _, std::mem::size_of::<Self>())
    }
}

impl std::fmt::Display for PushConstant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "position:\t{:?}\ntime:\t\t{}\nwidth, height:\t{:?}\nmouse:\t\t{:?}\n",
            self.pos, self.time, self.wh, self.mouse
        )
    }
}

impl PilkaRender {
    pub fn get_device_name(&self) -> Result<&str, std::str::Utf8Error> {
        unsafe { CStr::from_ptr(self.device_properties.properties.device_name.as_ptr()) }.to_str()
    }
    pub fn get_device_type(&self) -> pilka_ash::ash::vk::PhysicalDeviceType {
        self.device_properties.properties.device_type
    }

    pub fn new<W: HasRawWindowHandle>(window: &W) -> Result<Self, Box<dyn std::error::Error>> {
        let validation_layers = if cfg!(debug_assertions) {
            vec!["VK_LAYER_KHRONOS_validation\0"]
        } else {
            vec![]
        };
        let extention_names = ash_window::ash_window::enumerate_required_extensions(window)?;
        let instance = VkInstance::new(&validation_layers, &extention_names)?;

        let surface = instance.create_surface(window)?;

        let (device, device_properties, queues) =
            instance.create_device_and_queues(Some(&surface))?;

        let surface_resolution = surface.resolution(&device)?;

        let swapchain_loader = instance.create_swapchain_loader(&device);

        let swapchain = device.create_swapchain(swapchain_loader, &surface, &queues)?;
        let command_pool = device
            .create_vk_command_pool(queues.graphics_queue.index, swapchain.images.len() as u32)?;
        for i in 0..swapchain.images.len() {
            let submit_fence = command_pool.fences[i];
            let command_buffer = command_pool.command_buffers[i];

            unsafe { device.wait_for_fences(&[submit_fence], true, std::u64::MAX) }?;
            unsafe { device.reset_fences(&[submit_fence]) }?;

            let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

            unsafe { device.begin_command_buffer(command_buffer, &command_buffer_begin_info) }?;

            let barrier = vk::ImageMemoryBarrier::builder()
                .image(swapchain.images[i])
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build();
            unsafe {
                device.cmd_pipeline_barrier(
                    command_buffer,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                )
            };

            unsafe { device.end_command_buffer(command_buffer) }?;
            let command_buffers = vec![command_buffer];
            let submit_info = vk::SubmitInfo::builder().command_buffers(&command_buffers);

            unsafe {
                device.queue_submit(
                    queues.graphics_queue.queue,
                    &[submit_info.build()],
                    submit_fence,
                )
            }?;
        }

        let render_pass = device.create_vk_render_pass(swapchain.format())?;

        let present_complete_semaphore = device.create_semaphore()?;
        let rendering_complete_semaphore = device.create_semaphore()?;

        let framebuffers = swapchain.create_framebuffers(
            (surface_resolution.width, surface_resolution.height),
            &render_pass,
            &device,
        )?;

        let (viewports, scissors, extent) = {
            let surface_resolution = surface.resolution(&device)?;
            (
                Box::new([vk::Viewport {
                    x: 0.0,
                    y: surface_resolution.height as f32,
                    width: surface_resolution.width as f32,
                    height: -(surface_resolution.height as f32),
                    min_depth: 0.0,
                    max_depth: 1.0,
                }]),
                Box::new([vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: surface_resolution,
                }]),
                surface_resolution,
            )
        };

        let compiler = shaderc::Compiler::new().unwrap();

        let push_constant = PushConstant {
            pos: [0.; 3],
            wh: surface.resolution_slice(&device)?,
            mouse: [0.; 2],
            time: 0.,
        };

        let pipeline_cache_create_info = vk::PipelineCacheCreateInfo::builder();
        let pipeline_cache =
            unsafe { device.create_pipeline_cache(&pipeline_cache_create_info, None) }?;

        let screenshot_ctx = ScreenshotCtx::init(
            &device,
            &device_properties.memory,
            &command_pool,
            queues.graphics_queue.queue,
            extent,
        )?;

        Ok(Self {
            instance,
            device,
            queues,

            device_properties,

            surface,
            swapchain,
            framebuffers,

            render_pass,
            // pipeline_desc,
            pipelines: vec![],
            pipeline_cache,

            command_pool,
            present_complete_semaphore,
            rendering_complete_semaphore,

            shader_set: HashMap::new(),
            compiler,

            viewports,
            scissors,
            extent,

            push_constant,
            screenshot_ctx,
        })
    }

    // TODO(#17): Don't use `device_wait_idle` for resizing
    //
    // Probably Very bad! Consider waiting for approciate command buffers and fences
    // (i have no much choice of them) or restrict the amount of resizing events.
    pub fn resize(&mut self) -> VkResult<()> {
        unsafe { self.device.device_wait_idle() }?;

        self.extent = self.surface.resolution(&self.device)?;
        let vk::Extent2D { width, height } = self.extent;

        self.viewports.copy_from_slice(&[vk::Viewport {
            x: 0.,
            y: height as f32,
            width: width as f32,
            height: -(height as f32),
            min_depth: 0.0,
            max_depth: 1.0,
        }]);
        self.scissors = Box::new([vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D { width, height },
        }]);

        self.swapchain
            .recreate_swapchain((width, height), &self.device)?;

        for &framebuffer in &self.framebuffers {
            unsafe { self.device.destroy_framebuffer(framebuffer, None) };
        }
        for (framebuffer, present_image) in self
            .framebuffers
            .iter_mut()
            .zip(&self.swapchain.image_views)
        {
            let new_framebuffer = VkSwapchain::create_framebuffer(
                &[*present_image],
                (width, height),
                &self.render_pass,
                &self.device,
            )?;

            *framebuffer = new_framebuffer;
        }

        for i in 0..self.swapchain.images.len() {
            let submit_fence = self.command_pool.fences[i];
            let command_buffer = self.command_pool.command_buffers[i];

            unsafe {
                self.device
                    .wait_for_fences(&[submit_fence], true, std::u64::MAX)
            }?;
            unsafe { self.device.reset_fences(&[submit_fence]) }?;

            let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

            unsafe {
                self.device
                    .begin_command_buffer(command_buffer, &command_buffer_begin_info)
            }?;

            let barrier = vk::ImageMemoryBarrier::builder()
                .image(self.swapchain.images[i])
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build();
            unsafe {
                self.device.cmd_pipeline_barrier(
                    command_buffer,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                )
            };

            unsafe { self.device.end_command_buffer(command_buffer) }?;
            let command_buffers = vec![command_buffer];
            let submit_info = vk::SubmitInfo::builder().command_buffers(&command_buffers);

            unsafe {
                self.device.queue_submit(
                    self.queues.graphics_queue.queue,
                    &[submit_info.build()],
                    submit_fence,
                )
            }?;
        }

        Ok(())
    }

    pub fn push_shader_module(
        &mut self,
        vert_info: ShaderInfo,
        frag_info: ShaderInfo,
        dependencies: &[&str],
    ) -> VkResult<()> {
        let pipeline_number = self.pipelines.len();
        self.shader_set
            .insert(vert_info.name.canonicalize().unwrap(), pipeline_number);
        self.shader_set
            .insert(frag_info.name.canonicalize().unwrap(), pipeline_number);
        for deps in dependencies {
            self.shader_set
                .insert(PathBuf::from(deps).canonicalize().unwrap(), pipeline_number);
        }

        let new_pipeline = self.make_pipeline_from_shaders(&vert_info, &frag_info)?;
        self.pipelines.push(new_pipeline);

        Ok(())
    }

    pub fn make_pipeline_from_shaders(
        &mut self,
        vert_info: &ShaderInfo,
        frag_info: &ShaderInfo,
    ) -> VkResult<VkPipeline> {
        let vert_module = create_shader_module(
            vert_info.clone(),
            shaderc::ShaderKind::Vertex,
            &mut self.compiler,
            &self.device,
        )?;
        let frag_module = create_shader_module(
            frag_info.clone(),
            shaderc::ShaderKind::Fragment,
            &mut self.compiler,
            &self.device,
        )?;
        let shader_set = Box::new([
            vk::PipelineShaderStageCreateInfo {
                module: vert_module,
                p_name: vert_info.entry_point.as_ptr(),
                stage: vk::ShaderStageFlags::VERTEX,
                ..Default::default()
            },
            vk::PipelineShaderStageCreateInfo {
                module: frag_module,
                p_name: frag_info.entry_point.as_ptr(),
                stage: vk::ShaderStageFlags::FRAGMENT,
                ..Default::default()
            },
        ]);

        let new_pipeline =
            self.new_pipeline(self.pipeline_cache, shader_set, &vert_info, &frag_info)?;

        unsafe {
            self.device.destroy_shader_module(vert_module, None);
            self.device.destroy_shader_module(frag_module, None);
        }

        Ok(new_pipeline)
    }

    pub fn new_pipeline(
        &self,
        pipeline_cache: vk::PipelineCache,
        shader_set: Box<[vk::PipelineShaderStageCreateInfo]>,
        vs_info: &ShaderInfo,
        fs_info: &ShaderInfo,
    ) -> VkResult<VkPipeline> {
        let device = self.device.device.clone();
        let pipeline_layout = self.create_pipeline_layout()?;

        let desc = PipelineDescriptor::new(shader_set);

        Ok(VkPipeline::new(
            pipeline_cache,
            pipeline_layout,
            desc,
            &self.render_pass,
            vs_info.clone(),
            fs_info.clone(),
            device,
        )
        .unwrap())
    }

    pub fn rebuild_pipeline(&mut self, index: usize) -> VkResult<()> {
        let current_pipeline = &mut self.pipelines[index];
        let vs_info = current_pipeline.vs_info.clone();
        let fs_info = current_pipeline.fs_info.clone();
        let new_pipeline = match self.make_pipeline_from_shaders(&vs_info, &fs_info) {
            Ok(res) => res,
            Err(pilka_ash::ash::vk::Result::ERROR_UNKNOWN) => return Ok(()),
            Err(e) => return Err(e),
        };
        self.pipelines[index] = new_pipeline;

        Ok(())
    }

    pub fn render(&mut self) {
        let (present_index, is_suboptimal) = match unsafe {
            self.swapchain.swapchain_loader.acquire_next_image(
                self.swapchain.swapchain,
                std::u64::MAX,
                self.present_complete_semaphore,
                vk::Fence::null(),
            )
        } {
            Ok((index, check)) => (index, check),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) | Err(vk::Result::SUBOPTIMAL_KHR) => {
                println!("Oooopsie~ Get out-of-date swapchain in first time");
                self.resize()
                    .expect("Failed resize on acquiring next present image");
                return;
            }
            Err(_) => panic!(),
        };
        if is_suboptimal {
            self.resize()
                .expect("Failed resize on acquiring next present image");
            return;
        }

        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 1.0, 0.0],
            },
        }];

        let viewports = self.viewports.as_ref();
        let scissors = self.scissors.as_ref();
        let push_constant = self.push_constant;

        for pipeline in &self.pipelines[..] {
            let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
                .render_pass(*self.render_pass)
                .framebuffer(self.framebuffers[present_index as usize])
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self
                        .surface
                        .resolution(&self.device)
                        .expect("Failed to get surface resolution"),
                })
                .clear_values(&clear_values);

            let pipeline_layout = pipeline.pipeline_layout;
            let wait_mask = &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            // Start command queue
            unsafe {
                self.command_pool.record_submit_commandbuffer(
                    &self.device,
                    self.queues.graphics_queue.queue,
                    wait_mask,
                    &[self.present_complete_semaphore],
                    &[self.rendering_complete_semaphore],
                    |device, draw_command_buffer| {
                        device.cmd_begin_render_pass(
                            draw_command_buffer,
                            &render_pass_begin_info,
                            vk::SubpassContents::INLINE,
                        );
                        device.cmd_bind_pipeline(
                            draw_command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipeline.pipeline,
                        );
                        device.cmd_set_viewport(draw_command_buffer, 0, &viewports);
                        device.cmd_set_scissor(draw_command_buffer, 0, &scissors);

                        device.cmd_push_constants(
                            draw_command_buffer,
                            pipeline_layout,
                            vk::ShaderStageFlags::ALL_GRAPHICS,
                            0,
                            // TODO(#23): Find the better way to work with c_void
                            push_constant.as_slice(),
                        );

                        // Or draw without the index buffer
                        device.cmd_draw(draw_command_buffer, 3, 1, 0, 0);
                        device.cmd_end_render_pass(draw_command_buffer);
                    },
                );
            }
        }

        let wait_semaphores = [self.rendering_complete_semaphore];
        let swapchains = [self.swapchain.swapchain];
        let image_indices = [present_index];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        match unsafe {
            self.swapchain
                .swapchain_loader
                .queue_present(self.queues.graphics_queue.queue, &present_info)
        } {
            Ok(is_suboptimal) if is_suboptimal => {
                self.resize().expect("Failed resize on present.");
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) | Err(vk::Result::SUBOPTIMAL_KHR) => {
                self.resize().expect("Failed resize on present.");
            }
            Ok(_) => {}
            Err(e) => panic!("Unexpected error on presenting image: {}", e),
        }
    }

    pub fn create_pipeline_layout(&self) -> VkResult<vk::PipelineLayout> {
        let push_constant_range = vk::PushConstantRange::builder()
            .offset(0)
            .stage_flags(vk::ShaderStageFlags::ALL_GRAPHICS)
            .size(std::mem::size_of::<PushConstant>() as u32)
            .build();
        let layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .push_constant_ranges(&[push_constant_range])
            .build();
        unsafe {
            self.device
                .create_pipeline_layout(&layout_create_info, None)
        }
    }

    // TODO(#24): Make transfer command pool
    pub fn capture_image(&mut self) -> Result<(u32, u32), Box<dyn std::error::Error>> {
        let copybuffer = self.screenshot_ctx.commbuf;
        let cmd_begininfo = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { self.device.begin_command_buffer(copybuffer, &cmd_begininfo) }?;

        let extent = vk::Extent3D {
            width: self.extent.width,
            height: self.extent.height,
            depth: 1,
        };

        if self.screenshot_ctx.extent != extent {
            unsafe { self.device.destroy_image(self.screenshot_ctx.image, None) };

            let image_create_info = vk::ImageCreateInfo::builder()
                .format(vk::Format::R8G8B8A8_SRGB)
                .image_type(vk::ImageType::TYPE_2D)
                .extent(extent)
                .array_layers(1)
                .mip_levels(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::LINEAR)
                .usage(vk::ImageUsageFlags::TRANSFER_DST)
                .initial_layout(vk::ImageLayout::UNDEFINED);

            self.screenshot_ctx.image =
                unsafe { self.device.create_image(&image_create_info, None)? };
            self.screenshot_ctx.memory_reqs = unsafe {
                self.device
                    .get_image_memory_requirements(self.screenshot_ctx.image)
            };

            if self.screenshot_ctx.memory_reqs.size as usize > self.screenshot_ctx.data.len() {
                unsafe { self.device.unmap_memory(self.screenshot_ctx.memory) };
                unsafe { self.device.free_memory(self.screenshot_ctx.memory, None) }

                self.screenshot_ctx.data =
                    Vec::with_capacity(self.screenshot_ctx.memory_reqs.size as usize);
                self.screenshot_ctx.memory = self.device.alloc_memory(
                    &self.device_properties.memory,
                    self.screenshot_ctx.memory_reqs,
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                )?;

                self.screenshot_ctx.source_ptr = unsafe {
                    self.device.map_memory(
                        self.screenshot_ctx.memory,
                        0,
                        self.screenshot_ctx.memory_reqs.size,
                        vk::MemoryMapFlags::empty(),
                    )
                }? as *const u8;
            }

            unsafe {
                self.device.bind_image_memory(
                    self.screenshot_ctx.image,
                    self.screenshot_ctx.memory,
                    0,
                )
            }?;

            let barrier = vk::ImageMemoryBarrier::builder()
                .image(self.screenshot_ctx.image)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build();

            unsafe {
                self.device.cmd_pipeline_barrier(
                    self.screenshot_ctx.commbuf,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                )
            };
        }

        let source_image = self.swapchain.images[self.command_pool.active_command];

        let barrier = vk::ImageMemoryBarrier::builder()
            .image(source_image)
            .src_access_mask(vk::AccessFlags::MEMORY_READ)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .build();

        unsafe {
            self.device.cmd_pipeline_barrier(
                copybuffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            )
        };

        let zero_offset = vk::Offset3D::default();
        let copy_area = vk::ImageCopy::builder()
            .src_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_offset(zero_offset)
            .dst_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .dst_offset(zero_offset)
            .extent(extent)
            .build();

        unsafe {
            self.device.cmd_copy_image(
                copybuffer,
                source_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.screenshot_ctx.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[copy_area],
            )
        };

        let barrier = vk::ImageMemoryBarrier::builder()
            .image(source_image)
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .build();
        unsafe {
            self.device.cmd_pipeline_barrier(
                copybuffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            )
        };

        unsafe { self.device.end_command_buffer(copybuffer) }?;
        let submit_commbuffers = [copybuffer];
        let submit_infos = [vk::SubmitInfo::builder()
            .command_buffers(&submit_commbuffers)
            .build()];
        unsafe {
            self.device.queue_submit(
                self.queues.graphics_queue.queue,
                &submit_infos,
                self.screenshot_ctx.fence,
            )
        }?;
        unsafe {
            self.device
                .wait_for_fences(&[self.screenshot_ctx.fence], true, u64::MAX)
        }?;
        unsafe { self.device.reset_fences(&[self.screenshot_ctx.fence]) }?;

        let subresource_layout = unsafe {
            self.device.get_image_subresource_layout(
                self.screenshot_ctx.image,
                vk::ImageSubresource {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    array_layer: 0,
                },
            )
        };

        unsafe {
            std::ptr::copy(
                self.screenshot_ctx.source_ptr,
                self.screenshot_ctx.data.as_mut_ptr(),
                subresource_layout.size as usize,
            );
            self.screenshot_ctx
                .data
                .set_len(subresource_layout.size as usize);
        };

        Ok((
            subresource_layout.row_pitch as u32 / 4,
            (subresource_layout.size / subresource_layout.row_pitch) as u32,
        ))
    }
}

pub struct ScreenshotCtx {
    fence: vk::Fence,
    commbuf: vk::CommandBuffer,
    memory: vk::DeviceMemory,
    memory_reqs: vk::MemoryRequirements,
    source_ptr: *const u8,
    image: vk::Image,
    pub data: Vec<u8>,
    extent: vk::Extent3D,
}

impl ScreenshotCtx {
    pub fn init(
        device: &VkDevice,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        command_pool: &VkCommandPool,
        queue: vk::Queue,
        extent: vk::Extent2D,
    ) -> VkResult<Self> {
        let commandbuf_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool.pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let commbuf = unsafe { device.allocate_command_buffers(&commandbuf_allocate_info) }?[0];
        let fence = device.create_fence(false)?;
        let extent = vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        };

        let image_create_info = vk::ImageCreateInfo::builder()
            .format(vk::Format::R8G8B8A8_SRGB)
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .array_layers(1)
            .mip_levels(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::LINEAR)
            .usage(vk::ImageUsageFlags::TRANSFER_DST)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe { device.create_image(&image_create_info, None)? };
        let memory_reqs = unsafe { device.get_image_memory_requirements(image) };

        let data = Vec::with_capacity(memory_reqs.size as usize);
        let memory = device.alloc_memory(
            memory_properties,
            memory_reqs,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        unsafe { device.bind_image_memory(image, memory, 0) }?;
        let source_ptr =
            unsafe { device.map_memory(memory, 0, memory_reqs.size, vk::MemoryMapFlags::empty()) }?
                as *const u8;

        let cmd_begininfo = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { device.begin_command_buffer(commbuf, &cmd_begininfo) }?;

        let barrier = vk::ImageMemoryBarrier::builder()
            .image(image)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .build();

        unsafe {
            device.cmd_pipeline_barrier(
                commbuf,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            )
        };

        unsafe { device.end_command_buffer(commbuf) }?;
        let submit_commbuffers = [commbuf];
        let submit_infos = [vk::SubmitInfo::builder()
            .command_buffers(&submit_commbuffers)
            .build()];
        unsafe { device.queue_submit(queue, &submit_infos, fence) }?;
        unsafe { device.wait_for_fences(&[fence], true, u64::MAX) }?;
        unsafe { device.reset_fences(&[fence]) }?;

        Ok(Self {
            fence,
            commbuf,
            memory,
            memory_reqs,
            source_ptr,
            image,
            data,
            extent,
        })
    }

    fn destroy(&self, device: &VkDevice) {
        unsafe {
            device.unmap_memory(self.memory);
            device.destroy_fence(self.fence, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }
}

impl Drop for PilkaRender {
    fn drop(&mut self) {
        unsafe {
            self.screenshot_ctx.destroy(&self.device);
            self.device
                .destroy_pipeline_cache(self.pipeline_cache, None);

            self.device
                .destroy_semaphore(self.present_complete_semaphore, None);
            self.device
                .destroy_semaphore(self.rendering_complete_semaphore, None);

            for &framebuffer in &self.framebuffers {
                self.device.destroy_framebuffer(framebuffer, None);
            }
        }
    }
}
