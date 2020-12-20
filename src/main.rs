use pilka_lib::*;

#[cfg(debug_assertions)]
#[allow(unused_imports)]
#[allow(clippy::single_component_path_imports)]
use pilka_dyn;

use ash::{version::DeviceV1_0, vk};
use eyre::*;

use notify::{
    event::{EventKind, ModifyKind},
    RecommendedWatcher, RecursiveMode, Watcher,
};
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
    platform::desktop::EventLoopExtDesktop,
};

use std::{path::PathBuf, time::Instant};

// const SHADER_PATH: &str = "shaders";
// const SHADER_ENTRY_POINT: &str = "main";

fn main() -> Result<()> {
    // Initialize error hook.
    color_eyre::install()?;

    let _time: Instant = Instant::now();

    let mut event_loop = winit::event_loop::EventLoop::new();

    let window = winit::window::WindowBuilder::new()
        .with_title("Pilka")
        .with_inner_size(winit::dpi::LogicalSize::new(
            f64::from(1280),
            f64::from(720),
        ))
        .build(&event_loop)?;

    let mut pilka = PilkaRender::new(&window).unwrap();
    // TODO: Think about canonicalize
    pilka.push_shader_module(
        ash::ShaderInfo {
            name: PathBuf::from("shaders/shader.vert"),
            entry_point: "main".to_string(),
        },
        ash::ShaderInfo {
            name: PathBuf::from("shaders/shader.frag"),
            entry_point: "main".to_string(),
        },
        &[],
    )?;

    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher: RecommendedWatcher = Watcher::new_immediate(move |res| match res {
        Ok(event) => {
            tx.send(event).unwrap();
        }
        Err(e) => println!("watch error: {:?}", e),
    })?;

    watcher.watch("shaders/", RecursiveMode::Recursive)?;

    let mut pipelines_to_recompile = std::collections::HashSet::new();
    let mut ctrl_pressed = false;

    event_loop.run_return(|event, _, control_flow| {
        *control_flow = winit::event_loop::ControlFlow::Poll;
        match event {
            // What @.@
            Event::NewEvents(_) => {
                if let Ok(rx_event) = rx.try_recv() {
                    if let notify::Event {
                        kind: EventKind::Modify(ModifyKind::Data(_)),
                        ..
                    } = rx_event
                    {
                        for path in rx_event.paths {
                            if pilka.shader_set.contains_key(&path) {
                                pipelines_to_recompile.insert(pilka.shader_set[&path]);
                            }
                        }
                        println!("{:?}", &pipelines_to_recompile);
                    }
                }
            }

            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::ModifiersChanged(state) => {
                    ctrl_pressed = state.ctrl();
                }
                WindowEvent::Resized(PhysicalSize { .. }) => {
                    let vk::Extent2D { width, height } =
                        pilka.surface.resolution(&pilka.device).unwrap();
                    let vk::Extent2D {
                        width: old_width,
                        height: old_height,
                    } = pilka.extent;

                    if width == old_width && height == old_height {
                        return;
                    }

                    pilka.resize().unwrap();
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
                    if VirtualKeyCode::R == keycode && ctrl_pressed {
                        println!("Event!");

                        unsafe {
                            // FIXME: Just forget the existing of this function, you lack!
                            pilka.device.device_wait_idle().unwrap();
                        }
                        for index in pipelines_to_recompile.drain() {
                            pilka.rebuild_pipeline(index).unwrap();
                        }
                    }
                }
                // WindowEvent::CursorMoved {
                //     position: PhysicalPosition { x, y },
                //     ..
                // } => {
                //     let vk::Extent2D { width, height } = pilka.extent;
                // }
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
