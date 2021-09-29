use color_eyre::*;
use notify::Config;
use notify::{
    event::{EventKind, ModifyKind},
    RecommendedWatcher, RecursiveMode, Watcher,
};
use std::mem::size_of;
use std::path::{Path, PathBuf};
use winit::dpi::PhysicalSize;
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
    let window = WindowBuilder::new()
        .with_title("Pilka")
        .build(&event_loop)?;

    let mut state =
        pilka_wgpu::State::new(&window, std::mem::size_of::<PushConstant>() as _).await?;

    let shader_f = PathBuf::new().join("shaders").join("shader_f.wgsl");
    let shader_v = PathBuf::new().join("shaders").join("shader_v.wgsl");

    state.push_render_pipeline(shader_f, shader_v, &[])?;

    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = notify::recommended_watcher(move |res| match res {
        Ok(event) => tx.send(event).unwrap(),
        Err(e) => {
            eprintln!("Watch error: {:?}", e)
        }
    })?;
    watcher.watch(Path::new(SHADER_PATH), RecursiveMode::Recursive)?;
    watcher.configure(Config::PreciseEvents(true))?;
    watcher.configure(Config::NoticeEvents(false))?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::NewEvents(_) => {
                for (i, rx_event) in rx.try_iter().enumerate() {
                    if let notify::Event {
                        kind:
                            EventKind::Access(notify::event::AccessKind::Close(
                                notify::event::AccessMode::Write,
                            )),
                        ..
                    } = rx_event
                    {
                        match state.rebuild_pipelines(&rx_event.paths) {
                            Ok(_) => {}
                            Err(e) => eprintln!("{}", e),
                        };
                    }
                }
            }
            Event::DeviceEvent { device_id, event } => {}
            Event::WindowEvent { window_id, event } if window_id == window.id() => match event {
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
                WindowEvent::Resized(PhysicalSize { width, height }) => {
                    state.resize(width, height);
                }
                WindowEvent::ScaleFactorChanged {
                    new_inner_size: PhysicalSize { width, height },
                    ..
                } => state.resize(*width, *height),
                _ => {}
            },
            Event::MainEventsCleared => {
                match state.render(&[0; size_of::<PushConstant>()]) {
                    Ok(_) => {}
                    Err(_) => {}
                };
            }
            Event::LoopDestroyed => {}
            _ => {}
        }
    });
}

fn main() -> Result<()> {
    futures::executor::block_on(run())?;

    Ok(())
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
    unsafe fn as_slice(&self) -> &[u8] {
        any_as_u8_slice(self)
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
