use pilka_lib::*;

#[cfg(debug_assertions)]
#[allow(unused_imports)]
#[allow(clippy::single_component_path_imports)]
use pilka_dyn;

use ash::{version::DeviceV1_0, vk};
use eyre::*;

use winit::{
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
    platform::desktop::EventLoopExtDesktop,
};

#[repr(C)]
#[derive(Clone, Debug, Copy)]
struct Vertex {
    pos: [f32; 4],
    color: [f32; 4],
}

fn main() -> Result<()> {
    // Initialize error hook.
    color_eyre::install()?;

    let mut event_loop = winit::event_loop::EventLoop::new();
    let window = winit::window::Window::new(&event_loop)?;
    window.set_title("Pilka");

    let instance = ash::VkInstance::new(Some(&window))?;

    let surface = instance.create_surface(&window)?;

    let (device, _device_properties, queues) = instance.create_device_and_queues(Some(&surface))?;

    let mut swapchain = instance.create_swapchain(&device, &surface, &queues)?;
    let render_pass = device.create_vk_render_pass(&mut swapchain)?;

    let mut command_pool = device.create_commmand_buffer(queues.graphics_queue.1, 3)?;

    //////////////////////////////////////////////////////////////////////////////
    let mut compiler =
        shaderc::Compiler::new().with_context(|| "Failed to create shader compiler")?;
    let vertex_shader_module = ash::VkShaderModule::new(
        "shaders/shader.vert",
        shaderc::ShaderKind::Vertex,
        &mut compiler,
        &device,
    )?;
    let fragment_shader_module = ash::VkShaderModule::new(
        "shaders/shader.frag",
        shaderc::ShaderKind::Fragment,
        &mut compiler,
        &device,
    )?;
    //////////////////////////////////////////////////////////////////////////////////
    let graphic_pipeline = ash::VkPipeline::new(
        vertex_shader_module.module,
        fragment_shader_module.module,
        swapchain.extent,
        &render_pass,
        device.device.clone(),
    )?;

    let semaphore_create_info = vk::SemaphoreCreateInfo::default();
    let present_complete_semaphore =
        unsafe { device.create_semaphore(&semaphore_create_info, None) }?;
    let rendering_complete_semaphore =
        unsafe { device.create_semaphore(&semaphore_create_info, None) }?;

    event_loop.run_return(|event, _, control_flow| {
        *control_flow = winit::event_loop::ControlFlow::Poll;
        match event {
            // What @.@
            // Event::NewEvents(_) => {
            //     inputs.wheel_delta = 0.0;
            // }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(winit::dpi::PhysicalSize { width, height }) => {
                    swapchain.info.image_extent = vk::Extent2D { width, height };
                    // swapchain
                    //     .recreate_swapchain(new_size.width, new_size.height)
                    //     .expect("Failed to recreate swapchain.");
                }
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            virtual_keycode: Some(keycode),
                            state: ElementState::Pressed,
                            ..
                        },
                    ..
                } => {
                    if VirtualKeyCode::Escape == keycode {
                        *control_flow = ControlFlow::Exit;
                    }
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                let (present_index, _) = unsafe {
                    swapchain.swapchain_loader.acquire_next_image(
                        swapchain.swapchain,
                        std::u64::MAX,
                        present_complete_semaphore,
                        vk::Fence::null(),
                    )
                }
                .unwrap();
                let clear_values = [vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 0.0],
                    },
                }];

                let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
                    .render_pass(*render_pass)
                    .framebuffer(swapchain.framebuffers[present_index as usize])
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: swapchain.extent,
                    })
                    .clear_values(&clear_values);

                let wait_mask = &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
                // Start command queue
                unsafe {
                    command_pool.record_submit_commandbuffer(
                        &device,
                        queues.graphics_queue.0,
                        wait_mask,
                        &[present_complete_semaphore],
                        &[rendering_complete_semaphore],
                        |device, draw_command_buffer| {
                            device.cmd_begin_render_pass(
                                draw_command_buffer,
                                &render_pass_begin_info,
                                vk::SubpassContents::INLINE,
                            );
                            device.cmd_bind_pipeline(
                                draw_command_buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                graphic_pipeline.get(),
                            );
                            device.cmd_set_viewport(
                                draw_command_buffer,
                                0,
                                &graphic_pipeline.viewports,
                            );
                            device.cmd_set_scissor(
                                draw_command_buffer,
                                0,
                                &graphic_pipeline.scissors,
                            );
                            // Or draw without the index buffer
                            device.cmd_draw(draw_command_buffer, 3, 1, 0, 0);
                            device.cmd_end_render_pass(draw_command_buffer);
                        },
                    );
                }

                let wait_semaphores = [rendering_complete_semaphore];
                let swapchains = [swapchain.swapchain];
                let image_indices = [present_index];
                let present_info = vk::PresentInfoKHR::builder()
                    .wait_semaphores(&wait_semaphores)
                    .swapchains(&swapchains)
                    .image_indices(&image_indices);
                match unsafe {
                    swapchain
                        .swapchain_loader
                        .queue_present(queues.graphics_queue.0, &present_info)
                } {
                    Ok(_) => {}
                    Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                        // swapchain
                        //     .recreate_swapchain(swapchain.extent.width, swapchain.extent.height)
                        //     .expect("Failed to recreate swapchain.");
                    }
                    Err(_) => {
                        panic!("Derpy error.");
                    }
                }
            }
            Event::LoopDestroyed => {
                unsafe { device.device_wait_idle() }.unwrap();
            }
            _ => {}
        }
    });

    println!("End from the loop. Bye bye~");

    unsafe {
        device.destroy_semaphore(present_complete_semaphore, None);
        device.destroy_semaphore(rendering_complete_semaphore, None);
    }

    Ok(())
}

pub fn find_memorytype_index(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_prop.memory_types[..memory_prop.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_req.memory_type_bits != 0
                && (memory_type.property_flags & flags) == flags
        })
        .map(|(index, _memory_type)| index as _)
}
