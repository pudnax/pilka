use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use crate::shader_module;

use color_eyre::Result;
use pilka_ash::{AshRender, HasRawWindowHandle};
use pilka_types::{Frame, ImageDimentions, PipelineInfo, ShaderCreateInfo};
use pilka_wgpu::WgpuRender;
use shaderc::Compiler;

#[derive(Debug)]
pub struct ContiniousHashMap<K, V>(HashMap<K, HashSet<V>>);

impl<K, V> Deref for ContiniousHashMap<K, V> {
    type Target = HashMap<K, HashSet<V>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K, V> DerefMut for ContiniousHashMap<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<K, V> ContiniousHashMap<K, V> {
    fn new() -> Self {
        Self(HashMap::new())
    }
}

impl<K: Eq + Hash, V: Eq + Hash> ContiniousHashMap<K, V> {
    fn push_value(&mut self, key: K, value: V) {
        self.0.entry(key).or_insert_with(HashSet::new).insert(value);
    }
}

pub trait Renderer {
    fn get_info(&self) -> String;

    fn pause(&mut self);

    fn resize(&mut self, width: u32, height: u32) -> Result<()>;

    fn render(&mut self, push_constant: &[u8]) -> Result<()>;

    fn capture_frame(&mut self) -> Result<Frame>;
    fn captured_frame_dimentions(&self) -> ImageDimentions;

    fn wait_idle(&self) {}
    fn shut_down(&self) {}
}

pub enum Backend<'a> {
    Ash(AshRender<'a>),
    Wgpu(WgpuRender),
}

pub struct RenderBundleStatic<'a> {
    kind: Option<Backend<'a>>,
    shader_set: ContiniousHashMap<PathBuf, usize>,
    pipelines: Vec<PipelineInfo>,
    includes: Vec<Vec<PathBuf>>,
    push_constant_range: u32,
    wh: (u32, u32),
}

