use pilka_ash::ash::{prelude::VkResult, version::DeviceV1_0, *};
use std::{collections::HashMap, ffi::CString, path::Path};

pub struct PilkaRender {
    pub instance: VkInstance,
    pub device: VkDevice,
    pub queues: VkQueues,

    pub surface: VkSurface,
    pub swapchain: VkSwapchain,
    pub framebuffers: Vec<vk::Framebuffer>,

    pub render_pass: VkRenderPass,
    // pub pipeline_desc: PipelineDescriptor,
    pub pipelines: Vec<VkPipeline>,

    pub command_pool: VkCommandPool,
    pub present_complete_semaphore: vk::Semaphore,
    pub rendering_complete_semaphore: vk::Semaphore,

    pub shader_modules: HashMap<String, vk::ShaderModule>,
    pub shader_set: Vec<(VertexShaderEntryPoint, FragmentShaderEntryPoint)>,

    pub push_constants: PushConstants,

    pub viewports: Box<[vk::Viewport]>,
    pub scissors: Box<[vk::Rect2D]>,
}

pub fn compile_shaders<P: AsRef<Path>>(
    dir: P,
    compiler: &mut shaderc::Compiler,
    device: &VkDevice,
) -> Result<Vec<VkShaderModule>, Box<dyn std::error::Error>> {
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
        let module = VkShaderModule::new(path, shader_type, compiler, device)
            .expect("Failed to create shader module");
        shader_modules.push(module);
    }

    Ok(shader_modules)
}

pub struct VertexShaderEntryPoint {
    pub module: String,
    pub entry_point: String,
}

pub struct FragmentShaderEntryPoint {
    pub module: String,
    pub entry_point: String,
}

pub struct PushConstants {
    pub wh: [f32; 2],
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

        let framebuffers: Result<Vec<_>, _> = swapchain
            .image_views
            .iter()
            .map(|&present_image_view| {
                let framebuffer_attachments = [present_image_view];
                unsafe {
                    device.create_framebuffer(
                        &vk::FramebufferCreateInfo::builder()
                            .render_pass(*render_pass)
                            .attachments(&framebuffer_attachments)
                            .width(surface_resolution.width)
                            .height(surface_resolution.height)
                            .layers(1),
                        None,
                    )
                }
            })
            .collect();
        let framebuffers = framebuffers?;

        let push_constants = PushConstants {
            wh: surface.resolution_slice(&device)?,
            time: 0.,
        };

        let (viewports, scissors) = {
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
            )
        };

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

            shader_modules: HashMap::new(),
            shader_set: vec![],

            push_constants,

            viewports,
            scissors,
        })
    }

    pub fn render(&mut self) {
        let (present_index, _) = unsafe {
            self.swapchain.swapchain_loader.acquire_next_image(
                self.swapchain.swapchain,
                std::u64::MAX,
                self.present_complete_semaphore,
                vk::Fence::null(),
            )
        }
        .expect("failed to acquire next image");
        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 1.0, 0.0],
            },
        }];
        let command_pool = &mut self.command_pool;
        let viewports = self.viewports.as_ref();
        let scissors = self.scissors.as_ref();
        let push_constants = &self.push_constants;

        for pipeline in &self.pipelines {
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
        unsafe {
            self.swapchain
                .swapchain_loader
                .queue_present(self.queues.graphics_queue.queue, &present_info)
                .expect("Failed to submit queue.");
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

    pub fn rebuild_pipelines(&mut self, pipeline_cache: vk::PipelineCache) -> VkResult<()> {
        let device = self.device.device.clone();
        let pipeline_layout = self.create_pipeline_layout()?;
        let modules_names = self
            .shader_set
            .iter()
            .map(|(vert, frag)| {
                let vert_module = *self.shader_modules.get(&vert.module).unwrap();
                let vert_name = CString::new(vert.entry_point.clone()).unwrap();
                let frag_module = *self.shader_modules.get(&frag.module).unwrap();
                let frag_name = CString::new(frag.entry_point.clone()).unwrap();
                ((frag_module, frag_name), (vert_module, vert_name))
            })
            .collect::<Vec<_>>();
        let viewport = vk::PipelineViewportStateCreateInfo::builder();
        let descs = modules_names
            .iter()
            .map(|((frag_module, frag_name), (vert_module, vert_name))| {
                PipelineDescriptor::new(Box::new([
                    vk::PipelineShaderStageCreateInfo {
                        module: *vert_module,
                        p_name: (*vert_name).as_ptr(),
                        stage: vk::ShaderStageFlags::VERTEX,
                        ..Default::default()
                    },
                    vk::PipelineShaderStageCreateInfo {
                        // `s_type` is optionated
                        s_type: vk::StructureType::PIPELINE_SHADER_STAGE_CREATE_INFO,
                        module: *frag_module,
                        p_name: (*frag_name).as_ptr(),
                        stage: vk::ShaderStageFlags::FRAGMENT,
                        ..Default::default()
                    },
                ]))
            })
            .collect::<Vec<_>>();
        let pipeline_info = descs
            .iter()
            .map(|desc| {
                vk::GraphicsPipelineCreateInfo::builder()
                    .stages(&desc.shader_stages)
                    .vertex_input_state(&desc.vertex_input)
                    .input_assembly_state(&desc.input_assembly)
                    .rasterization_state(&desc.rasterization)
                    .multisample_state(&desc.multisample)
                    .depth_stencil_state(&desc.depth_stencil)
                    .color_blend_state(&desc.color_blend)
                    .dynamic_state(&desc.dynamic_state_info)
                    .viewport_state(&viewport)
                    .layout(pipeline_layout)
                    .render_pass(self.render_pass.render_pass)
                    .build()
            })
            .collect::<Vec<_>>();
        self.pipelines = unsafe {
            self.device
                .create_graphics_pipelines(pipeline_cache, &pipeline_info, None)
                .expect("Unable to create graphics pipeline")
        }
        .iter()
        .zip(descs)
        .map(|(&pipeline, desc)| VkPipeline {
            pipeline,
            pipeline_layout,
            color_blend_attachments: desc.color_blend_attachments,
            dynamic_state: desc.dynamic_state,
            device: device.clone(),
        })
        .collect();
        Ok(())
    }

    pub fn build_pipelines(
        &mut self,
        pipeline_cache: vk::PipelineCache,
        shader_set: Vec<(VertexShaderEntryPoint, FragmentShaderEntryPoint)>,
    ) -> VkResult<()> {
        self.shader_set = shader_set;
        self.rebuild_pipelines(pipeline_cache)?;
        Ok(())
    }

    pub fn insert_shader_module(
        &mut self,
        name: String,
        shader_module: vk::ShaderModule,
    ) -> VkResult<()> {
        if let Some(old_module) = self.shader_modules.insert(name, shader_module) {
            unsafe { self.device.destroy_shader_module(old_module, None) }
        };
        Ok(())
    }
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

            for &shader_module in self.shader_modules.values() {
                self.device.destroy_shader_module(shader_module, None);
            }
        }
    }
}
