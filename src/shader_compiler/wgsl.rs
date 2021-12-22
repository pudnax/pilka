use std::borrow::Cow;

use naga::{
    back::{spv::PipelineOptions, wgsl::WriterFlags},
    front::{self, wgsl},
    valid::{Capabilities, ValidationFlags, Validator},
    Module,
};

pub struct ShaderCompiler {
    wgsl: wgsl::Parser,
    validator: Validator,
    out: Vec<u32>,
}

impl ShaderCompiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn wgsl_to_wgsl(&mut self, source: impl AsRef<str>) -> Option<Cow<str>> {
        let module = match self.wgsl.parse(source.as_ref()) {
            Ok(m) => m,
            Err(e) => {
                e.emit_to_stderr(source.as_ref());
                return None;
            }
        };
        let module_info = self.validator.validate(&module).unwrap();
        Some(Cow::Owned(
            naga::back::wgsl::write_string(&module, &module_info, WriterFlags::EXPLICIT_TYPES)
                .unwrap(),
        ))
    }

    fn compile(&mut self, module: Module, stage: naga::ShaderStage) -> &[u32] {
        let module_info = self.validator.validate(&module).unwrap();
        let mut writer = naga::back::spv::Writer::new(&naga::back::spv::Options::default())
            .expect("Failed to create spirv writer");

        self.out.clear();
        writer
            .write(
                &module,
                &module_info,
                Some(&PipelineOptions {
                    shader_stage: stage,
                    entry_point: "main".into(),
                }),
                &mut self.out,
            )
            .expect("Failed to write spirv");
        &self.out
    }

    pub fn parse_wgsl(
        &mut self,
        source: impl AsRef<str>,
        stage: naga::ShaderStage,
    ) -> Option<Module> {
        match self.wgsl.parse(source.as_ref()) {
            Ok(m) => Some(m),
            Err(e) => {
                e.emit_to_stderr(source.as_ref());
                return None;
            }
        }
    }
    pub fn parse_spv(&mut self, data: &[u8], stage: naga::ShaderStage) -> Option<Module> {
        match naga::front::spv::parse_u8_slice(data, &front::spv::Options::default()) {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!("Spir-V error {e}");
                return None;
            }
        }
    }
}

impl Default for ShaderCompiler {
    fn default() -> Self {
        let validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        Self {
            wgsl: wgsl::Parser::new(),
            validator,
            out: Vec::new(),
        }
    }
}
