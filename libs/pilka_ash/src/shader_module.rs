use crate::device::VkDevice;
use ash::{prelude::VkResult, version::DeviceV1_0, vk};

use std::ffi::CString;
use std::path::PathBuf;

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
    let shader_data = match compiler.compile_into_spirv(
        &shader_text,
        shader_type,
        path.name.to_str().unwrap(),
        path.entry_point.to_str().unwrap(),
        None,
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
