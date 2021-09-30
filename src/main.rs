mod default_shaders;
mod input;
mod recorder;
mod render_interface;
mod utils;

#[allow(dead_code)]
mod audio;

use std::{
    error::Error,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use pilka_ash::{vk, PilkaRender, ShaderInfo, SHADER_ENTRY_POINT, SHADER_PATH};
use recorder::{RecordEvent, RecordTimer};
use utils::{parse_args, print_help, save_screenshot, save_shaders, Args};

use eyre::*;
use notify::{
    event::{EventKind, ModifyKind},
    RecommendedWatcher, RecursiveMode, Watcher,
};
use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
    platform::unix::WindowBuilderExtUnix,
    window::WindowBuilder,
};

pub const SCREENSHOTS_FOLDER: &str = "screenshots";
pub const SHADER_DUMP_FOLDER: &str = "shader_dump";
pub const VIDEO_FOLDER: &str = "recordings";

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize error hook.
    color_eyre::install()?;

    let Args {
        record_time,
        inner_size,
    } = parse_args();

    // let mut audio_context = audio::AudioContext::new()?;

    let event_loop = winit::event_loop::EventLoop::new();

    let window = {
        let mut window_builder = WindowBuilder::new().with_title("Pilka");
        #[cfg(unix)]
        {
            window_builder =
                window_builder.with_resize_increments(LogicalSize::<u32>::from((8, 2)));
        }
        if let Some(size) = inner_size {
            window_builder = window_builder
                .with_resizable(false)
                .with_inner_size(LogicalSize::<u32>::from(size));
        } else {
            window_builder = window_builder.with_inner_size(LogicalSize::new(1280, 720));
        }
        window_builder.build(&event_loop)?
    };

    let mut pilka = PilkaRender::new(&window, PushConstant::size())?;

    let mut push_constant = PushConstant::default();

    let shader_dir = PathBuf::new().join(SHADER_PATH);

    if !shader_dir.is_dir() {
        default_shaders::create_default_shaders(&shader_dir)?;
    }

    pilka.push_compute_pipeline(
        ShaderInfo::new(shader_dir.join("shader.comp"), SHADER_ENTRY_POINT.into())?,
        &[],
    )?;

    pilka.push_render_pipeline(
        ShaderInfo::new(shader_dir.join("shader.vert"), SHADER_ENTRY_POINT.into())?,
        ShaderInfo::new(shader_dir.join("shader.frag"), SHADER_ENTRY_POINT.into())?,
        &[shader_dir.join("prelude.glsl")],
    )?;

    let (ffmpeg_version, has_ffmpeg) = recorder::ffmpeg_version()?;

    println!("Vendor name: {}", pilka.get_vendor_name());
    println!("Device name: {}", pilka.get_device_name()?);
    println!("Device type: {:?}", pilka.get_device_type());
    println!("Vulkan version: {}", pilka.get_vulkan_version_name()?);
    // println!("Audio host: {:?}", audio_context.host_id);
    // println!(
    //     "Sample rate: {}, channels: {}",
    //     audio_context.sample_rate, audio_context.num_channels
    // );
    println!("{}", ffmpeg_version);
    println!(
        "Default shader path:\n\t{}",
        shader_dir.canonicalize()?.display()
    );

    print_help();

    println!("// Set up our new world⏎ ");
    println!("// And let's begin the⏎ ");
    println!("\tSIMULATION⏎ \n");

    let (tx, rx) = crossbeam_channel::unbounded();

    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res| match res {
        Ok(event) => {
            tx.send(event).unwrap();
        }
        Err(e) => eprintln!("watch error: {:?}", e),
    })?;

    watcher.watch(Path::new(SHADER_PATH), RecursiveMode::Recursive)?;

    let mut video_recording = false;
    let (video_tx, video_rx) = crossbeam_channel::unbounded();
    std::thread::spawn(move || recorder::record_thread(video_rx));

    let mut input = input::Input::new();
    let mut pause = false;
    let mut timeline = Instant::now();
    let mut prev_time = timeline.elapsed();
    let mut backup_time = timeline.elapsed();
    let mut dt = Duration::from_secs_f32(1. / 60.);

    let (mut timer, start_event) = RecordTimer::new(record_time, video_tx.clone());
    if let Some(period) = record_time {
        push_constant.record_period = period.as_secs_f32();
    }

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
                        pilka.rebuild_pipelines(&rx_event.paths).unwrap();
                    }
                }

                pilka.paused = !pause;

                push_constant.time = if pause {
                    backup_time.as_secs_f32()
                } else {
                    if let Some(recording_time) = timer.counter {
                        recording_time.elapsed().as_secs_f32()
                    } else {
                        timeline.elapsed().as_secs_f32()
                    }
                };

                input.process_position(&mut push_constant);

                if !pause {
                    // let mut tmp_buf = [0f32; audio::FFT_SIZE];
                    // audio_context.get_fft(&mut tmp_buf);
                    // pilka.update_fft_texture(&tmp_buf).unwrap();

                    dt = timeline.elapsed().saturating_sub(prev_time);
                    push_constant.time_delta = dt.as_secs_f32();

                    prev_time = timeline.elapsed();
                }
                push_constant.wh = pilka.surface.resolution_slice(&pilka.device).unwrap();

                timer
                    .update(&mut video_recording, pilka.screenshot_dimentions())
                    .unwrap();
            }

            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(PhysicalSize { .. }) => {
                    let vk::Extent2D { width, height } =
                        pilka.surface.resolution(&pilka.device).unwrap();
                    let vk::Extent2D {
                        width: old_width,
                        height: old_height,
                    } = pilka.resolution;

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
                            print_help();
                        }

                        if VirtualKeyCode::F2 == keycode {
                            if !pause {
                                backup_time = timeline.elapsed();
                                pause = true;
                            } else {
                                timeline = Instant::now() - backup_time;
                                pause = false;
                            }
                        }

                        if VirtualKeyCode::F3 == keycode {
                            if !pause {
                                backup_time = timeline.elapsed();
                                pause = true;
                            }
                            backup_time = backup_time.saturating_sub(dt);
                        }

                        if VirtualKeyCode::F4 == keycode {
                            if !pause {
                                backup_time = timeline.elapsed();
                                pause = true;
                            }
                            backup_time += dt;
                        }

                        if VirtualKeyCode::F5 == keycode {
                            push_constant.pos = [0.; 3];
                            push_constant.time = 0.;
                            push_constant.frame = 0;
                            timeline = Instant::now();
                            backup_time = timeline.elapsed();
                        }

                        if VirtualKeyCode::F6 == keycode {
                            eprintln!("{}", push_constant);
                        }

                        if VirtualKeyCode::F10 == keycode {
                            save_shaders(&pilka).unwrap();
                        }

                        if VirtualKeyCode::F11 == keycode {
                            let now = Instant::now();
                            let (_, image_dimentions) = pilka.capture_frame().unwrap();
                            eprintln!("Capture image: {:#?}", now.elapsed());
                            let frame = &pilka.screenshot_ctx.data
                                [..image_dimentions.padded_bytes_per_row * image_dimentions.height];
                            save_screenshot(frame, image_dimentions);
                        }

                        if has_ffmpeg && VirtualKeyCode::F12 == keycode {
                            if video_recording {
                                video_tx.send(RecordEvent::Finish).unwrap()
                            } else {
                                let (_, image_dimentions) = pilka.capture_frame().unwrap();
                                video_tx.send(RecordEvent::Start(image_dimentions)).unwrap()
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
                        let vk::Extent2D { width, height } = pilka.resolution;
                        let x = (x as f32 / width as f32 - 0.5) * 2.;
                        let y = -(y as f32 / height as f32 - 0.5) * 2.;
                        push_constant.mouse = [x, y];
                    }
                }
                WindowEvent::MouseInput {
                    button: winit::event::MouseButton::Left,
                    state,
                    ..
                } => match state {
                    ElementState::Pressed => push_constant.mouse_pressed = true as _,
                    ElementState::Released => push_constant.mouse_pressed = false as _,
                },
                _ => {}
            },

            Event::MainEventsCleared => {
                pilka.render(push_constant.as_slice()).unwrap();
                start_event.try_send(()).ok();
                if video_recording {
                    let (frame, _image_dimentions) = pilka.capture_frame().unwrap();
                    video_tx.send(RecordEvent::Record(frame.to_vec())).unwrap()
                }
                push_constant.frame += 1;
            }
            Event::LoopDestroyed => {
                println!("// End from the loop. Bye bye~⏎ ");
                unsafe { pilka.device.device_wait_idle() }.unwrap();
            }
            _ => {}
        }
    })
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PushConstant {
    pub pos: [f32; 3],
    pub time: f32,
    pub wh: [f32; 2],
    pub mouse: [f32; 2],
    pub mouse_pressed: u32,
    pub frame: u32,
    pub time_delta: f32,
    pub record_period: f32,
}

