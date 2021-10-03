use color_eyre::*;
use notify::Config;
use pollster::FutureExt as _;
use std::path::PathBuf;
use winit::{
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
    window::WindowBuilder,
};

const SHADER_PATH: &str = "shaders";

async fn run() -> Result<()> {
    env_logger::init();
    color_eyre::install()?;
    let event_loop = winit::event_loop::EventLoop::new();
    let window = {
        let window_builder = WindowBuilder::new().with_title("Pilka");
        window_builder.build(&event_loop)?
    };

    let mut state = pilka_wgpu::State::new(&window).await?;

    let shader_f = PathBuf::new().join("shaders").join("shader_f.wgsl");
    let shader_v = PathBuf::new().join("shaders").join("shader_v.wgsl");

    state.push_render_pipeline(shader_f, shader_v, &[])?;

    let (tx, rx) = std::sync::mpsc::channel();

    use notify::{
        event::{EventKind, ModifyKind},
        RecommendedWatcher, RecursiveMode, Watcher,
    };
    use std::path::Path;
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res| match res {
        Ok(event) => {
            tx.send(event).unwrap();
        }
        Err(e) => eprintln!("watch error: {:?}", e),
    })?;
    watcher.watch(Path::new(SHADER_PATH), RecursiveMode::Recursive)?;
    watcher.configure(Config::PreciseEvents(true))?;
    watcher.configure(Config::NoticeEvents(false))?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        match event {
            Event::NewEvents(_) => {
                for (i, rx_event) in rx.try_iter().enumerate(){
                    dbg!(i);

                // if let Ok(rx_event) = rx_eventt {
                    if let notify::Event {
                        kind: EventKind::Access(notify::event::AccessKind::Close(notify::event::AccessMode::Write)),
                        ..
                    } = rx_event
                    {
                    dbg!(&rx_event);
                        // let a: () = rx_event;
                        match state.rebuild_pipelines(&rx_event.paths){
                            Ok(_) => {},
                            Err(e) => eprintln!("{}",e),
                        };
                    }
                }
                // }
                }
            Event::MainEventsCleared => window.request_redraw(),
            Event::DeviceEvent {
                event: _,
                .. // We're not using device_id currently
            } => {
                // state.input(event);
            }
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == window.id() => {
                match event {
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                ..
                            },
                        ..
                    } => *control_flow = ControlFlow::Exit,
                    WindowEvent::Resized(physical_size) => {
                        state.resize(physical_size.width, physical_size.height);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        state.resize(new_inner_size.width, new_inner_size.height);
                    }
                    _ => {}
                }
            }
            Event::RedrawRequested(_) => {
                let _now = std::time::Instant::now();
                match state.render() {
                    Ok(_) => {}
                    // Reconfigure the surface if lost
                    // Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                    // The system is out of memory, we should probably quit
                    Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
                    // All other errors (Outdated, Timeout) should be resolved by the next frame
                    Err(e) => eprintln!("{:?}", e),
                }
            }
            _ => {}
        }
    });
}

fn main() -> Result<()> {
    run().block_on()
}
