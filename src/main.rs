use core::panic;
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{bail, Result};
use ash::{khr, vk};
use either::Either;
use pilka::{
    align_to, default_shaders, dispatch_optimal, parse_args, print_help, save_shaders, Args,
    ComputeHandle, Device, FragmentOutputDesc, FragmentShaderDesc, Input, Instance, PipelineArena,
    PushConstant, Recorder, RenderHandle, ShaderKind, ShaderSource, Surface, Swapchain,
    TextureArena, UserEvent, VertexInputDesc, VertexShaderDesc, Watcher, COLOR_SUBRESOURCE_MASK,
    PREV_FRAME_IMAGE_IDX, SCREENSIZED_IMAGE_INDICES, SHADER_FOLDER,
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, KeyEvent, MouseButton, StartCause, WindowEvent},
    event_loop::EventLoopProxy,
    keyboard::{Key, NamedKey},
    window::{Window, WindowAttributes},
};

pub const UPDATES_PER_SECOND: u32 = 60;
pub const FIXED_TIME_STEP: f64 = 1. / UPDATES_PER_SECOND as f64;
pub const MAX_FRAME_TIME: f64 = 15. * FIXED_TIME_STEP; // 0.25;

#[allow(dead_code)]
struct AppInit {
    window: Window,
    input: Input,

    pause: bool,
    timeline: Instant,
    backup_time: Duration,
    frame_instant: Instant,
    frame_accumulated_time: f64,

    texture_arena: TextureArena,

    file_watcher: Watcher,
    recorder: Recorder,
    video_recording: bool,
    record_time: Option<Duration>,

    push_constant: PushConstant,
    render_pipeline: RenderHandle,
    compute_pipeline: ComputeHandle,
    pipeline_arena: PipelineArena,

    queue: vk::Queue,
    transfer_queue: vk::Queue,

    swapchain: Swapchain,
    surface: Surface,
    device: Arc<Device>,
    instance: Instance,
}

impl AppInit {
    fn new(
        event_loop: &winit::event_loop::ActiveEventLoop,
        proxy: EventLoopProxy<UserEvent>,
        window_attributes: WindowAttributes,
        record_time: Option<Duration>,
    ) -> Result<Self> {
        let window = event_loop.create_window(window_attributes)?;
        let watcher = Watcher::new(proxy)?;
        let mut recorder = Recorder::new();

        let instance = Instance::new(Some(&window))?;
        let surface = instance.create_surface(&window)?;
        let (device, queue, transfer_queue) = instance.create_device_and_queues(&surface)?;
        let device = Arc::new(device);

        let swapchain_loader = khr::swapchain::Device::new(&instance, &device);
        let swapchain = Swapchain::new(&device, &surface, swapchain_loader)?;

        let mut pipeline_arena = PipelineArena::new(&device, watcher.clone())?;

        let extent = swapchain.extent();
        let video_recording = record_time.is_some();
        let push_constant = PushConstant {
            wh: [extent.width as f32, extent.height as f32],
            record_time: record_time.map(|t| t.as_secs_f32()).unwrap_or(10.),
            ..Default::default()
        };

        let texture_arena = TextureArena::new(&device, &queue, swapchain.extent())?;

        let vertex_shader_desc = VertexShaderDesc {
            shader_path: "shaders/shader.vert".into(),
            ..Default::default()
        };
        let fragment_shader_desc = FragmentShaderDesc {
            shader_path: "shaders/shader.frag".into(),
        };
        let fragment_output_desc = FragmentOutputDesc {
            surface_format: swapchain.format(),
            ..Default::default()
        };
        let push_constant_range = vk::PushConstantRange::default()
            .size(size_of::<PushConstant>() as _)
            .stage_flags(
                vk::ShaderStageFlags::VERTEX
                    | vk::ShaderStageFlags::FRAGMENT
                    | vk::ShaderStageFlags::COMPUTE,
            );
        let render_pipeline = pipeline_arena.create_render_pipeline(
            &VertexInputDesc::default(),
            &vertex_shader_desc,
            &fragment_shader_desc,
            &fragment_output_desc,
            &[push_constant_range],
            &[texture_arena.images_set_layout],
        )?;

        let compute_pipeline = pipeline_arena.create_compute_pipeline(
            "shaders/shader.comp",
            &[push_constant_range],
            &[texture_arena.images_set_layout],
        )?;

        if record_time.is_some() {
            let mut image_dimensions = swapchain.image_dimensions;
            image_dimensions.width = align_to(image_dimensions.width, 2);
            image_dimensions.height = align_to(image_dimensions.height, 2);
            recorder.start(image_dimensions);
        }

        Ok(Self {
            window,
            input: Input::default(),

            pause: false,
            timeline: Instant::now(),
            backup_time: Duration::from_secs(0),
            frame_instant: Instant::now(),
            frame_accumulated_time: 0.,

            texture_arena,

            file_watcher: watcher,
            video_recording,
            record_time,
            recorder,

            push_constant,
            render_pipeline,
            compute_pipeline,
            pipeline_arena,

            queue,
            transfer_queue,

            surface,
            swapchain,
            device,
            instance,
        })
    }

