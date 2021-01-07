use crate::device::VkDevice;
use ash::{prelude::VkResult, version::DeviceV1_0, vk};

use std::ffi::CString;
use std::path::{Path, PathBuf};

pub const SHADER_PATH: &str = "shaders";
pub const SHADER_ENTRY_POINT: &str = "main";

#[derive(Hash, Debug, Clone)]
pub struct ShaderInfo {
    pub name: PathBuf,
    pub entry_point: CString,
}

impl ShaderInfo {
    pub fn new(path: PathBuf, entry_point: String) -> Result<ShaderInfo, std::ffi::NulError> {
        Ok(ShaderInfo {
            name: path,
            entry_point: CString::new(entry_point)?,
        })
    }
}

#[derive(Debug)]
pub struct VkShaderModule {
    pub path: PathBuf,
    pub module: vk::ShaderModule,
}

pub fn create_shader_module(
    path: ShaderInfo,
    shader_type: shaderc::ShaderKind,
    compiler: &mut shaderc::Compiler,
    device: &VkDevice,
) -> VkResult<vk::ShaderModule> {
    let shader_text = std::fs::read_to_string(&path.name).unwrap();
    let mut compile_options = shaderc::CompileOptions::new().unwrap();
    compile_options.set_warnings_as_errors();
    compile_options.set_target_env(shaderc::TargetEnv::Vulkan, 0);
    compile_options.set_optimization_level(shaderc::OptimizationLevel::Performance);
    //}
    // Helps a lot when inspecting in ShaderDoc (will show all original source files before processing) but doesn't seem to hurt performance at all :)
    compile_options.set_generate_debug_info();

    compile_options.add_macro_definition(
        "FRAGMENT_SHADER",
        Some(if shader_type == shaderc::ShaderKind::Fragment {
            "1"
        } else {
            "0"
        }),
    );
    compile_options.add_macro_definition(
        "VERTEX_SHADER",
        Some(if shader_type == shaderc::ShaderKind::Vertex {
            "1"
        } else {
            "0"
        }),
    );
    compile_options.add_macro_definition(
        "COMPUTE_SHADER",
        Some(if shader_type == shaderc::ShaderKind::Compute {
            "1"
        } else {
            "0"
        }),
    );

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

    let shader_data = match compiler.compile_into_spirv(
        &shader_text,
        shader_type,
        path.name.to_str().unwrap(),
        path.entry_point.to_str().unwrap(),
        Some(&compile_options),
    ) {
        Ok(data) => data,
        Err(e) => {
            println!("{}", e);
            return Err(ash::vk::Result::ERROR_UNKNOWN);
        }
    };
    let shader_data = shader_data.as_binary_u8();
    let shader_code = crate::utils::make_spirv(&shader_data);
    let shader_info = vk::ShaderModuleCreateInfo::builder().code(&shader_code);

    Ok(unsafe { device.create_shader_module(&shader_info, None) }?)
}
