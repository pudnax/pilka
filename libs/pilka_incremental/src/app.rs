use pilka_ash::ash::{prelude::VkResult, version::DeviceV1_0, ShaderInfo, *};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// The main struct that holds all render primitives
///
/// Rust documentation states for FIFO drop order for struct fields.
/// Or in the other words it's the same order that they're declared.
pub struct PilkaRender {
    pub scissors: Box<[vk::Rect2D]>,
    pub viewports: Box<[vk::Viewport]>,
    pub extent: vk::Extent2D,

    pub push_constants: PushConstants,

    pub shader_set: HashMap<PathBuf, usize>,
    pub compiler: shaderc::Compiler,

    pub rendering_complete_semaphore: vk::Semaphore,
    pub present_complete_semaphore: vk::Semaphore,
    pub command_pool: VkCommandPool,

    // FIXME: Where is `PipeilineCache`?
    pub pipelines: Vec<VkPipeline>,
    pub render_pass: VkRenderPass,

    pub framebuffers: Vec<vk::Framebuffer>,
    pub swapchain: VkSwapchain,
    pub surface: VkSurface,

    pub queues: VkQueues,
    pub device: VkDevice,
    pub instance: VkInstance,
}

pub fn compile_shaders<P: AsRef<Path>>(
    dir: P,
    compiler: &mut shaderc::Compiler,
    device: &VkDevice,
) -> Result<Vec<vk::ShaderModule>, Box<dyn std::error::Error>> {
    let mut shader_modules = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let shader_type = match path.extension().and_then(std::ffi::OsStr::to_str) {
            Some("frag") => shaderc::ShaderKind::Fragment,
            Some("vert") => shaderc::ShaderKind::Vertex,
            Some("comp") => shaderc::ShaderKind::Compute,
            _ => {
                panic!(
                    "Did not recognize file extension for shader file \"{:?}\"",
                    path
                );
            }
        };
        let module = create_shader_module(
            ShaderInfo {
                name: path,
                entry_point: "main".to_string(),
            },
            shader_type,
            compiler,
            device,
        )
        .expect("Failed to create shader module");
        shader_modules.push(module);
    }

    Ok(shader_modules)
}

pub struct PushConstants {
    pub resolution: [f32; 2],
    pub mouse: [f32; 2],
    pub time: f32,
}

impl PilkaRender {
    pub fn new<W: HasRawWindowHandle>(window: &W) -> Result<Self, Box<dyn std::error::Error>> {
        let instance = VkInstance::new(Some(window))?;

        let surface = instance.create_surface(window)?;

        let (device, _device_properties, queues) =
            instance.create_device_and_queues(Some(&surface))?;

        let surface_resolution = surface.resolution(&device)?;

        let swapchain_loader = instance.create_swapchain_loader(&device);

        let swapchain = device.create_swapchain(swapchain_loader, &surface, &queues)?;
        let render_pass = device.create_vk_render_pass(swapchain.format())?;

        let command_pool = device.create_commmand_buffer(queues.graphics_queue.index, 3)?;

        let present_complete_semaphore = device.create_semaphore()?;
        let rendering_complete_semaphore = device.create_semaphore()?;

        let framebuffers = swapchain.create_framebuffers(
            (surface_resolution.width, surface_resolution.height),
            &render_pass,
            &device,
        )?;

        let push_constants = PushConstants {
            resolution: surface.resolution_slice(&device)?,
            mouse: [0.0; 2],
            time: 0.,
        };

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

        Ok(Self {
            instance,
            device,
            queues,

            surface,
            swapchain,
            framebuffers,

            render_pass,
            // pipeline_desc,
            pipelines: vec![],

            command_pool,
            present_complete_semaphore,
            rendering_complete_semaphore,

            shader_set: HashMap::new(),
            compiler,

            push_constants,

            viewports,
            scissors,
            extent,
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
        self.push_constants.resolution = [width as f32, height as f32];

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
            .insert(vert_info.name.clone(), pipeline_number);
        self.shader_set
            .insert(frag_info.name.clone(), pipeline_number);
        for deps in dependencies {
            self.shader_set.insert(PathBuf::from(deps), pipeline_number);
        }

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
                p_name: vert_info.entry_point.as_ptr() as *const i8,
                stage: vk::ShaderStageFlags::VERTEX,
                ..Default::default()
            },
            vk::PipelineShaderStageCreateInfo {
                module: frag_module,
                p_name: frag_info.entry_point.as_ptr() as *const i8,
                stage: vk::ShaderStageFlags::FRAGMENT,
                ..Default::default()
            },
        ]);

        self.pipelines.push(self.new_pipeline(
            vk::PipelineCache::null(),
            shader_set,
            &vert_info,
            &frag_info,
        )?);

        unsafe {
            self.device.destroy_shader_module(vert_module, None);
            self.device.destroy_shader_module(frag_module, None);
        }

        Ok(())
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

