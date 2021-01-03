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
    dpi::PhysicalPosition,
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
    platform::desktop::EventLoopExtDesktop,
};

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

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
    pilka.push_shader_module(
        ash::ShaderInfo::new(
            PathBuf::from([SHADER_PATH, "/shader.vert"].concat()),
            SHADER_ENTRY_POINT.to_string(),
        )?,
        ash::ShaderInfo::new(
            PathBuf::from([SHADER_PATH, "/shader.frag"].concat()),
            SHADER_ENTRY_POINT.to_string(),
        )?,
        &[],
    )?;

    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher: RecommendedWatcher = Watcher::new_immediate(move |res| match res {
        Ok(event) => {
            tx.send(event).unwrap();
        }
        Err(e) => println!("watch error: {:?}", e),
    })?;

    watcher.watch(SHADER_PATH, RecursiveMode::Recursive)?;

    let mut ctrl_pressed = false;

    event_loop.run_return(|event, _, control_flow| {
        *control_flow = winit::event_loop::ControlFlow::Poll;
        match event {
            Event::NewEvents(_) => {
                if let Ok(rx_event) = rx.try_recv() {
                    if let notify::Event {
                        kind: EventKind::Modify(ModifyKind::Data(_)),
                        ..
                    } = rx_event
                    {
                        unsafe { pilka.device.device_wait_idle() }.unwrap();
                        for path in rx_event.paths {
                            if pilka.shader_set.contains_key(&path) {
                                pilka.rebuild_pipeline(pilka.shader_set[&path]).unwrap();
                            }
                        }
                    }
                }

                pilka.push_constant.time = time.elapsed().as_secs_f32();
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

                    pilka.push_constant.wh = [width as f32, height as f32];
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
                    if VirtualKeyCode::F10 == keycode {
                        let dump_folder = std::path::Path::new("shader_dump");
                        match std::fs::create_dir(dump_folder) {
                            Ok(_) => {}
                            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                            Err(e) => panic!("Failed to create folder: {}", e),
                        }
                        let dump_folder = dump_folder
                            .join(chrono::Local::now().format("%d.%m.%Y-%H:%M:%S").to_string());
                        match std::fs::create_dir(&dump_folder) {
                            Ok(_) => {}
                            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                            Err(e) => panic!("Failed to create folder: {}", e),
                        }
                        for path in pilka.shader_set.keys() {
                            let to = dump_folder.join(
                                path.strip_prefix(Path::new(SHADER_PATH).canonicalize().unwrap())
                                    .unwrap(),
                            );
                            if !to.exists() {
                                std::fs::create_dir_all(
                                    &to.parent().unwrap().canonicalize().unwrap(),
                                )
                                .unwrap();
                                std::fs::File::create(&to).unwrap();
                            }
                            std::fs::copy(path, &to).unwrap();
                            eprintln!("Saved: {}", &to.display());
                        }
                    }
                    if VirtualKeyCode::F11 == keycode {
                        let now = Instant::now();
                        let (width, height) = pilka.capture_image().unwrap();
                        eprintln!("Capture image: {:#?}", now.elapsed());

                        let frame = pilka.screenshot_ctx.data.clone();
                        std::thread::spawn(move || {
                            let now = Instant::now();
                            let screen: image::ImageBuffer<image::Bgra<u8>, _> =
                                image::ImageBuffer::from_raw(width, height, frame)
                                    .expect("ImageBuffer creation");
                            let screen_image = image::DynamicImage::ImageBgra8(screen).to_rgba8();
                            match std::fs::create_dir("screenshots") {
                                Ok(_) => {}
                                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                                Err(e) => panic!("Failed to create folder: {}", e),
                            }
                            screen_image
                                .save(std::path::Path::new("screenshots").join(format!(
                                    "screenshot-{}.jpg",
                                    chrono::Local::now().format("%d.%m.%Y-%H:%M:%S").to_string()
                                )))
                                .unwrap();
                            eprintln!("Encode image: {:#?}", now.elapsed());
                        });
                    }
                }
                WindowEvent::CursorMoved {
                    position: PhysicalPosition { x, y },
                    ..
                } => {
                    let vk::Extent2D { width, height } = pilka.extent;
                    let x = (x as f32 / width as f32 - 0.5) * 2.;
                    let y = -(y as f32 / height as f32 - 0.5) * 2.;
                    pilka.push_constant.mouse = [x, y];
                }
                _ => {}
            },

            Event::MainEventsCleared => {
                pilka.render();
            }
            Event::LoopDestroyed => unsafe { pilka.device.device_wait_idle() }.unwrap(),
            _ => {}
        }
    });

    println!("End from the loop. Bye bye~");

    Ok(())
}
