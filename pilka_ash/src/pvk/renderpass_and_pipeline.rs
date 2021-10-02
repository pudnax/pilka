use crate::VkDevice;

use super::device::RawDevice;
use ash::{
    prelude::VkResult,
    vk::{self, ShaderModule},
};
use std::{ffi::CString, sync::Arc};

use super::instance::VkQueues;

#[derive(Debug)]
pub enum Pipeline {
    Graphics(VkGraphicsPipeline),
    Compute(VkComputePipeline),
}

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
    pub dynamic_state_info: vk::PipelineDynamicStateCreateInfo,
    pub dynamic_state: Box<[vk::DynamicState]>,
    pub vertex_module: ShaderModule,
    pub vertex_entry_point: CString,
    pub fragment_module: ShaderModule,
    pub fragment_entry_point: CString,
    pub vertex_input: vk::PipelineVertexInputStateCreateInfo,
    pub input_assembly: vk::PipelineInputAssemblyStateCreateInfo,
    pub rasterization: vk::PipelineRasterizationStateCreateInfo,
    pub multisample: vk::PipelineMultisampleStateCreateInfo,
    pub depth_stencil: vk::PipelineDepthStencilStateCreateInfo,
    pub color_blend: vk::PipelineColorBlendStateCreateInfo,
}

impl PipelineDescriptor {
    pub fn new(
        vertex_module: ShaderModule,
        vertex_entry_point: CString,
        fragment_module: ShaderModule,
        fragment_entry_point: CString,
    ) -> Self {
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
            color_blend_attachments,
            dynamic_state_info,
            dynamic_state,
            vertex_module,
            vertex_entry_point,
            fragment_module,
            fragment_entry_point,
            vertex_input,
            input_assembly,
            rasterization,
            multisample,
            depth_stencil,
            color_blend,
        }
    }
}

#[derive(Debug)]
pub struct VkGraphicsPipeline {
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
    pub dynamic_state: Box<[vk::DynamicState]>,
    pub descriptor_set_layouts: Vec<vk::DescriptorSetLayout>,
    pub device: Arc<RawDevice>,
}

impl VkGraphicsPipeline {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pipeline_cache: vk::PipelineCache,
        pipeline_layout: vk::PipelineLayout,
        descriptor_set_layouts: Vec<vk::DescriptorSetLayout>,
        desc: PipelineDescriptor,
        render_pass: &VkRenderPass,
        device: &VkDevice,
    ) -> VkResult<Self> {
        let viewport = vk::PipelineViewportStateCreateInfo::builder()
            .viewports(&[vk::Viewport::default()])
            .scissors(&[vk::Rect2D::default()])
            .build();

        let vertex_stage = vk::PipelineShaderStageCreateInfo::builder()
            .module(desc.vertex_module)
            .stage(vk::ShaderStageFlags::VERTEX)
            .name(desc.vertex_entry_point.as_c_str())
            .build();
        let fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
            .module(desc.fragment_module)
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .name(desc.fragment_entry_point.as_c_str())
            .build();
        let stages = [vertex_stage, fragment_stage];

        let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&stages)
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

        Ok(VkGraphicsPipeline {
            pipeline,
            pipeline_layout,
            dynamic_state: desc.dynamic_state,
            descriptor_set_layouts,
            device: device.device.clone(),
        })
    }
}

impl Drop for VkGraphicsPipeline {
    fn drop(&mut self) {
        unsafe {
            for desc_set_layout in &self.descriptor_set_layouts {
                self.device
                    .destroy_descriptor_set_layout(*desc_set_layout, None);
            }

            self.device.destroy_pipeline(self.pipeline, None);

            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}

#[derive(Debug)]
pub struct VkComputePipeline {
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
    pub descriptor_set_layouts: Vec<vk::DescriptorSetLayout>,
    pub command_pool: vk::CommandPool,
    pub command_buffer: vk::CommandBuffer,
    pub semaphore: vk::Semaphore,
    // pub cs_info: ShaderInfo,
    pub device: Arc<RawDevice>,
}

impl VkComputePipeline {
    pub fn new(
        device: &VkDevice,
        queues: &VkQueues,
        pipeline_layout: vk::PipelineLayout,
        descriptor_set_layouts: Vec<vk::DescriptorSetLayout>,
        shader_stage: vk::PipelineShaderStageCreateInfo,
    ) -> VkResult<Self> {
        let pipeline_info = vk::ComputePipelineCreateInfo::builder()
            .stage(shader_stage)
            .layout(pipeline_layout);

        let pipeline = unsafe {
            device.create_compute_pipelines(
                vk::PipelineCache::null(),
                &[pipeline_info.build()],
                None,
            )
        }
        .expect("Unable to create graphics pipeline")
        .pop()
        .unwrap();

        let command_pool_create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queues.compute_queue.index);
        let command_pool = unsafe { device.create_command_pool(&command_pool_create_info, None) }?;

        let command_buffer_create_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY);
        let command_buffer =
            unsafe { device.allocate_command_buffers(&command_buffer_create_info) }?[0];

        let semaphore = device.create_semaphore()?;
        device.name_semaphore(semaphore, "Compute Semaphore")?;

        // let signal_semaphores = [semaphore];
        // let submits = [vk::SubmitInfo::builder()
        //     .signal_semaphores(&signal_semaphores)
        //     .build()];
        // unsafe { device.queue_submit(queues.compute_queue.queue, &submits, vk::Fence::null()) }?;
        // unsafe { device.queue_wait_idle(queues.compute_queue.queue) }?;

        Ok(Self {
            pipeline,
            pipeline_layout,
            descriptor_set_layouts,
            command_pool,
            command_buffer,
            semaphore,
            // cs_info,
            device: device.device.clone(),
        })
    }
}

impl Drop for VkComputePipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_semaphore(self.semaphore, None);
            self.device.destroy_command_pool(self.command_pool, None);
            for desc_set_layout in &self.descriptor_set_layouts {
                self.device
                    .destroy_descriptor_set_layout(*desc_set_layout, None);
            }

            self.device.destroy_pipeline(self.pipeline, None);

            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}
