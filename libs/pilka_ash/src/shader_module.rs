use crate::device::{RawDevice, VkDevice};
use ash::{prelude::VkResult, version::DeviceV1_0, vk};

use std::sync::Arc;

pub struct VkShaderModule {
    pub module: vk::ShaderModule,
    device: Arc<RawDevice>,
}

impl VkShaderModule {
    pub fn new<P: AsRef<std::path::Path>>(
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
        let mut shader_data = std::io::Cursor::new(shader_data);
        let shader_code = ash::util::read_spv(&mut shader_data).unwrap();
        let shader_info = vk::ShaderModuleCreateInfo::builder().code(&shader_code);

        let module = unsafe { device.create_shader_module(&shader_info, None) }?;
        Ok(VkShaderModule {
            module,
            device: device.device.clone(),
        })
    }
}

impl Drop for VkShaderModule {
    fn drop(&mut self) {
        unsafe { self.device.destroy_shader_module(self.module, None) };
    }
}