impl PushConstant {
    fn as_slice(&self) -> &[u8] {
        unsafe { any_as_u8_slice(self) }
    }

    pub fn size() -> u32 {
        std::mem::size_of::<Self>() as _
    }
}

/// # Safety
/// Until you're using it on not ZST or DST it's fine
pub unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    std::slice::from_raw_parts((p as *const T) as *const _, std::mem::size_of::<T>())
}

impl Default for PushConstant {
    fn default() -> Self {
        Self {
            pos: [0.; 3],
            time: 0.,
            wh: [1920.0, 780.],
            mouse: [0.; 2],
            mouse_pressed: false as _,
            frame: 0,
            time_delta: 1. / 60.,
            record_period: 10.,
        }
    }
}

// TODO: Make proper ms -> sec converion
impl std::fmt::Display for PushConstant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "position:\t{:?}\n\
             time:\t\t{:.2}\n\
             time delta:\t{:.3} ms, fps: {:.2}\n\
             width, height:\t{:?}\nmouse:\t\t{:.2?}\n\
             frame:\t\t{}\nrecord_period:\t{}\n",
            self.pos,
            self.time,
            self.time_delta * 1000.,
            1. / self.time_delta,
            self.wh,
            self.mouse,
            self.frame,
            self.record_period
        )
    }
}
