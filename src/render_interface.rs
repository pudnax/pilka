use std::path::PathBuf;

use color_eyre::Result;
use pilka_ash::{AshRender, HasRawWindowHandle};
use pilka_types::{Frame, ImageDimentions, PipelineInfo};
use pilka_wgpu::WgpuRender;

pub trait Renderer {
    fn get_info(&self) -> String;

    fn update(&mut self);

    fn input(&mut self);

    fn shader_list(&self) -> Vec<PathBuf>;

    fn push_pipeline(&mut self, pipeline: PipelineInfo, includes: &[PathBuf]) -> Result<()>;

    fn rebuild_pipelines(&mut self, paths: &[PathBuf]) -> Result<()>;

    fn rebuild_all_pipelines(&mut self) -> Result<()> {
        self.rebuild_pipelines(&self.shader_list())
    }

    fn pause(&mut self);

    fn resize(&mut self, width: u32, height: u32) -> Result<()>;

    fn render(&mut self, push_constant: &[u8]) -> Result<()>;

    fn capture_frame(&mut self) -> Result<Frame>;
    fn captured_frame_dimentions(&self) -> ImageDimentions;

    fn wait_idle(&self) {}
    fn shut_down(&self) {}
}

pub enum RendererType<'a> {
    Ash(AshRender<'a>),
    Wgpu(WgpuRender),
}

pub struct RenderBundleStatic<'a> {
    kind: Option<RendererType<'a>>,
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
        // let wgpu = WgpuRender::new(window, push_constant_range).await.unwrap();
        let ash = AshRender::new(window, push_constant_range).unwrap();
        Ok(Self {
            kind: Some(RendererType::Ash(ash)),
            pipelines: vec![],
            includes: vec![],
            push_constant_range,
            wh: (width, height),
        })
    }
    fn get_active(&self) -> &dyn Renderer {
        match self.kind.as_ref().unwrap() {
            RendererType::Ash(ash) => ash,
            RendererType::Wgpu(wgpu) => wgpu,
        }
    }
    fn get_active_mut(&mut self) -> &mut dyn Renderer {
        match self.kind.as_mut().unwrap() {
            RendererType::Ash(ash) => ash,
            RendererType::Wgpu(wgpu) => wgpu,
        }
    }
    pub async fn switch(&mut self, window: &impl HasRawWindowHandle) -> Result<()> {
        self.wait_idle();
        enum Kind {
            Ash,
            Wgpu,
        }
        let kind = match &self.kind {
            Some(RendererType::Ash(_)) => Kind::Ash,
            Some(RendererType::Wgpu(_)) => Kind::Wgpu,
            _ => unreachable!(),
        };
        let old = self.kind.take();
        drop(old);

        self.kind = match kind {
            Kind::Ash => Some(RendererType::Wgpu(
                WgpuRender::new(window, self.push_constant_range, self.wh.0, self.wh.1)
                    .await
                    .unwrap(),
            )),
            Kind::Wgpu => Some(RendererType::Ash(
                AshRender::new(window, self.push_constant_range).unwrap(),
            )),
        };
        match &mut self.kind {
            Some(RendererType::Ash(ash)) => {
                for (pipeline, include) in self.pipelines.iter().zip(&self.includes) {
                    ash.push_pipeline(pipeline.clone(), include)?;
                }
            }
            Some(RendererType::Wgpu(wgpu)) => {
                for (pipeline, include) in self.pipelines.iter().zip(&self.includes) {
                    wgpu.push_pipeline(pipeline.clone(), include)?;
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}

impl Renderer for RenderBundleStatic<'_> {
    fn get_info(&self) -> String {
        self.get_active().get_info()
    }
    fn update(&mut self) {
        self.get_active_mut().update()
    }
    fn input(&mut self) {
        self.get_active_mut().input()
    }

    fn push_pipeline(&mut self, pipeline: PipelineInfo, includes: &[PathBuf]) -> Result<()> {
        self.pipelines.push(pipeline.clone());
        self.includes.push(includes.to_vec());
        self.get_active_mut().push_pipeline(pipeline, includes)
    }

    fn rebuild_pipelines(&mut self, paths: &[PathBuf]) -> Result<()> {
        self.get_active_mut().rebuild_pipelines(paths)
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
    fn shader_list(&self) -> Vec<PathBuf> {
        self.get_active().shader_list()
    }

    fn wait_idle(&self) {
        self.get_active().wait_idle()
    }
    fn shut_down(&self) {
        self.get_active().shut_down()
    }
}

// FIXME: Can't have several renderers with the same surface
struct RenderBundleDyn {
    list: Vec<Box<dyn Renderer>>,
    active: usize,
}

impl RenderBundleDyn {
    fn get_active(&self) -> &dyn Renderer {
        &*self.list[self.active]
    }
    fn get_active_mut(&mut self) -> &mut dyn Renderer {
        &mut *self.list[self.active]
    }
    fn next(&mut self) -> Result<()> {
        self.active = (self.active + 1) % self.list.len();
        self.get_active_mut().rebuild_all_pipelines()
    }
}

impl Renderer for AshRender<'_> {
    fn get_info(&self) -> String {
        self.get_info().to_string()
    }

    fn update(&mut self) {
        todo!()
    }

    fn input(&mut self) {
        todo!()
    }

    fn push_pipeline(&mut self, pipeline: PipelineInfo, includes: &[PathBuf]) -> Result<()> {
        match pipeline {
            PipelineInfo::Rendering { vert, frag } => {
                self.push_render_pipeline(vert, frag, includes)?
            }
            PipelineInfo::Compute { comp } => self.push_compute_pipeline(comp, includes)?,
        }
        Ok(())
    }

    fn rebuild_pipelines(&mut self, paths: &[PathBuf]) -> Result<()> {
        Ok(self.rebuild_pipelines(paths)?)
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

    fn shader_list(&self) -> Vec<PathBuf> {
        self.shader_set.keys().cloned().collect()
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

    fn update(&mut self) {
        todo!()
    }

    fn input(&mut self) {
        todo!()
    }

    fn shader_list(&self) -> Vec<PathBuf> {
        self.shader_set.keys().cloned().collect()
    }

    fn push_pipeline(&mut self, pipeline: PipelineInfo, includes: &[PathBuf]) -> Result<()> {
        match pipeline {
            PipelineInfo::Rendering { vert, frag } => {
                self.push_render_pipeline(vert, frag, includes)
            }
            PipelineInfo::Compute { comp } => self.push_compute_pipeline(comp, includes),
        }
    }

    fn rebuild_pipelines(&mut self, paths: &[PathBuf]) -> Result<()> {
        self.rebuild_pipelines(paths)
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
        todo!()
    }

    fn captured_frame_dimentions(&self) -> ImageDimentions {
        todo!()
    }

    fn wait_idle(&self) {
        self.wait_idle()
    }
}
