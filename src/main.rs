mod app;
mod default_shaders;
mod input;
mod profiler_window;
mod recorder;
mod render_bundle;
mod shader_compiler;
mod utils;

#[allow(dead_code)]
mod audio;

use profiler_window::ProfilerWindow;

use std::{
    error::Error,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use pilka_types::{PushConstant, ShaderInfo};
use recorder::{RecordEvent, RecordTimer};
use render_bundle::Renderer;
use utils::{parse_args, print_help, save_screenshot, save_shaders, Args};

use eyre::*;
use notify::{RecursiveMode, Watcher};
use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use app::App;

pub const SCREENSHOTS_FOLDER: &str = "screenshots";
pub const SHADER_DUMP_FOLDER: &str = "shader_dump";
pub const VIDEO_FOLDER: &str = "recordings";
pub const SHADER_PATH: &str = "shaders";

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize error hook.
    color_eyre::install()?;
    env_logger::init();
    puffin::set_scopes_on(true);

    let Args {
        record_time,
        inner_size,
        wgsl_mode,
    } = parse_args();

    // let mut audio_context = audio::AudioContext::new()?;

    let event_loop = EventLoop::new();

    let main_window = {
        let mut window_builder = WindowBuilder::new().with_title("Pilka");
        #[cfg(unix)]
        {
            use winit::platform::unix::WindowBuilderExtUnix;
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

    let shader_dir = PathBuf::new().join(SHADER_PATH);
    if !shader_dir.is_dir() {
        default_shaders::create_default_shaders(&shader_dir, wgsl_mode)?;
    }

    let (folder_tx, folder_rx) = crossbeam_channel::unbounded();
    let mut watcher = notify::recommended_watcher(move |res| match res {
        Ok(event) => {
            folder_tx.send(event).unwrap();
        }
        Err(e) => eprintln!("watch error: {:?}", e),
    })?;
    watcher.watch(Path::new(SHADER_PATH), RecursiveMode::Recursive)?;

    let mut app = App::new(&main_window, folder_rx)?;

    let mut video_recording = false;
    let (video_tx, video_rx) = crossbeam_channel::unbounded();
    std::thread::spawn(move || recorder::record_thread(video_rx));

    let mut input = input::Input::new();
    let mut pause = false;

    let mut timeline = Instant::now();
    let mut prev_time = timeline.elapsed();
    let mut backup_time = timeline.elapsed();
    let mut dt = Duration::from_secs_f32(1. / 60.);

    let mut last_update_inst = Instant::now();

    let (mut timer, start_event) = RecordTimer::new(record_time, video_tx.clone());
    if let Some(period) = record_time {
        app.push_constant.record_period = period.as_secs_f32();
    }

    let mut profiler_window: Option<ProfilerWindow> = None;

    event_loop.run(move |event, event_loop, control_flow| {
        *control_flow = winit::event_loop::ControlFlow::Poll;

        if let Some(ref mut w) = profiler_window {
            w.handle_event(&event);
        }

        match event {
            Event::RedrawEventsCleared => {
                puffin::profile_scope!("Redraw Timeout");
                let target_frametime = Duration::from_secs_f64(1.0 / 60.0);
                let time_since_last_frame = last_update_inst.elapsed();
                if time_since_last_frame >= target_frametime {
                    main_window.request_redraw();
                    if let Some(ref mut w) = profiler_window {
                        w.request_redraw();
                    }
                    last_update_inst = Instant::now();
                } else {
                    *control_flow = ControlFlow::WaitUntil(
                        Instant::now() + target_frametime - time_since_last_frame,
                    );
                }
            }
            Event::NewEvents(_) => {
                puffin::profile_scope!("Frame setup");

                app.setup_frame();

                app.push_constant.time = if pause {
                    backup_time.as_secs_f32()
                } else if let Some(recording_time) = timer.counter {
                    recording_time.elapsed().as_secs_f32()
                } else {
                    timeline.elapsed().as_secs_f32()
                };

                app.push_constant.wh = main_window.inner_size().into();

                input.process_position(&mut app.push_constant);

                if !pause {
                    // let mut tmp_buf = [0f32; audio::FFT_SIZE];
                    // audio_context.get_fft(&mut tmp_buf);
                    // pilka.update_fft_texture(&tmp_buf).unwrap();

                    dt = timeline.elapsed().saturating_sub(prev_time);
                    app.push_constant.time_delta = dt.as_secs_f32();

                    prev_time = timeline.elapsed();
                }

                timer
                    .update(&mut video_recording, app.render.captured_frame_dimentions())
                    .unwrap();
            }
            Event::WindowEvent {
                event:
                    WindowEvent::Resized(size)
                    | WindowEvent::ScaleFactorChanged {
                        new_inner_size: &mut size,
                        ..
                    },
                window_id,
            } => {
                puffin::profile_scope!("Resize");
                let PhysicalSize { width, height } = size;

                if let Some(ref mut w) = profiler_window {
                    if w.id() == window_id {
                        w.resize();
                    }
                }

                if main_window.id() == window_id {
                    app.resize(width.max(1), height.max(1)).unwrap();
                }

                if video_recording {
                    println!("Stop recording. Resolution has been changed.",);
                    video_recording = false;
                    video_tx.send(RecordEvent::Finish).unwrap();
                }
            }

            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
            } => {
                let sec_id = profiler_window.as_ref().unwrap().id();
                if window_id == sec_id {
                    profiler_window = None;
                }
            }

            Event::WindowEvent { event, window_id } if main_window.id() == window_id => match event
            {
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            virtual_keycode: Some(keycode),
                            state,
                            ..
                        },
                    ..
                } => {
                    puffin::profile_scope!("Keyboard Events");

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
                            app.push_constant.pos = [0.; 3];
                            app.push_constant.time = 0.;
                            app.push_constant.frame = 0;
                            timeline = Instant::now();
                            backup_time = timeline.elapsed();
                        }

                        if VirtualKeyCode::F6 == keycode {
                            eprintln!("{}", app.push_constant);
                        }

                        if VirtualKeyCode::F7 == keycode {
                            if profiler_window.is_some() {
                                profiler_window = None;
                            } else {
                                profiler_window = Some(ProfilerWindow::new(event_loop).unwrap());
                            }
                        }

                        if VirtualKeyCode::F8 == keycode {
                            app.render.switch(&main_window, &mut app.compiler).unwrap();
                        }

                        if VirtualKeyCode::F10 == keycode {
                            save_shaders(&app.render.shader_list()).unwrap();
                        }

                        if VirtualKeyCode::F11 == keycode {
                            let now = Instant::now();
                            let (frame, image_dimentions) = app.render.capture_frame().unwrap();
                            eprintln!("Capture image: {:#.2?}", now.elapsed());
                            save_screenshot(frame, image_dimentions);
                        }

                        if app.has_ffmpeg && VirtualKeyCode::F12 == keycode {
                            if video_recording {
                                video_tx.send(RecordEvent::Finish).unwrap()
                            } else {
                                let (_, image_dimentions) = app.render.capture_frame().unwrap();
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
                        let PhysicalSize { width, height } = main_window.inner_size();
                        let x = (x as f32 / width as f32 - 0.5) * 2.;
                        let y = -(y as f32 / height as f32 - 0.5) * 2.;
                        app.push_constant.mouse = [x, y];
                    }
                }
                WindowEvent::MouseInput {
                    button: winit::event::MouseButton::Left,
                    state,
                    ..
                } => match state {
                    ElementState::Pressed => app.push_constant.mouse_pressed = true as _,
                    ElementState::Released => app.push_constant.mouse_pressed = false as _,
                },
                _ => {}
            },
            Event::RedrawRequested(_) => {
                puffin::GlobalProfiler::lock().new_frame();
                puffin::profile_scope!("Rendering");

                if let Some(w) = &mut profiler_window {
                    w.render(&timeline);
                }

                app.render();

                start_event.try_send(()).ok();
                if video_recording {
                    let (frame, _image_dimentions) = app.render.capture_frame().unwrap();
                    video_tx.send(RecordEvent::Record(frame)).unwrap()
                }
            }
            Event::LoopDestroyed => {
                app.shut_down();
                println!("// End from the loop. Bye bye~⏎ ");
            }
            _ => {}
        }
    });
}
