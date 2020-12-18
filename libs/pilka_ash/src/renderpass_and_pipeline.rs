use crate::{device::RawDevice, shader_module::ShaderInfo};
use ash::{prelude::VkResult, version::DeviceV1_0, vk};
use std::sync::Arc;

pub struct VkRenderPass {
    pub render_pass: vk::RenderPass,
    pub device: Arc<RawDevice>,
}

impl std::ops::Deref for VkRenderPass {
    type Target = vk::RenderPass;

    fn deref(&self) -> &Self::Target {
        &self.render_pass
    }
}

impl std::ops::DerefMut for VkRenderPass {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.render_pass
    }
}

impl Drop for VkRenderPass {
    fn drop(&mut self) {
        unsafe { self.device.destroy_render_pass(self.render_pass, None) };
    }
}

pub struct PipelineDescriptor {
    pub color_blend_attachments: Box<[vk::PipelineColorBlendAttachmentState]>,
    pub dynamic_state: Box<[vk::DynamicState]>,
    pub shader_stages: Box<[vk::PipelineShaderStageCreateInfo]>,
    pub vertex_input: vk::PipelineVertexInputStateCreateInfo,
    pub input_assembly: vk::PipelineInputAssemblyStateCreateInfo,
    pub rasterization: vk::PipelineRasterizationStateCreateInfo,
    pub multisample: vk::PipelineMultisampleStateCreateInfo,
    pub depth_stencil: vk::PipelineDepthStencilStateCreateInfo,
    pub color_blend: vk::PipelineColorBlendStateCreateInfo,
    pub dynamic_state_info: vk::PipelineDynamicStateCreateInfo,
}

impl PipelineDescriptor {
    pub fn new(shader_stages: Box<[vk::PipelineShaderStageCreateInfo]>) -> Self {
        let noop_stencil_state = vk::StencilOpState {
            fail_op: vk::StencilOp::KEEP,
            pass_op: vk::StencilOp::KEEP,
            depth_fail_op: vk::StencilOp::KEEP,
            compare_op: vk::CompareOp::ALWAYS,
            ..Default::default()
        };
        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo {
            depth_test_enable: 0,
            depth_write_enable: 0,
            depth_compare_op: vk::CompareOp::ALWAYS,
            front: noop_stencil_state,
            back: noop_stencil_state,
            max_depth_bounds: 1.0,
            ..Default::default()
        };

        let vertex_input = vk::PipelineVertexInputStateCreateInfo {
            vertex_attribute_description_count: 0,
            vertex_binding_description_count: 0,
            // vertex_attribute_description_count: vertex_input_attribute_descriptions.len()
            //     as u32,
            // p_vertex_attribute_descriptions: vertex_input_attribute_descriptions.as_ptr(),
            // vertex_binding_description_count: vertex_input_binding_descriptions.len() as u32,
            // p_vertex_binding_descriptions: vertex_input_binding_descriptions.as_ptr(),
            ..Default::default()
        };

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo {
            topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            ..Default::default()
        };

        let rasterization = vk::PipelineRasterizationStateCreateInfo {
            front_face: vk::FrontFace::COUNTER_CLOCKWISE,
            line_width: 1.0,
            polygon_mode: vk::PolygonMode::FILL,
            cull_mode: vk::CullModeFlags::BACK,
            ..Default::default()
        };
        let multisample = vk::PipelineMultisampleStateCreateInfo {
            rasterization_samples: vk::SampleCountFlags::TYPE_1,
            ..Default::default()
        };

        let color_blend_attachments = Box::new([vk::PipelineColorBlendAttachmentState {
            blend_enable: 0,
            src_color_blend_factor: vk::BlendFactor::SRC_COLOR,
            dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_DST_COLOR,
            color_blend_op: vk::BlendOp::ADD,
            src_alpha_blend_factor: vk::BlendFactor::ZERO,
            dst_alpha_blend_factor: vk::BlendFactor::ZERO,
            alpha_blend_op: vk::BlendOp::ADD,
            color_write_mask: vk::ColorComponentFlags::all(),
        }]);
        let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op(vk::LogicOp::CLEAR)
            .attachments(color_blend_attachments.as_ref())
            .build();

        let dynamic_state = Box::new([vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);
        let dynamic_state_info = vk::PipelineDynamicStateCreateInfo::builder()
            .dynamic_states(dynamic_state.as_ref())
            .build();

        Self {
            shader_stages,
            vertex_input,
            input_assembly,
            rasterization,
            multisample,
            depth_stencil,
            color_blend_attachments,
            color_blend,
            dynamic_state,
            dynamic_state_info,
        }
    }
}

// TODO: Remove `ShaderInfo`
pub struct VkPipeline {
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
    pub dynamic_state: Box<[vk::DynamicState]>,
    pub device: Arc<RawDevice>,
    pub vs_info: ShaderInfo,
    pub fs_info: ShaderInfo,
}

impl VkPipeline {
    pub fn new(
        pipeline_cache: vk::PipelineCache,
        pipeline_layout: vk::PipelineLayout,
        desc: PipelineDescriptor,
        render_pass: &VkRenderPass,
        vs_info: ShaderInfo,
        fs_info: ShaderInfo,
        device: Arc<RawDevice>,
    ) -> VkResult<Self> {
        let viewport = vk::PipelineViewportStateCreateInfo::builder()
            .viewports(&[vk::Viewport::default()])
            .scissors(&[vk::Rect2D::default()])
            .build();

        let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&desc.shader_stages)
            .vertex_input_state(&desc.vertex_input)
            .input_assembly_state(&desc.input_assembly)
            .rasterization_state(&desc.rasterization)
            .multisample_state(&desc.multisample)
            .depth_stencil_state(&desc.depth_stencil)
            .color_blend_state(&desc.color_blend)
            .dynamic_state(&desc.dynamic_state_info)
            .viewport_state(&viewport)
            .layout(pipeline_layout)
            .render_pass(render_pass.render_pass);

        let pipeline = unsafe {
            device.create_graphics_pipelines(pipeline_cache, &[pipeline_info.build()], None)
        }
        .expect("Unable to create graphics pipeline")
        .pop()
        .unwrap();

        Ok(VkPipeline {
            pipeline,
            pipeline_layout,
            dynamic_state: desc.dynamic_state,
            device,
            vs_info,
            fs_info,
        })
    }
}

impl Drop for VkPipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);

            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}
