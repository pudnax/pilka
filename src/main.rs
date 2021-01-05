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

    let mut time = Instant::now();
    let mut backup_time = time.elapsed();
    let dt = 1. / 60.;
    let mut input = Input::new();
    let mut pause = false;

    let event_loop = winit::event_loop::EventLoop::new();

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

    println!("Device name: {}", pilka.get_device_name()?);
    println!("Device type: {:?}", pilka.get_device_type());

    println!("- `F1`:   Toggles play/pause");
    println!("- `F2`:   Pauses and steps back one frame");
    println!("- `F3`:   Pauses and steps forward one frame");
    println!("- `F4`:   Restarts playback at frame 0 (`Time` and `Pos` = 0)");
    println!("- `F5`:   Print parameters");
    println!("- `F10`:  Save shaders");
    println!("- `F11`:  Take Screenshot");
    // println!("- `F12`:  Start/Stop record video");
    println!("- `ESC`:  Exit the application");
    println!("- `Arrows`: Change `Pos`\n");
    println!("// Set up our new world⏎ ");
    println!("// And let's begin the⏎ ");
    println!("\tSIMULATION⏎ \n");

    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher: RecommendedWatcher = Watcher::new_immediate(move |res| match res {
        Ok(event) => {
            tx.send(event).unwrap();
        }
        Err(e) => println!("watch error: {:?}", e),
    })?;

    watcher.watch(SHADER_PATH, RecursiveMode::Recursive)?;

    event_loop.run(move |event, _, control_flow| {
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

                pilka.push_constant.time = if pause {
                    backup_time.as_secs_f32()
                } else {
                    time.elapsed().as_secs_f32()
                };

                if !pause {
                    let dx = 0.01;
                    if input.left_pressed {
                        pilka.push_constant.pos[0] -= dx;
                    }
                    if input.right_pressed {
                        pilka.push_constant.pos[0] += dx;
                    }
                    if input.down_pressed {
                        pilka.push_constant.pos[1] -= dx;
                    }
                    if input.up_pressed {
                        pilka.push_constant.pos[1] += dx;
                    }
                    if input.slash_pressed {
                        pilka.push_constant.pos[2] -= dx;
                    }
                    if input.right_shift_pressed {
                        pilka.push_constant.pos[2] += dx;
                    }
                }
            }

            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
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
                            state,
                            ..
                        },
                    ..
                } => {
                    input.update(&keycode, &state);

                    if VirtualKeyCode::Escape == keycode {
                        *control_flow = ControlFlow::Exit;
                    }
                    if ElementState::Pressed == state {
                        if VirtualKeyCode::F1 == keycode {
                            if !pause {
                                backup_time = time.elapsed();
                                pause = true;
                            } else {
                                time = Instant::now() - backup_time;
                                pause = false;
                            }
                        }
                        if VirtualKeyCode::F2 == keycode {
                            if !pause {
                                backup_time = time.elapsed();
                                pause = true;
                            }
                            backup_time = backup_time
                                .checked_sub(std::time::Duration::from_secs_f32(dt))
                                .unwrap_or_else(Default::default);
                        }
                        if VirtualKeyCode::F3 == keycode {
                            if !pause {
                                backup_time = time.elapsed();
                                pause = true;
                            }
                            backup_time += std::time::Duration::from_secs_f32(dt);
                        }
                        if VirtualKeyCode::F4 == keycode {
                            pilka.push_constant.pos = [0.; 3];
                            pilka.push_constant.time = 0.;
                            time = Instant::now();
                            backup_time = time.elapsed();
                        }
                        if VirtualKeyCode::F5 == keycode {
                            eprintln!("{}", pilka.push_constant);
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
                                    path.strip_prefix(
                                        Path::new(SHADER_PATH).canonicalize().unwrap(),
                                    )
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
                                let screen_image =
                                    image::DynamicImage::ImageBgra8(screen).to_rgba8();
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
                }

                WindowEvent::CursorMoved {
                    position: PhysicalPosition { x, y },
                    ..
                } => {
                    if !pause {
                        let vk::Extent2D { width, height } = pilka.extent;
                        let x = (x as f32 / width as f32 - 0.5) * 2.;
                        let y = -(y as f32 / height as f32 - 0.5) * 2.;
                        pilka.push_constant.mouse = [x, y];
                    }
                }
                _ => {}
            },

            Event::MainEventsCleared => {
                pilka.render();
            }
            Event::LoopDestroyed => {
                println!("// End from the loop. Bye bye~⏎ ");
                unsafe { pilka.device.device_wait_idle() }.unwrap();
            }
            _ => {}
        }
    });
}

#[derive(Debug, Default)]
pub struct Input {
    pub up_pressed: bool,
    pub down_pressed: bool,
    pub right_pressed: bool,
    pub left_pressed: bool,
    pub slash_pressed: bool,
    pub right_shift_pressed: bool,
    pub enter_pressed: bool,
    pub space_pressed: bool,
}

impl Input {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn update(&mut self, key: &VirtualKeyCode, state: &ElementState) -> bool {
        let pressed = state == &ElementState::Pressed;
        match key {
            VirtualKeyCode::Up => {
                self.up_pressed = pressed;
                true
            }
            VirtualKeyCode::Down => {
                self.down_pressed = pressed;
                true
            }
            VirtualKeyCode::Left => {
                self.left_pressed = pressed;
                true
            }
            VirtualKeyCode::Right => {
                self.right_pressed = pressed;
                true
            }
            VirtualKeyCode::Slash => {
                self.slash_pressed = pressed;
                true
            }
            VirtualKeyCode::RShift => {
                self.right_shift_pressed = pressed;
                true
            }
            VirtualKeyCode::Return => {
                self.enter_pressed = pressed;
                true
            }
            VirtualKeyCode::Space => {
                self.space_pressed = pressed;
                true
            }
            _ => false,
        }
    }
}
