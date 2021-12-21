use color_eyre::*;
use notify::Config;
use notify::{event::EventKind, RecursiveMode, Watcher};
use pilka_types::{PushConstant, ShaderCreateInfo};
use std::ffi::CString;
use std::path::Path;
use std::time::Instant;
use winit::dpi::PhysicalSize;
use winit::{
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
    window::WindowBuilder,
};

fn main() -> Result<()> {
    env_logger::init();
    color_eyre::install()?;

    let event_loop = winit::event_loop::EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Pilka")
        .build(&event_loop)?;

    let PhysicalSize { width, height } = window.inner_size();
    let mut state = pilka_wgpu::WgpuRender::new(&window, PushConstant::size(), width, height)?;

    let shader_v = wgpu::util::make_spirv_raw(include_bytes!("shader.vert.spv"));
    let shader_f = wgpu::util::make_spirv_raw(include_bytes!("shader.frag.spv"));

    let entry_point = CString::new("main").unwrap();
    state.push_render_pipeline(
        ShaderCreateInfo::new(&shader_v, entry_point.as_c_str()),
        ShaderCreateInfo::new(&shader_f, entry_point.as_c_str()),
    )?;

    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = notify::recommended_watcher(move |res| match res {
        Ok(event) => tx.send(event).unwrap(),
        Err(e) => {
            eprintln!("Watch error: {:?}", e)
        }
    })?;
    watcher.watch(Path::new("./"), RecursiveMode::Recursive)?;
    watcher.configure(Config::PreciseEvents(true))?;
    watcher.configure(Config::NoticeEvents(false))?;

    let mut push_constant = PushConstant::default();
    let time = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::NewEvents(_) => {
                for rx_event in rx.try_iter() {
                    if let notify::Event {
                        kind:
                            EventKind::Access(notify::event::AccessKind::Close(
                                notify::event::AccessMode::Write,
                            )),
                        ..
                    } = rx_event
                    {
                        // match state.rebuild_pipelines(&rx_event.paths) {
                        //     Ok(_) => {}
                        //     Err(e) => eprintln!("{}", e),
                        // };
                    }
                }

                push_constant.time = time.elapsed().as_secs_f32();
            }
            // Event::DeviceEvent { device_id, event } => {}
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
                    push_constant.wh = [width as _, height as _];
                }
                WindowEvent::ScaleFactorChanged {
                    new_inner_size: PhysicalSize { width, height },
                    ..
                } => state.resize(*width, *height),
                _ => {}
            },
            Event::MainEventsCleared => {
                if state.render(push_constant).is_ok() {};
            }
            Event::LoopDestroyed => {}
            _ => {}
        }
    });
}

pub fn make_spirv(data: &[u8]) -> std::borrow::Cow<[u32]> {
    const MAGIC_NUMBER: u32 = 0x723_0203;

    assert_eq!(
        data.len() % std::mem::size_of::<u32>(),
        0,
        "data size is not a multiple of 4"
    );

    let words = if data.as_ptr().align_offset(std::mem::align_of::<u32>()) == 0 {
        let (pre, words, post) = unsafe { data.align_to::<u32>() };
        debug_assert!(pre.is_empty());
        debug_assert!(post.is_empty());
        std::borrow::Cow::from(words)
    } else {
        let mut words = vec![0u32; data.len() / std::mem::size_of::<u32>()];
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), words.as_mut_ptr() as *mut u8, data.len());
        }
        std::borrow::Cow::from(words)
    };
    assert_eq!(
        words[0], MAGIC_NUMBER,
        "wrong magic word {:x}. Make sure you are using a binary SPIRV file.",
        words[0]
    );
    words
}
