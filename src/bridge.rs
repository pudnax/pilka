use std::path::{Path, PathBuf};

use eyre::Result;
use pilka_ash::PilkaRender;
use pilka_types::{Frame, ImageDimentions, ShaderInfo};

pub trait Renderer {
    fn get_info(&self) -> String;

    fn update(&mut self);

    fn input(&mut self);

    fn shader_list(&self) -> Vec<PathBuf>;

    fn push_render_pipeline(
        &mut self,
        vert_info: ShaderInfo,
        frag_info: ShaderInfo,
        includes: &[PathBuf],
    ) -> Result<()>;
    fn push_compute_pipeline(&mut self, comp_info: ShaderInfo, includes: &[PathBuf]) -> Result<()>;

    fn rebuild_pipelines(&mut self, paths: &[PathBuf]) -> Result<()>;

    fn rebuild_all_pipelines(&mut self) -> Result<()> {
        self.rebuild_pipelines(&self.shader_list())
    }

    fn pause(&mut self);

    fn resize(&mut self, width: u32, height: u32) -> Result<()>;

    fn render(&mut self, push_constant: &[u8]) -> Result<()>;

    fn capture_frame(&mut self) -> Result<Frame>;
    fn captured_frame_dimentions(&self) -> ImageDimentions;

    fn shut_down(&self) {}
}

struct RenderBundle {
    list: Vec<Box<dyn Renderer>>,
    active: usize,
}

impl RenderBundle {
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

impl Renderer for RenderBundle {
    fn get_info(&self) -> String {
        self.get_active().get_info()
    }
    fn push_render_pipeline(
        &mut self,
        vert_info: pilka_types::ShaderInfo,
        frag_info: pilka_types::ShaderInfo,
        includes: &[std::path::PathBuf],
    ) -> Result<()> {
        self.get_active_mut()
            .push_render_pipeline(vert_info, frag_info, includes)
    }
    fn update(&mut self) {
        self.get_active_mut().update()
    }
    fn input(&mut self) {
        self.get_active_mut().input()
    }
    fn push_compute_pipeline(
        &mut self,
        comp_info: pilka_types::ShaderInfo,
        includes: &[std::path::PathBuf],
    ) -> Result<()> {
        self.get_active_mut()
            .push_compute_pipeline(comp_info, includes)
    }
    fn rebuild_pipelines(&mut self, paths: &[PathBuf]) -> Result<()> {
        self.get_active_mut().rebuild_pipelines(paths)
    }
    fn pause(&mut self) {
        self.get_active_mut().pause()
    }
    fn resize(&mut self, width: u32, height: u32) -> Result<()> {
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
}

impl Renderer for PilkaRender<'_> {
    fn get_info(&self) -> String {
        self.get_info().to_string()
    }

    fn update(&mut self) {
        todo!()
    }

    fn input(&mut self) {}

    fn push_render_pipeline(
        &mut self,
        vert_info: ShaderInfo,
        frag_info: ShaderInfo,
        includes: &[PathBuf],
    ) -> Result<()> {
        Ok(self.push_render_pipeline(vert_info, frag_info, includes)?)
    }

    fn push_compute_pipeline(&mut self, comp_info: ShaderInfo, includes: &[PathBuf]) -> Result<()> {
        Ok(self.push_compute_pipeline(comp_info, includes)?)
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

    fn shut_down(&self) {
        unsafe { self.device.device_wait_idle().unwrap() }
    }
}