    pub fn rebuild_pipeline(
        &mut self,
        index: usize,
        shader_set: Box<[vk::PipelineShaderStageCreateInfo]>,
    ) -> VkResult<()> {
        self.pipelines[index] = self.new_pipeline(
            vk::PipelineCache::null(),
            shader_set,
            &self.pipelines[index].vs_info,
            &self.pipelines[index].fs_info,
        )?;
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
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                // println!("Oooopsie~ 2");
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
        let command_pool = &mut self.command_pool;
        let viewports = self.viewports.as_ref();
        let scissors = self.scissors.as_ref();
        let push_constants = &self.push_constants;

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

            let wait_mask = &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            // Start command queue
            unsafe {
                command_pool.record_submit_commandbuffer(
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
                        // TODO: Command buffers have to be recompiled to update
                        // push constants.
                        device.cmd_push_constants(
                            draw_command_buffer,
                            pipeline.pipeline_layout,
                            vk::ShaderStageFlags::all(),
                            0,
                            any_as_u8_slice(&push_constants),
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
            Ok(_) => {}
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => { /*println!("Oooopsie~ 2") */ }
            Err(_) => panic!(),
        }
    }

    pub fn create_pipeline_layout(&self) -> VkResult<vk::PipelineLayout> {
        let push_constant_range = vk::PushConstantRange::builder()
            .offset(0)
            .size(std::mem::size_of::<PushConstants>() as u32)
            .stage_flags(vk::ShaderStageFlags::all())
            .build();
        let layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .push_constant_ranges(&[push_constant_range])
            .build();
        unsafe {
            self.device
                .create_pipeline_layout(&layout_create_info, None)
        }
    }

    // pub fn rebuild_pipelines(&mut self, pipeline_cache: vk::PipelineCache) -> VkResult<()> {
    //     let device = self.device.device.clone();
    //     let pipeline_layout = self.create_pipeline_layout()?;
    //     let modules_names = self
    //         .shader_set
    //         .iter()
    //         .map(|(vert, frag)| {
    //             let vert_module = *self.shader_modules.get(&vert.module).unwrap();
    //             let vert_name = CString::new(vert.entry_point.clone()).unwrap();
    //             let frag_module = *self.shader_modules.get(&frag.module).unwrap();
    //             let frag_name = CString::new(frag.entry_point.clone()).unwrap();
    //             ((frag_module, frag_name), (vert_module, vert_name))
    //         })
    //         .collect::<Vec<_>>();
    //     let viewport = vk::PipelineViewportStateCreateInfo::builder()
    //         // TODO: Look at this
    //         .viewports(self.viewports.as_ref())
    //         .scissors(self.scissors.as_ref());
    //     let descs = modules_names
    //         .iter()
    //         .map(|((frag_module, frag_name), (vert_module, vert_name))| {
    //             PipelineDescriptor::new(Box::new([
    //                 vk::PipelineShaderStageCreateInfo {
    //                     module: *vert_module,
    //                     p_name: (*vert_name).as_ptr(),
    //                     stage: vk::ShaderStageFlags::VERTEX,
    //                     ..Default::default()
    //                 },
    //                 vk::PipelineShaderStageCreateInfo {
    //                     // `s_type` is optionated
    //                     s_type: vk::StructureType::PIPELINE_SHADER_STAGE_CREATE_INFO,
    //                     module: *frag_module,
    //                     p_name: (*frag_name).as_ptr(),
    //                     stage: vk::ShaderStageFlags::FRAGMENT,
    //                     ..Default::default()
    //                 },
    //             ]))
    //         })
    //         .collect::<Vec<_>>();
    //     let pipeline_info = descs
    //         .iter()
    //         .map(|desc| {
    //             vk::GraphicsPipelineCreateInfo::builder()
    //                 .stages(&desc.shader_stages)
    //                 .vertex_input_state(&desc.vertex_input)
    //                 .input_assembly_state(&desc.input_assembly)
    //                 .rasterization_state(&desc.rasterization)
    //                 .multisample_state(&desc.multisample)
    //                 .depth_stencil_state(&desc.depth_stencil)
    //                 .color_blend_state(&desc.color_blend)
    //                 .dynamic_state(&desc.dynamic_state_info)
    //                 .viewport_state(&viewport)
    //                 .layout(pipeline_layout)
    //                 .render_pass(self.render_pass.render_pass)
    //                 .build()
    //         })
    //         .collect::<Vec<_>>();
    //     self.pipelines = unsafe {
    //         self.device
    //             .create_graphics_pipelines(pipeline_cache, &pipeline_info, None)
    //             .expect("Unable to create graphics pipeline")
    //     }
    //     .iter()
    //     .zip(descs)
    //     .map(|(&pipeline, desc)| VkPipeline {
    //         pipeline,
    //         pipeline_layout,
    //         dynamic_state: desc.dynamic_state,
    //         device: device.clone(),
    //     })
    //     .collect();
    //     Ok(())
    // }
}

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts((p as *const T) as *const u8, ::std::mem::size_of::<T>())
}

impl Drop for PilkaRender {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_semaphore(self.present_complete_semaphore, None);
            self.device
                .destroy_semaphore(self.rendering_complete_semaphore, None);

            for &framebuffer in &self.framebuffers {
                self.device.destroy_framebuffer(framebuffer, None);
            }
            // for &shader_module in self.shader_modules.values() {
            //     self.device.destroy_shader_module(shader_module, None);
            // }
        }
    }
}
