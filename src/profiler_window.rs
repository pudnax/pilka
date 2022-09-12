use std::time::Instant;

use egui::FontDefinitions;
use egui_wgpu_backend::{
    wgpu::{
        self, Backends, Device, Features, Instance, Limits, Queue, RequestAdapterOptions, Surface,
        SurfaceConfiguration,
    },
    RenderPass, ScreenDescriptor,
};
use egui_winit_platform::{Platform, PlatformDescriptor};
use eyre::Result;
use winit::{
    dpi::PhysicalSize,
    event::Event,
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowId},
};

pub struct ProfilerWindow {
    surface: Surface,
    surface_config: SurfaceConfiguration,
    device: Device,
    queue: Queue,
    platform: Platform,
    render_pass: RenderPass,
    window: Window,
    previous_frame_time: Option<f32>,
}

impl ProfilerWindow {
    pub fn new<T>(event_loop: &EventLoopWindowTarget<T>) -> Result<Self> {
        let instance = Instance::new(Backends::PRIMARY);
        let window = Window::new(event_loop)?;
        let surface = unsafe { instance.create_surface(&window) };
        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: egui_wgpu_backend::wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .unwrap();

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                features: Features::default(),
                limits: Limits::default(),
                label: None,
            },
            None,
        ))?;

        let size = window.inner_size();
        let surface_format = surface.get_supported_formats(&adapter)[0].unwrap();
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width as u32,
            height: size.height as u32,
            present_mode: wgpu::PresentMode::Immediate,
        };
        surface.configure(&device, &surface_config);

        // We use the egui_winit_platform crate as the platform.
        let platform = Platform::new(PlatformDescriptor {
            physical_width: size.width as u32,
            physical_height: size.height as u32,
            scale_factor: window.scale_factor(),
            font_definitions: FontDefinitions::default(),
            style: Default::default(),
        });

        // We use the egui_wgpu_backend crate as the render backend.
        let render_pass = RenderPass::new(&device, surface_format, 1);

        Ok(Self {
            queue,
            surface,
            surface_config,
            device,
            platform,
            render_pass,
            window,
            previous_frame_time: Some(0.),
        })
    }

    pub fn handle_event<T>(&mut self, event: &Event<T>) {
        self.platform.handle_event(event);
    }
    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn resize(&mut self) {
        let PhysicalSize { width, height } = self.window.inner_size();
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw()
    }

    pub fn render(&mut self, start_time: &Instant) {
        self.platform
            .update_time(start_time.elapsed().as_secs_f64());

        let output_frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(e) => {
                eprintln!("Dropped frame with error: {}", e);
                return;
            }
        };
        let output_view = output_frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Begin to draw the UI frame.
        let egui_start = Instant::now();
        self.platform.begin_frame();

        let _ = puffin_egui::profiler_window(&self.platform.context());

        // End the UI frame. We could now handle the output and draw the UI with the backend.
        let (_output, paint_commands) = self.platform.end_frame(Some(&self.window));
        let paint_jobs = self.platform.context().tessellate(paint_commands);

        let frame_time = (Instant::now() - egui_start).as_secs_f64() as f32;
        self.previous_frame_time = Some(frame_time);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("encoder"),
            });

        // Upload all resources for the GPU.
        let screen_descriptor = ScreenDescriptor {
            physical_width: self.surface_config.width,
            physical_height: self.surface_config.height,
            scale_factor: self.window.scale_factor() as f32,
        };
        self.render_pass.update_texture(
            &self.device,
            &self.queue,
            &self.platform.context().font_image(),
        );
        self.render_pass
            .update_user_textures(&self.device, &self.queue);
        self.render_pass
            .update_buffers(&self.device, &self.queue, &paint_jobs, &screen_descriptor);

        // Record all render passes.
        self.render_pass
            .execute(
                &mut encoder,
                &output_view,
                &paint_jobs,
                &screen_descriptor,
                Some(wgpu::Color::BLACK),
            )
            .unwrap();
        // Submit the commands.
        self.queue.submit(std::iter::once(encoder.finish()));

        // Redraw egui
        output_frame.present();
    }
}
