use std::path::Path;

use crate::{ShaderInfo, SHADER_PATH};

pub fn create_shader_module(
    shader_info: &ShaderInfo,
    shader_type: shaderc::ShaderKind,
    compiler: &mut shaderc::Compiler,
) -> shaderc::Result<shaderc::CompilationArtifact> {
    let shader_text = std::fs::read_to_string(&shader_info.path).unwrap();
    let mut compile_options =
        shaderc::CompileOptions::new().expect("Failed to create shader compiler options");
    // compile_options.set_warnings_as_errors();
    compile_options.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_2 as u32,
    );

    compile_options.set_optimization_level(shaderc::OptimizationLevel::Performance);
    compile_options.set_generate_debug_info();

    match shader_type {
        shaderc::ShaderKind::Fragment => {
            compile_options.add_macro_definition("FRAGMENT_SHADER", Some("1"))
        }
        shaderc::ShaderKind::Vertex => {
            compile_options.add_macro_definition("VERTEX_SHADER", Some("1"))
        }
        shaderc::ShaderKind::Compute => {
            compile_options.add_macro_definition("COMPUTE_SHADER", Some("1"))
        }
        _ => panic!("We doesn't support {:?} shaders yet", shader_type),
    }

    if cfg!(debug_assertions) {
        compile_options.add_macro_definition("DEBUG", Some("1"));
    } else {
        compile_options.add_macro_definition("NDEBUG", Some("1"));
    }

    compile_options.set_include_callback(|name, include_type, source_file, _depth| {
        let path = if include_type == shaderc::IncludeType::Relative {
            Path::new(Path::new(source_file).parent().unwrap()).join(name)
        } else {
            Path::new(SHADER_PATH).join(name)
        };
        match std::fs::read_to_string(&path) {
            Ok(glsl_code) => Ok(shaderc::ResolvedInclude {
                resolved_name: String::from(name),
                content: glsl_code,
            }),
            Err(err) => Err(format!(
                "Failed to resolve include to {} in {} (was looking for {:?}): {}",
                name, source_file, path, err
            )),
        }
    });

    match compiler.compile_into_spirv(
        &shader_text,
        shader_type,
        shader_info.path.to_str().unwrap(),
        "main",
        Some(&compile_options),
    ) {
        Ok(compilation_artifact) => {
            if compilation_artifact.get_num_warnings() > 0 {
                eprintln!(
                    "[WARNING] In shader {}:\n{}",
                    shader_info.path.display(),
                    compilation_artifact.get_warning_messages()
                );
            }
            Ok(compilation_artifact)
        }
        Err(e) => Err(e),
    }
}
