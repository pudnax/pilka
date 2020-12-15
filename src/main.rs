use pilka_lib::*;

#[cfg(debug_assertions)]
#[allow(unused_imports)]
#[allow(clippy::single_component_path_imports)]
use pilka_dyn;

use ash::{version::DeviceV1_0, vk};
use eyre::*;

use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
    platform::desktop::EventLoopExtDesktop,
};

use std::time::Instant;

const SHADER_PATH: &str = "shaders";
const SHADER_ENTRY_POINT: &str = "main";

fn main() -> Result<()> {
    // Initialize error hook.
    color_eyre::install()?;

    let time: Instant = Instant::now();

    let mut event_loop = winit::event_loop::EventLoop::new();

    let window = winit::window::WindowBuilder::new()
        .with_title("Pilka")
        .with_inner_size(winit::dpi::LogicalSize::new(
            f64::from(1280),
            f64::from(720),
        ))
        .build(&event_loop)?;

    let mut pilka = PilkaRender::new(&window).unwrap();
    let mut compiler = shaderc::Compiler::new().unwrap();
    let shaders = compile_shaders(SHADER_PATH, &mut compiler, &pilka.device).unwrap();

    for ash::VkShaderModule { path, module: _ } in &shaders {
        println!("{:?}", path);
    }
    for ash::VkShaderModule { path, module } in shaders {
        pilka.insert_shader_module(path.display().to_string(), module)?;
    }
    pilka.build_pipelines(
        vk::PipelineCache::null(),
        vec![(
            VertexShaderEntryPoint {
                module: "shaders/shader.vert".into(),
                entry_point: SHADER_ENTRY_POINT.to_string(),
            },
            FragmentShaderEntryPoint {
                module: "shaders/shader.frag".into(),
                entry_point: SHADER_ENTRY_POINT.to_string(),
            },
        )],
    )?;

    event_loop.run_return(|event, _, control_flow| {
        *control_flow = winit::event_loop::ControlFlow::Poll;
        match event {
            // What @.@
            Event::NewEvents(_) => {
                pilka.push_constants.time = time.elapsed().as_secs_f32();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(winit::dpi::PhysicalSize { width, height }) => {
                    // swapchain.info.image_extent = vk::Extent2D { width, height };
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
                WindowEvent::CursorMoved {
                    position: PhysicalPosition { x, y },
                    ..
                } => {
                    let vk::Extent2D { width, height } = pilka.extent;

                    pilka.push_constants.resolution = [
                        (x / width as f64 * 2.0 - 1.0) as f32,
                        -(y / height as f64 * 2.0 - 1.0) as f32,
                    ];
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                pilka.render();
            }
            Event::LoopDestroyed => {
                unsafe { pilka.device.device_wait_idle() }.unwrap();
            }
            _ => {}
        }
    });

    println!("End from the loop. Bye bye~");

    Ok(())
}