impl<'a> RenderBundleStatic<'a> {
    pub async fn new(
        window: &impl HasRawWindowHandle,
        push_constant_range: u32,
        (width, height): (u32, u32),
    ) -> Result<RenderBundleStatic<'a>> {
        let kind = match false {
            true => {
                Backend::Wgpu(WgpuRender::new(window, push_constant_range, width, height).await?)
            }
            false => Backend::Ash(AshRender::new(window, push_constant_range).unwrap()),
        };
        Ok(Self {
            kind: Some(kind),
            shader_set: ContiniousHashMap::new(),
            pipelines: vec![],
            includes: vec![],
            push_constant_range,
            wh: (width, height),
        })
    }

    pub fn push_pipeline(
        &mut self,
        pipeline: PipelineInfo,
        includes: &[PathBuf],
        shader_compiler: &mut Compiler,
    ) -> Result<()> {
        let pipeline_number = self.pipelines.len();
        match pipeline {
            PipelineInfo::Rendering { ref vert, ref frag } => {
                self.shader_set
                    .push_value(frag.path.canonicalize()?, pipeline_number);
                self.shader_set
                    .push_value(vert.path.canonicalize()?, pipeline_number);

                let vert_artifact = shader_module::create_shader_module(
                    vert,
                    shaderc::ShaderKind::Vertex,
                    shader_compiler,
                )?;
                let vert = ShaderCreateInfo::new(vert_artifact.as_binary(), &vert.entry_point);

                let frag_arifact = shader_module::create_shader_module(
                    frag,
                    shaderc::ShaderKind::Fragment,
                    shader_compiler,
                )?;
                let frag = ShaderCreateInfo::new(frag_arifact.as_binary(), &frag.entry_point);

                match self.kind.as_mut().unwrap() {
                    Backend::Ash(ash) => ash.push_render_pipeline(vert, frag)?,
                    Backend::Wgpu(wgpu) => wgpu.push_render_pipeline(vert, frag)?,
                }
            }
            PipelineInfo::Compute { ref comp } => {
                self.shader_set
                    .push_value(comp.path.canonicalize()?, pipeline_number);

                let comp_artifact = shader_module::create_shader_module(
                    comp,
                    shaderc::ShaderKind::Compute,
                    shader_compiler,
                )?;
                let comp = ShaderCreateInfo::new(comp_artifact.as_binary(), &comp.entry_point);

                match self.kind.as_mut().unwrap() {
                    Backend::Ash(ash) => ash.push_compute_pipeline(comp)?,
                    Backend::Wgpu(wgpu) => wgpu.push_compute_pipeline(comp)?,
                }
            }
        }
        for include in includes {
            self.shader_set.push_value(include.clone(), pipeline_number);
        }
        self.pipelines.push(pipeline.clone());
        self.includes.push(includes.to_vec());

        Ok(())
    }

    pub fn register_shader_change(
        &mut self,
        paths: &[PathBuf],
        shader_compiler: &mut Compiler,
    ) -> Result<()> {
        self.wait_idle();
        for path in paths {
            if let Some(pipeline_indices) = self.shader_set.get(path) {
                for &index in pipeline_indices {
                    match &self.pipelines[index] {
                        PipelineInfo::Rendering { vert, frag } => {
                            let vert_artifact = shader_module::create_shader_module(
                                vert,
                                shaderc::ShaderKind::Vertex,
                                shader_compiler,
                            )?;
                            let vert =
                                ShaderCreateInfo::new(vert_artifact.as_binary(), &vert.entry_point);

                            let frag_arifact = shader_module::create_shader_module(
                                frag,
                                shaderc::ShaderKind::Fragment,
                                shader_compiler,
                            )?;
                            let frag =
                                ShaderCreateInfo::new(frag_arifact.as_binary(), &frag.entry_point);

                            match self.kind.as_mut().unwrap() {
                                Backend::Ash(ash) => {
                                    ash.rebuild_render_pipeline(index, vert, frag)?
                                }
                                Backend::Wgpu(wgpu) => {
                                    wgpu.rebuild_render_pipeline(index, vert, frag)?
                                }
                            }
                        }

                        PipelineInfo::Compute { comp } => {
                            let comp_artifact = shader_module::create_shader_module(
                                comp,
                                shaderc::ShaderKind::Compute,
                                shader_compiler,
                            )?;
                            let comp =
                                ShaderCreateInfo::new(comp_artifact.as_binary(), &comp.entry_point);

                            match self.kind.as_mut().unwrap() {
                                Backend::Ash(ash) => ash.rebuild_compute_pipeline(index, comp)?,
                                Backend::Wgpu(wgpu) => {
                                    wgpu.rebuild_compute_pipeline(index, comp)?
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn get_active(&self) -> &dyn Renderer {
        match self.kind.as_ref().unwrap() {
            Backend::Ash(ash) => ash,
            Backend::Wgpu(wgpu) => wgpu,
        }
    }
    fn get_active_mut(&mut self) -> &mut dyn Renderer {
        match self.kind.as_mut().unwrap() {
            Backend::Ash(ash) => ash,
            Backend::Wgpu(wgpu) => wgpu,
        }
    }
    pub fn shader_list(&self) -> Vec<PathBuf> {
        self.shader_set.keys().cloned().collect()
    }
    pub async fn switch(
        &mut self,
        window: &impl HasRawWindowHandle,
        shader_compiler: &mut Compiler,
    ) -> Result<()> {
        self.wait_idle();
        #[derive(Debug)]
        enum Kind {
            Ash,
            Wgpu,
        }
        let kind = match &self.kind {
            Some(Backend::Ash(_)) => Kind::Ash,
            Some(Backend::Wgpu(_)) => Kind::Wgpu,
            _ => unreachable!(),
        };
        let old = self.kind.take();
        drop(old);

        self.kind = match kind {
            Kind::Ash => Some(Backend::Wgpu(
                WgpuRender::new(window, self.push_constant_range, self.wh.0, self.wh.1)
                    .await
                    .unwrap(),
            )),
            Kind::Wgpu => Some(Backend::Ash(
                AshRender::new(window, self.push_constant_range).unwrap(),
            )),
        };

        for pipeline in &self.pipelines {
            match pipeline {
                PipelineInfo::Rendering { vert, frag } => {
                    let vert_artifact = shader_module::create_shader_module(
                        vert,
                        shaderc::ShaderKind::Vertex,
                        shader_compiler,
                    )?;
                    let vert = ShaderCreateInfo::new(vert_artifact.as_binary(), &vert.entry_point);

                    let frag_arifact = shader_module::create_shader_module(
                        frag,
                        shaderc::ShaderKind::Fragment,
                        shader_compiler,
                    )?;
                    let frag = ShaderCreateInfo::new(frag_arifact.as_binary(), &frag.entry_point);

                    match self.kind.as_mut().unwrap() {
                        Backend::Ash(ash) => ash.push_render_pipeline(vert, frag)?,
                        Backend::Wgpu(wgpu) => wgpu.push_render_pipeline(vert, frag)?,
                    }
                }
                PipelineInfo::Compute { comp } => {
                    let comp_artifact = shader_module::create_shader_module(
                        comp,
                        shaderc::ShaderKind::Compute,
                        shader_compiler,
                    )?;
                    let comp = ShaderCreateInfo::new(comp_artifact.as_binary(), &comp.entry_point);
                    match self.kind.as_mut().unwrap() {
                        Backend::Ash(ash) => ash.push_compute_pipeline(comp)?,
                        Backend::Wgpu(wgpu) => wgpu.push_compute_pipeline(comp)?,
                    }
                }
            }
        }

        println!(
            "Switched to: {}",
            match kind {
                Kind::Ash => "Wgpu",
                Kind::Wgpu => "Ash",
            }
        );

        Ok(())
    }
}

impl Renderer for RenderBundleStatic<'_> {
    fn get_info(&self) -> String {
        self.get_active().get_info()
    }

    fn pause(&mut self) {
        self.get_active_mut().pause()
    }
    fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        self.wh = (width, height);
        self.get_active_mut().resize(width, height)
    }
    fn render(&mut self, push_constant: &[u8]) -> Result<()> {
        self.get_active_mut().render(push_constant)
    }
    fn capture_frame(
        &mut self,
    ) -> std::result::Result<(&[u8], pilka_types::ImageDimentions), eyre::Report> {
        self.get_active_mut().capture_frame()
    }
    fn captured_frame_dimentions(&self) -> ImageDimentions {
        self.get_active().captured_frame_dimentions()
    }

    fn wait_idle(&self) {
        self.get_active().wait_idle()
    }
    fn shut_down(&self) {
        self.get_active().shut_down()
    }
}

impl Renderer for AshRender<'_> {
    fn get_info(&self) -> String {
        self.get_info().to_string()
    }

    fn pause(&mut self) {
        self.paused = !self.paused;
    }

    fn resize(&mut self, _width: u32, _height: u32) -> Result<()> {
        Ok(self.resize()?)
    }

    fn render(&mut self, push_constant: &[u8]) -> Result<()> {
        Ok(self.render(push_constant)?)
    }

    fn capture_frame(&mut self) -> Result<Frame> {
        Ok(self.capture_frame()?)
    }
    fn captured_frame_dimentions(&self) -> ImageDimentions {
        self.screenshot_dimentions()
    }

    fn wait_idle(&self) {
        unsafe { self.device.device_wait_idle().unwrap() }
    }
    fn shut_down(&self) {
        unsafe { self.device.device_wait_idle().unwrap() }
    }
}

impl Renderer for pilka_wgpu::WgpuRender {
    fn get_info(&self) -> String {
        self.get_info().to_string()
    }

    fn pause(&mut self) {
        self.paused = !self.paused;
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        self.resize(width, height);
        Ok(())
    }

    fn render(&mut self, push_constant: &[u8]) -> Result<()> {
        Ok(Self::render(self, push_constant)?)
    }

    fn capture_frame(&mut self) -> Result<Frame> {
        let (data, dim) = self.capture_frame()?;
        Ok((&data, dim))
    }

    fn captured_frame_dimentions(&self) -> ImageDimentions {
        self.screenshot_dimentions()
    }

    fn wait_idle(&self) {
        self.wait_idle()
    }
}
