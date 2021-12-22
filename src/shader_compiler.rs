mod glsl;
mod wgsl;

use color_eyre::Result;
use eyre::Context;
pub use glsl::create_shader_module;
use pilka_types::{ShaderFlavor, ShaderInfo};

pub struct ShaderCompiler {
    wgsl: wgsl::ShaderCompiler,
    glsl: shaderc::Compiler,
}

impl ShaderCompiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_shader_module(
        &mut self,
        shader_info: &ShaderInfo,
        shader_stage: shaderc::ShaderKind,
    ) -> Result<Vec<u32>> {
        let module = match shader_info.flavour {
            ShaderFlavor::Wgsl => {
                unimplemented!()
            }
            ShaderFlavor::Glsl => {
                glsl::create_shader_module(shader_info, shader_stage, &mut self.glsl)
                    .context("Shader Compilation Error")?
                    .as_binary()
                    .to_vec()
            }
        };
        Ok(module)
    }
}

impl Default for ShaderCompiler {
    fn default() -> Self {
        Self {
            wgsl: wgsl::ShaderCompiler::default(),
            glsl: shaderc::Compiler::new().unwrap(),
        }
    }
}
