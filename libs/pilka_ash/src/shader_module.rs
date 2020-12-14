use crate::device::VkDevice;
use ash::{prelude::VkResult, version::DeviceV1_0, vk};

use std::path::{Path, PathBuf};

pub struct VkShaderModule {
    pub path: PathBuf,
    pub module: vk::ShaderModule,
}

impl VkShaderModule {
    pub fn new<P: Into<PathBuf> + AsRef<Path>>(
        path: P,
        shader_type: shaderc::ShaderKind,
        compiler: &mut shaderc::Compiler,
        device: &VkDevice,
    ) -> VkResult<Self> {
        let shader_text = std::fs::read_to_string(&path).unwrap();
        let shader_data = compiler
            .compile_into_spirv(
                &shader_text,
                shader_type,
                path.as_ref().to_str().unwrap(),
                "main",
                None,
            )
            .unwrap();
        let shader_data = shader_data.as_binary_u8();
        let shader_code = crate::utils::make_spirv(&shader_data);
        let shader_info = vk::ShaderModuleCreateInfo::builder().code(&shader_code);

        let module = unsafe { device.create_shader_module(&shader_info, None) }?;
        Ok(VkShaderModule {
            path: path.into(),
            module,
        })
    }
}
