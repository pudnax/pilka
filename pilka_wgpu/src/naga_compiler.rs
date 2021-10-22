use naga::{
    back::spv::PipelineOptions,
    front::{self, glsl, wgsl},
    valid::{Capabilities, ValidationFlags, Validator},
    Module,
};

struct ShaderCompiler {
    wgsl: wgsl::Parser,
    glsl: glsl::Parser,
    validator: Validator,
    out: Vec<u32>,
}

impl ShaderCompiler {
    const SUPPORTED_SOURCES: &'static [&'static str] = &["glsl", "wgsl", "spv"];

    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_supported(&self, path: impl AsRef<Path>) -> bool {
        path.as_ref()
            .extension()
            .map(|ext| Self::SUPPORTED_SOURCES.contains(&ext.to_str().unwrap()))
            .is_some()
    }

    // TODO: Move outside
    pub fn from_path(
        &mut self,
        path: impl AsRef<Path>,
        stage: naga::ShaderStage,
    ) -> Option<Cow<str>> {
        let file = || std::fs::read_to_string(&path).unwrap();
        let module = match path.as_ref().extension() {
            Some(ext) => match ext.to_str() {
                Some("wgsl") => self.parse_wgsl(file(), stage),
                Some("glsl" | "frag" | "vert" | "comp") => self.parse_glsl(file(), stage),
                Some("spv") => self.parse_spv(file().as_bytes(), stage),
                _ => None,
            },
            None => None,
        }
        .unwrap();
        let module_info = self.validator.validate(&module).unwrap();
        Some(Cow::Owned(
            naga::back::wgsl::write_string(&module, &module_info).unwrap(),
        ))
    }

    fn wgsl_to_wgsl(&mut self, source: impl AsRef<str>) -> Option<Cow<str>> {
        let module = match self.wgsl.parse(source.as_ref()) {
            Ok(m) => m,
            Err(e) => {
                e.emit_to_stderr(source.as_ref());
                return None;
            }
        };
        let module_info = self.validator.validate(&module).unwrap();
        Some(Cow::Owned(
            naga::back::wgsl::write_string(&module, &module_info).unwrap(),
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
    pub fn parse_glsl(
        &mut self,
        source: impl AsRef<str>,
        stage: naga::ShaderStage,
    ) -> Option<Module> {
        match self
            .glsl
            .parse(&glsl::Options::from(stage), source.as_ref())
        {
            Ok(m) => Some(m),
            Err(span) => {
                println!("Got here");
                for e in span {
                    eprintln!("Glsl error: {e}");
                }
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
            glsl: glsl::Parser::default(),
            validator,
            out: Vec::new(),
        }
    }
}
