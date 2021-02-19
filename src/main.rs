use pilka_lib::*;

#[cfg(debug_assertions)]
#[allow(unused_imports)]
#[allow(clippy::single_component_path_imports)]
use pilka_dyn;

mod input;
mod recorder;

use ash::{version::DeviceV1_0, vk, SHADER_ENTRY_POINT, SHADER_PATH};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eyre::*;
use notify::{
    event::{EventKind, ModifyKind},
    RecommendedWatcher, RecursiveMode, Watcher,
};
use recorder::RecordEvent;
use std::{
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
    sync::mpsc::Sender,
    time::Instant,
};
use winit::{
    dpi::PhysicalPosition,
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
};

fn main() -> Result<()> {
    // Initialize error hook.
    color_eyre::install()?;

    let host = cpal::default_host();

    let device = host
        .default_input_device()
        .context("failed to find input device")?;

    let config = device.default_input_config()?;
    let sample_rate = config.sample_rate().0;
    let num_channels = config.channels();

    let err_fn = move |err| {
        eprintln!("an error occured on stream: {}", err);
    };

    let (audio_tx, audio_rx) = std::sync::mpsc::channel();

    fn write_input_data<T>(input: &[T], tx: &Sender<f32>)
    where
        T: cpal::Sample,
    {
        let sample = input
            .iter()
            .map(|s| cpal::Sample::from(s))
            .map(|s: T| s.to_f32())
            .sum::<f32>()
            / input.len() as f32;

        tx.send(sample.max(-1.0).min(1.0)).ok();
    }
    let stream = device.build_input_stream(
        &config.into(),
        move |data, _: &_| write_input_data::<f32>(data, &audio_tx),
        err_fn,
    )?;

    stream.play()?;

    let mut input = input::Input::new();
    let mut pause = false;
    let mut time = Instant::now();
    let mut backup_time = time.elapsed();
    let dt = 1. / 60.;

    let event_loop = winit::event_loop::EventLoop::new();

    let window = winit::window::WindowBuilder::new()
        .with_title("Pilka")
        .with_inner_size(winit::dpi::LogicalSize::new(
            f64::from(1280),
            f64::from(720),
        ))
        .build(&event_loop)?;

    let mut pilka = PilkaRender::new(&window).unwrap();

    let shader_dir = PathBuf::new().join(SHADER_PATH);
    pilka.push_shader_module(
        ash::ShaderInfo::new(
            shader_dir.join("shader.vert"),
            SHADER_ENTRY_POINT.to_string(),
        )?,
        ash::ShaderInfo::new(
            shader_dir.join("shader.frag"),
            SHADER_ENTRY_POINT.to_string(),
        )?,
        &[shader_dir.join("prelude.glsl")],
    )?;

    let mut has_ffmpeg = false;
    let ffmpeg_version = match recorder::ffmpeg_version() {
        Ok(output) => {
            has_ffmpeg = true;
            String::from_utf8(output.stdout)?
                .lines()
                .next()
                .unwrap()
                .to_string()
        }
        Err(e) => {
            has_ffmpeg = false;
            e.to_string()
        }
    };

    println!("Vendor name: {}", pilka.get_vendor_name());
    println!("Device name: {}", pilka.get_device_name()?);
    println!("Device type: {:?}", pilka.get_device_type());
    println!("Vulkan version: {}", pilka.get_vulkan_version_name()?);
    println!("Audio host: {:?}", host.id());
    println!("Sample rate: {}, channels: {}", sample_rate, num_channels);
    println!("{}", ffmpeg_version);
    println!(
        "Default shader path:\n\t{}",
        shader_dir.canonicalize()?.display()
    );

    println!("\n- `F1`:   Toggles play/pause");
    println!("- `F2`:   Pauses and steps back one frame");
    println!("- `F3`:   Pauses and steps forward one frame");
    println!("- `F4`:   Restarts playback at frame 0 (`Time` and `Pos` = 0)");
    println!("- `F5`:   Print parameters");
    println!("- `F10`:  Save shaders");
    println!("- `F11`:  Take Screenshot");
    println!("- `F12`:  Start/Stop record video");
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

    let mut video_recording = false;
    let (video_tx, video_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || recorder::record_thread(video_rx));

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
                    if let Ok(spectrum) = audio_rx.try_recv() {
                        pilka.push_constant.spectrum = spectrum;
                    }

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
                pilka.push_constant.wh = pilka.surface.resolution_slice(&pilka.device).unwrap();
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

                    if video_recording {
                        println!(
                            "Stop recording. Resolution has been changed {}×{} => {}×{}.",
                            width, height, old_width, old_height
                        );
                        video_recording = false;
                        video_tx.send(RecordEvent::Finish).unwrap();
                    }
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
                                .join(chrono::Local::now().format("%d-%m-%Y-%H-%M-%S").to_string());
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
                            let (width, height) = pilka.capture_frame().unwrap();
                            eprintln!("Capture image: {:#?}", now.elapsed());

                            let frame = pilka.screenshot_ctx.data;
                            std::thread::spawn(move || {
                                let now = Instant::now();
                                let screenshots_folder = Path::new("screenshots");
                                match std::fs::create_dir(screenshots_folder) {
                                    Ok(_) => {}
                                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                                    Err(e) => panic!("Failed to create folder: {}", e),
                                }
                                let path = screenshots_folder.join(format!(
                                    "screenshot-{}.jpg",
                                    chrono::Local::now().format("%d-%m-%Y-%H-%M-%S").to_string()
                                ));
                                let file = File::create(path).unwrap();
                                let w = BufWriter::new(file);
                                let mut encoder = png::Encoder::new(w, width, height);
                                encoder.set_color(png::ColorType::RGBA);
                                encoder.set_depth(png::BitDepth::Eight);
                                let mut writer = encoder.write_header().unwrap();
                                writer.write_image_data(&frame).unwrap();
                                eprintln!("Encode image: {:#?}", now.elapsed());
                            });
                        }

                        if has_ffmpeg && VirtualKeyCode::F12 == keycode {
                            if video_recording {
                                video_tx.send(RecordEvent::Finish).unwrap()
                            } else {
                                let [w, h] = pilka.surface.resolution_slice(&pilka.device).unwrap();
                                video_tx
                                    .send(RecordEvent::Start(w as u32, h as u32))
                                    .unwrap()
                            }
                            video_recording = !video_recording;
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
                if video_recording {
                    let frame = pilka.screenshot_ctx.data.to_vec();
                    println!("len: {}, padding: {}", frame.len(), frame.len() % 4);
                    video_tx.send(RecordEvent::Record(frame)).unwrap()
                }
            }
            Event::LoopDestroyed => {
                println!("// End from the loop. Bye bye~⏎ ");
                unsafe { pilka.device.device_wait_idle() }.unwrap();
            }
            _ => {}
        }
    });
}
