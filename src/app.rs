use std::{error::Error, io::Write, path::PathBuf};

use crate::render_bundle::{RenderBundleStatic, Renderer};
use crate::utils::{self, print_help};
use crate::{default_shaders, recorder};
use pilka_types::{PipelineInfo, PushConstant, ShaderInfo};

use notify::event::{EventKind, ModifyKind};
use winit::dpi::PhysicalSize;

pub const SHADER_PATH: &str = "shaders";
const SHADER_ENTRY_POINT: &str = "main";

use crate::shader_compiler::ShaderCompiler;

pub struct App<'a> {
    pub render: RenderBundleStatic<'a>,
    pub compiler: ShaderCompiler,
    pub push_constant: PushConstant,
    pub has_ffmpeg: bool,

    pub folder_events: crossbeam_channel::Receiver<notify::event::Event>,
}

impl<'a> App<'a> {
    pub fn new(
        window: &winit::window::Window,
        folder_events: crossbeam_channel::Receiver<notify::event::Event>,
        wgsl_mode: Option<()>,
    ) -> Result<Self, Box<dyn Error>> {
        let PhysicalSize { width, height } = window.inner_size();
        let mut render = RenderBundleStatic::new(&window, PushConstant::size(), (width, height))?;

        let shader_dir = PathBuf::new().join(SHADER_PATH);

        if !shader_dir.is_dir() {
            default_shaders::create_default_shaders(&shader_dir, wgsl_mode)?;
        }

        let spec = utils::parse_folder(SHADER_PATH)?;

        let mut compiler = ShaderCompiler::new();

        // Compute pipeline have to go first
        render.push_pipeline(
            PipelineInfo::Compute {
                comp: ShaderInfo::new(spec.comp.path, SHADER_ENTRY_POINT.into(), spec.comp.ty),
            },
            &[spec.glsl_prelude.clone().unwrap()],
            &mut compiler,
        )?;

        render.push_pipeline(
            PipelineInfo::Rendering {
                vert: ShaderInfo::new(spec.vert.path, SHADER_ENTRY_POINT.into(), spec.vert.ty),
                frag: ShaderInfo::new(spec.frag.path, SHADER_ENTRY_POINT.into(), spec.frag.ty),
            },
            &[spec.glsl_prelude.unwrap()],
            &mut compiler,
        )?;

        let (ffmpeg_version, has_ffmpeg) = recorder::ffmpeg_version()?;

        println!("{}", render.get_info());
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

        let push_constant = PushConstant::default();

        Ok(Self {
            render,
            compiler,
            push_constant,
            has_ffmpeg,

            folder_events,
        })
    }

    pub fn setup_frame(&mut self) {
        if let Ok(rx_event) = self.folder_events.try_recv() {
            if let notify::Event {
                kind: EventKind::Modify(ModifyKind::Data(_)),
                ..
            } = rx_event
            {
                puffin::profile_scope!("Shader Change Event");
                match self
                    .render
                    .register_shader_change(rx_event.paths.as_ref(), &mut self.compiler)
                {
                    Ok(_) => {
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
                    Err(e) => {
                        eprintln!("{}", e)
                    }
                };
            }
        }

        self.render.pause();
    }

    pub fn render(&mut self) {
        self.render.render(self.push_constant).unwrap();
        self.push_constant.frame += 1;
    }

    pub fn resize(&mut self, width: u32, height: u32) -> eyre::Result<()> {
        self.render.resize(width, height)
    }

    pub fn shut_down(&self) {
        self.render.shut_down();
    }
}
