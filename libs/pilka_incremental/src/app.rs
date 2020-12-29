use pilka_ash::ash::{pilka_util, prelude::VkResult, version::DeviceV1_0, ShaderInfo, *};
use std::{collections::HashMap, path::PathBuf};

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
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PushConstant {
    pub wh: [f32; 2],
    pub mouse: [f32; 2],
    pub time: f32,
}

impl PilkaRender {
    pub fn new<W: HasRawWindowHandle>(window: &W) -> Result<Self, Box<dyn std::error::Error>> {
        let instance = VkInstance::new(Some(window))?;

        let surface = instance.create_surface(window)?;

        let (device, device_properties, queues) =
            instance.create_device_and_queues(Some(&surface))?;

        let surface_resolution = surface.resolution(&device)?;

        let swapchain_loader = instance.create_swapchain_loader(&device);

        let swapchain = device.create_swapchain(swapchain_loader, &surface, &queues)?;
        let render_pass = device.create_vk_render_pass(swapchain.format())?;

        let command_pool = device
            .create_commmand_pool(queues.graphics_queue.index, swapchain.images.len() as u32)?;

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
            wh: surface.resolution_slice(&device)?,
            mouse: [0.; 2],
            time: 0.,
        };

        let pipeline_cache_create_info = vk::PipelineCacheCreateInfo::builder();
        let pipeline_cache =
            unsafe { device.create_pipeline_cache(&pipeline_cache_create_info, None) }?;

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
            // TODO: Convert entry point into CString
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
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                println!("Oooopsie~ Get out-of-date swapchain in first time");
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
                            // TODO: Find better way to work with c_void
                            any_as_u8_slice(&push_constant),
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
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                println!("Oooopsie~ Get out-of-date swapchain");
            }
            Err(_) => panic!(),
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

    pub fn screenshot(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let commandbuf_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool.pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let copybuffer = unsafe {
            self.device
                .allocate_command_buffers(&commandbuf_allocate_info)
        }?[0];

        let cmd_begininfo = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(copybuffer, &cmd_begininfo)?
        };

        let extent = vk::Extent3D {
            width: self.extent.width,
            height: self.extent.height,
            depth: 1,
        };
        let image_create_info = vk::ImageCreateInfo::builder()
            .format(vk::Format::R8G8B8A8_UNORM)
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .array_layers(1)
            .mip_levels(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::LINEAR)
            .usage(vk::ImageUsageFlags::TRANSFER_DST)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let destination_image = unsafe { self.device.create_image(&image_create_info, None)? };
        let image_memory_reqs =
            unsafe { self.device.get_image_memory_requirements(destination_image) };

        let memory_type_index = pilka_util::find_memorytype_index(
            &image_memory_reqs,
            &self.device_properties.memory,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .unwrap();
        let alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(image_memory_reqs.size)
            .memory_type_index(memory_type_index)
            .build();
        let destination_image_memory = unsafe { self.device.allocate_memory(&alloc_info, None) }?;
        unsafe {
            self.device
                .bind_image_memory(destination_image, destination_image_memory, 0)
        }?;

        let barrier = vk::ImageMemoryBarrier::builder()
            .image(destination_image)
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
                copybuffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            )
        };

        // FIXME: index by pool.active_command? What?
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
                destination_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[copy_area],
            )
        };

        let barrier = vk::ImageMemoryBarrier::builder()
            .image(destination_image)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::GENERAL)
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
        let submit_infos = [vk::SubmitInfo::builder()
            .command_buffers(&[copybuffer])
            .build()];
        let fence = self.device.create_fence(false)?;
        unsafe {
            self.device
                .queue_submit(self.queues.graphics_queue.queue, &submit_infos, fence)
        }?;
        unsafe { self.device.wait_for_fences(&[fence], true, u64::MAX) }?;
        unsafe {
            self.device
                .free_command_buffers(self.command_pool.pool, &[copybuffer])
        };

        let source_ptr = unsafe {
            self.device.map_memory(
                destination_image_memory,
                0,
                image_memory_reqs.size,
                vk::MemoryMapFlags::empty(),
            )
        }? as *mut u8;
        let subresource_layout = unsafe {
            self.device.get_image_subresource_layout(
                destination_image,
                vk::ImageSubresource {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    array_layer: 0,
                },
            )
        };

        let mut data = Vec::with_capacity(subresource_layout.size as usize);

        unsafe {
            std::ptr::copy(
                source_ptr,
                data.as_mut_ptr(),
                subresource_layout.size as usize,
            );
            data.set_len(subresource_layout.size as usize);
        };

        unsafe { self.device.unmap_memory(destination_image_memory) };
        unsafe { self.device.destroy_image(destination_image, None) };
        unsafe { self.device.free_memory(destination_image_memory, None) };
        unsafe { self.device.destroy_fence(fence, None) };

        Ok(data)
    }
}

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    std::slice::from_raw_parts((p as *const T) as *const u8, std::mem::size_of::<T>())
}

impl Drop for PilkaRender {
    fn drop(&mut self) {
        unsafe {
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