    fn update(&mut self) {
        self.input.process_position(&mut self.push_constant);
    }

    fn reload_shaders(&mut self, path: PathBuf) -> Result<()> {
        if let Some(frame) = self.swapchain.get_current_frame() {
            let fences = std::slice::from_ref(&frame.present_finished);
            unsafe { self.device.wait_for_fences(fences, true, u64::MAX)? };
        }

        let resolved = {
            let mapping = self.file_watcher.include_mapping.lock();
            mapping[&path].clone()
        };

        for ShaderSource { path, kind } in resolved {
            let handles = &self.pipeline_arena.path_mapping[&path];
            for handle in handles {
                let compiler = &self.pipeline_arena.shader_compiler;
                match handle {
                    Either::Left(handle) => {
                        let pipeline = &mut self.pipeline_arena.render.pipelines[*handle];
                        match kind {
                            ShaderKind::Vertex => pipeline.reload_vertex_lib(compiler, &path),
                            ShaderKind::Fragment => pipeline.reload_fragment_lib(compiler, &path),
                            ShaderKind::Compute => {
                                bail!("Supplied compute shader into the render pipeline!")
                            }
                        }?;
                        pipeline.link()?;
                    }
                    Either::Right(handle) => {
                        let pipeline = &mut self.pipeline_arena.compute.pipelines[*handle];
                        pipeline.reload(compiler)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn recreate_swapchain(&mut self) -> Result<()> {
        if let Some(frame) = self.swapchain.get_current_frame() {
            let fences = std::slice::from_ref(&frame.present_finished);
            unsafe { self.device.wait_for_fences(fences, true, u64::MAX)? };
        }

        self.swapchain
            .recreate(&self.device, &self.surface)
            .expect("Failed to recreate swapchain");
        let extent = self.swapchain.extent();
        self.push_constant.wh = [extent.width as f32, extent.height as f32];

        for i in SCREENSIZED_IMAGE_INDICES {
            self.texture_arena.image_infos[i].extent = vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            };
        }
        self.texture_arena
            .update_images(&SCREENSIZED_IMAGE_INDICES)?;

        Ok(())
    }
}

impl ApplicationHandler<UserEvent> for AppInit {
    fn new_events(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        cause: winit::event::StartCause,
    ) {
        self.push_constant.time = if !self.pause {
            self.timeline.elapsed().as_secs_f32()
        } else {
            self.backup_time.as_secs_f32()
        };
        if let StartCause::WaitCancelled { .. } = cause {
            let new_instant = Instant::now();
            let frame_time = new_instant
                .duration_since(self.frame_instant)
                .as_secs_f64()
                .min(MAX_FRAME_TIME);
            self.frame_instant = new_instant;
            self.push_constant.time_delta = frame_time as _;

            self.frame_accumulated_time += frame_time;
            while self.frame_accumulated_time >= FIXED_TIME_STEP {
                self.update();

                self.frame_accumulated_time -= FIXED_TIME_STEP;
            }
        }

        if let Some(limit) = self.record_time {
            if self.timeline.elapsed() >= limit && self.recorder.is_active() {
                self.recorder.finish();
                event_loop.exit();
            }
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::Key(key_event) = event {
            self.input.update_device_input(key_event);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested
            | WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => event_loop.exit(),

            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(key),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                let dt = Duration::from_secs_f32(1. / 60.);
                match key {
                    NamedKey::F1 => print_help(),
                    NamedKey::F2 => {
                        if !self.pause {
                            self.backup_time = self.timeline.elapsed();
                        } else {
                            self.timeline = Instant::now() - self.backup_time;
                        }
                        self.pause = !self.pause;
                    }
                    NamedKey::F3 => {
                        if !self.pause {
                            self.backup_time = self.timeline.elapsed();
                            self.pause = true;
                        }
                        self.backup_time = self.backup_time.saturating_sub(dt);
                    }
                    NamedKey::F4 => {
                        if !self.pause {
                            self.backup_time = self.timeline.elapsed();
                            self.pause = true;
                        }
                        self.backup_time += dt;
                    }
                    NamedKey::F5 => {
                        self.push_constant.pos = [0.; 3];
                        self.push_constant.time = 0.;
                        self.push_constant.frame = 0;
                        self.timeline = Instant::now();
                        self.backup_time = self.timeline.elapsed();
                    }
                    NamedKey::F6 => {
                        println!("{}", self.push_constant);
                    }
                    NamedKey::F10 => {
                        let _ = save_shaders(SHADER_FOLDER).map_err(|err| log::error!("{err}"));
                    }
                    NamedKey::F11 => {
                        let _ = self
                            .device
                            .capture_image_data(
                                &self.queue,
                                self.swapchain.get_current_image(),
                                self.swapchain.extent(),
                                |tex| self.recorder.screenshot(tex),
                            )
                            .map_err(|err| log::error!("{err}"));
                    }
                    NamedKey::F12 => {
                        if !self.video_recording {
                            let mut image_dimensions = self.swapchain.image_dimensions;
                            image_dimensions.width = align_to(image_dimensions.width, 2);
                            image_dimensions.height = align_to(image_dimensions.height, 2);
                            self.recorder.start(image_dimensions);
                        } else {
                            self.recorder.finish();
                        }
                        self.video_recording = !self.video_recording;
                    }
                    _ => {}
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.input.update_window_input(&event);
            }

            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.push_constant.mouse_pressed = (ElementState::Pressed == state) as u32;
            }
            WindowEvent::CursorMoved {
                position: PhysicalPosition { x, y },
                ..
            } => {
                if !self.pause {
                    let PhysicalSize { width, height } = self.window.inner_size();
                    let x = (x as f32 / width as f32 - 0.5) * 2.;
                    let y = -(y as f32 / height as f32 - 0.5) * 2.;
                    self.push_constant.mouse = [x, y];
                }
            }
            WindowEvent::RedrawRequested => {
                let mut frame = match self.swapchain.acquire_next_image() {
                    Ok(frame) => frame,
                    Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                        let _ = self.recreate_swapchain().map_err(|err| log::warn!("{err}"));
                        self.window.request_redraw();
                        return;
                    }
                    Err(e) => panic!("error: {e}\n"),
                };

                let stages = vk::ShaderStageFlags::VERTEX
                    | vk::ShaderStageFlags::FRAGMENT
                    | vk::ShaderStageFlags::COMPUTE;
                let pipeline = self.pipeline_arena.get_pipeline(self.compute_pipeline);
                frame.push_constant(pipeline.layout, stages, &[self.push_constant]);
                frame.bind_descriptor_sets(
                    vk::PipelineBindPoint::COMPUTE,
                    pipeline.layout,
                    &[self.texture_arena.images_set],
                );
                frame.bind_pipeline(vk::PipelineBindPoint::COMPUTE, &pipeline.pipeline);
                const SUBGROUP_SIZE: u32 = 16;
                let extent = self.swapchain.extent();
                frame.dispatch(
                    dispatch_optimal(extent.width, SUBGROUP_SIZE),
                    dispatch_optimal(extent.height, SUBGROUP_SIZE),
                    1,
                );

                unsafe {
                    let image_barrier = vk::ImageMemoryBarrier2::default()
                        .subresource_range(COLOR_SUBRESOURCE_MASK)
                        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                        .dst_stage_mask(vk::PipelineStageFlags2::ALL_GRAPHICS)
                        .image(self.texture_arena.images[PREV_FRAME_IMAGE_IDX].image);
                    self.device.cmd_pipeline_barrier2(
                        *frame.command_buffer(),
                        &vk::DependencyInfo::default()
                            .image_memory_barriers(std::slice::from_ref(&image_barrier)),
                    )
                };

                frame.begin_rendering(
                    self.swapchain.get_current_image_view(),
                    [0., 0.025, 0.025, 1.0],
                );
                let pipeline = self.pipeline_arena.get_pipeline(self.render_pipeline);
                frame.push_constant(pipeline.layout, stages, &[self.push_constant]);
                frame.bind_descriptor_sets(
                    vk::PipelineBindPoint::GRAPHICS,
                    pipeline.layout,
                    &[self.texture_arena.images_set],
                );
                frame.bind_pipeline(vk::PipelineBindPoint::GRAPHICS, &pipeline.pipeline);

                frame.draw(3, 0, 1, 0);
                frame.end_rendering();

                self.device.blit_image(
                    frame.command_buffer(),
                    self.swapchain.get_current_image(),
                    self.swapchain.extent(),
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    &self.texture_arena.images[PREV_FRAME_IMAGE_IDX].image,
                    self.swapchain.extent(),
                    vk::ImageLayout::UNDEFINED,
                );

                match self.swapchain.submit_image(&self.queue, frame) {
                    Ok(_) => {}
                    Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                        let _ = self.recreate_swapchain().map_err(|err| log::warn!("{err}"));
                    }
                    Err(e) => panic!("error: {e}\n"),
                }

                self.window.request_redraw();

                if self.video_recording && self.recorder.ffmpeg_installed() {
                    let res = self.device.capture_image_data(
                        &self.queue,
                        self.swapchain.get_current_image(),
                        self.swapchain.extent(),
                        |tex| self.recorder.record(tex),
                    );
                    if let Err(err) = res {
                        log::error!("{err}");
                        self.video_recording = false;
                    }
                }

                self.push_constant.frame = self.push_constant.frame.saturating_add(1);
            }
            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Glsl { path } => {
                match self.reload_shaders(path) {
                    Err(err) => eprintln!("{err}"),
                    Ok(()) => {
                        const ESC: &str = "\x1B[";
                        const RESET: &str = "\x1B[0m";
                        eprint!("\r{}42m{}K{}\r", ESC, ESC, RESET);
                        std::io::stdout().flush().unwrap();
                        std::thread::spawn(|| {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            eprint!("\r{}40m{}K{}\r", ESC, ESC, RESET);
                            std::io::stdout().flush().unwrap();
                        });
                    }
                };
            }
        }
    }

    fn exiting(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        self.recorder.close_thread();
        if let Some(handle) = self.recorder.thread_handle.take() {
            let _ = handle.join();
        }
        let _ = unsafe { self.device.device_wait_idle() };
        println!("// End from the loop. Bye bye~⏎ ");
    }

    fn resumed(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        panic!("On native platforms `resumed` can be called only once.")
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let event_loop = winit::event_loop::EventLoop::with_user_event().build()?;

    let Args {
        record_time,
        inner_size,
    } = parse_args()?;

    let shader_dir = PathBuf::new().join(SHADER_FOLDER);
    if !shader_dir.is_dir() {
        default_shaders::create_default_shaders(&shader_dir)?;
    }

    let mut app = App::new(event_loop.create_proxy(), record_time, inner_size);
    event_loop.run_app(&mut app)?;
    Ok(())
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    record_time: Option<Duration>,
    initial_window_size: Option<(u32, u32)>,
    inner: AppEnum,
}

impl App {
    fn new(
        proxy: EventLoopProxy<UserEvent>,
        record_time: Option<Duration>,
        inner_size: Option<(u32, u32)>,
    ) -> Self {
        Self {
            proxy,
            record_time,
            initial_window_size: inner_size,
            inner: AppEnum::Uninitialized,
        }
    }
}

#[derive(Default)]
enum AppEnum {
    #[default]
    Uninitialized,
    Init(AppInit),
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let mut window_attributes = WindowAttributes::default().with_title("myndgera");
        if let Some(size) = self.initial_window_size {
            window_attributes = window_attributes
                .with_resizable(false)
                .with_inner_size(LogicalSize::<u32>::from(size));
        }
        match self.inner {
            AppEnum::Uninitialized => {
                let app = AppInit::new(
                    event_loop,
                    self.proxy.clone(),
                    window_attributes,
                    self.record_time,
                )
                .expect("Failed to create application");

                println!("{}", app.device.get_info());
                println!("{}", app.recorder.ffmpeg_version);
                println!(
                    "Default shader path:\n\t{}",
                    Path::new(SHADER_FOLDER).canonicalize().unwrap().display()
                );
                print_help();

                println!("// Set up our new world⏎ ");
                println!("// And let's begin the⏎ ");
                println!("\tSIMULATION⏎ \n");

                self.inner = AppEnum::Init(app);
            }
            AppEnum::Init(_) => {}
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        if let AppEnum::Init(app) = &mut self.inner {
            app.window_event(event_loop, window_id, event);
        }
    }

    fn new_events(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        cause: winit::event::StartCause,
    ) {
        if let AppEnum::Init(app) = &mut self.inner {
            app.new_events(event_loop, cause);
        }
    }

    fn user_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, event: UserEvent) {
        if let AppEnum::Init(app) = &mut self.inner {
            app.user_event(event_loop, event)
        }
    }

    fn device_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let AppEnum::Init(app) = &mut self.inner {
            app.device_event(event_loop, device_id, event)
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let AppEnum::Init(app) = &mut self.inner {
            app.about_to_wait(event_loop)
        }
    }

    fn suspended(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let AppEnum::Init(app) = &mut self.inner {
            app.suspended(event_loop)
        }
    }

    fn exiting(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let AppEnum::Init(app) = &mut self.inner {
            app.exiting(event_loop)
        }
    }

    fn memory_warning(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let AppEnum::Init(app) = &mut self.inner {
            app.memory_warning(event_loop)
        }
    }
}
