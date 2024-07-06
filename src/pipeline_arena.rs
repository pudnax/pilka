use ahash::{AHashMap, AHashSet};
use anyhow::Result;
use either::Either;
use slotmap::SlotMap;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use ash::{
    prelude::VkResult,
    vk::{self},
};

use crate::{Device, RawDevice, ShaderCompiler, ShaderKind, ShaderSource, Watcher};

pub struct ComputePipeline {
    pub layout: vk::PipelineLayout,
    pub pipeline: vk::Pipeline,
    shader_path: PathBuf,
    device: RawDevice,
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

impl ComputePipeline {
    fn new(
        device: &RawDevice,
        shader_compiler: &ShaderCompiler,
        shader_path: impl AsRef<Path>,
        push_constant_ranges: &[vk::PushConstantRange],
        descriptor_set_layouts: &[vk::DescriptorSetLayout],
    ) -> Result<Self> {
        let cs_bytes = shader_compiler.compile(&shader_path, shaderc::ShaderKind::Compute)?;

        let pipeline_layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(descriptor_set_layouts)
                    .push_constant_ranges(push_constant_ranges),
                None,
            )?
        };

        let mut shader_module = vk::ShaderModuleCreateInfo::default().code(cs_bytes.as_binary());
        let shader_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .name(c"main")
            .push_next(&mut shader_module);

        let create_info = vk::ComputePipelineCreateInfo::default()
            .layout(pipeline_layout)
            .stage(shader_stage);
        let pipeline = unsafe {
            device.create_compute_pipelines(vk::PipelineCache::null(), &[create_info], None)
        };
        let pipeline = pipeline.map_err(|(_, err)| err)?[0];

        Ok(Self {
            pipeline,
            shader_path: shader_path.as_ref().to_path_buf(),
            layout: pipeline_layout,
            device: device.clone(),
        })
    }

    pub fn reload(&mut self, shader_compiler: &ShaderCompiler) -> Result<()> {
        let cs_bytes = shader_compiler.compile(&self.shader_path, shaderc::ShaderKind::Compute)?;

        unsafe { self.device.destroy_pipeline(self.pipeline, None) }

        let mut shader_module = vk::ShaderModuleCreateInfo::default().code(cs_bytes.as_binary());
        let shader_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .name(c"main")
            .push_next(&mut shader_module);

        let create_info = vk::ComputePipelineCreateInfo::default()
            .layout(self.layout)
            .stage(shader_stage);
        let pipeline = unsafe {
            self.device
                .create_compute_pipelines(vk::PipelineCache::null(), &[create_info], None)
        };
        let pipeline = pipeline.map_err(|(_, err)| err)?[0];

        self.pipeline = pipeline;

        Ok(())
    }
}

pub struct VertexInputDesc {
    pub primitive_topology: vk::PrimitiveTopology,
    pub primitive_restart: bool,
}

impl Default for VertexInputDesc {
    fn default() -> Self {
        Self {
            primitive_topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            primitive_restart: false,
        }
    }
}

pub struct VertexShaderDesc {
    pub shader_path: PathBuf,
    pub dynamic_state: Vec<vk::DynamicState>,
    pub line_width: f32,
    pub polygon_mode: vk::PolygonMode,
    pub cull_mode: vk::CullModeFlags,
    pub front_face: vk::FrontFace,
    pub viewport_count: u32,
    pub scissot_count: u32,
}

impl Default for VertexShaderDesc {
    fn default() -> Self {
        Self {
            shader_path: PathBuf::new(),
            dynamic_state: vec![vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR],
            line_width: 1.0,
            polygon_mode: vk::PolygonMode::FILL,
            cull_mode: vk::CullModeFlags::BACK,
            front_face: vk::FrontFace::COUNTER_CLOCKWISE,
            viewport_count: 1,
            scissot_count: 1,
        }
    }
}

pub struct FragmentShaderDesc {
    pub shader_path: PathBuf,
}

pub struct FragmentOutputDesc {
    pub surface_format: vk::Format,
    pub multisample_state: vk::SampleCountFlags,
}

impl Default for FragmentOutputDesc {
    fn default() -> Self {
        Self {
            surface_format: vk::Format::B8G8R8A8_SRGB,
            multisample_state: vk::SampleCountFlags::TYPE_1,
        }
    }
}

pub struct RenderPipeline {
    pub layout: vk::PipelineLayout,
    pub pipeline: vk::Pipeline,
    vertex_input_lib: vk::Pipeline,
    vertex_shader_lib: vk::Pipeline,
    fragment_shader_lib: vk::Pipeline,
    fragment_output_lib: vk::Pipeline,
    device: RawDevice,
}

impl RenderPipeline {
    pub fn new(
        device: &RawDevice,
        shader_compiler: &ShaderCompiler,
        vertex_input_desc: &VertexInputDesc,
        vertex_shader_desc: &VertexShaderDesc,
        fragment_shader_desc: &FragmentShaderDesc,
        fragment_output_desc: &FragmentOutputDesc,
        push_constant_ranges: &[vk::PushConstantRange],
        descriptor_set_layouts: &[vk::DescriptorSetLayout],
    ) -> Result<Self> {
        let vs_bytes = shader_compiler
            .compile(&vertex_shader_desc.shader_path, shaderc::ShaderKind::Vertex)?;
        let fs_bytes = shader_compiler.compile(
            &fragment_shader_desc.shader_path,
            shaderc::ShaderKind::Fragment,
        )?;

        let pipeline_layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(descriptor_set_layouts)
                    .push_constant_ranges(push_constant_ranges),
                None,
            )?
        };

        use vk::GraphicsPipelineLibraryFlagsEXT as GPF;
        let vertex_input_lib = {
            let input_ass = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(vertex_input_desc.primitive_topology)
                .primitive_restart_enable(vertex_input_desc.primitive_restart);
            let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();

            create_library(device, GPF::VERTEX_INPUT_INTERFACE, |desc| {
                desc.vertex_input_state(&vertex_input)
                    .input_assembly_state(&input_ass)
            })?
        };

        let vertex_shader_lib = {
            let mut shader_module =
                vk::ShaderModuleCreateInfo::default().code(vs_bytes.as_binary());
            let shader_stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .name(c"main")
                .push_next(&mut shader_module);
            let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
                .dynamic_states(&vertex_shader_desc.dynamic_state);
            let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
                .line_width(vertex_shader_desc.line_width)
                .polygon_mode(vertex_shader_desc.polygon_mode)
                .cull_mode(vertex_shader_desc.cull_mode)
                .front_face(vertex_shader_desc.front_face);
            let viewport_state = vk::PipelineViewportStateCreateInfo::default()
                .viewport_count(vertex_shader_desc.viewport_count)
                .scissor_count(vertex_shader_desc.scissot_count);

            create_library(device, GPF::PRE_RASTERIZATION_SHADERS, |desc| {
                desc.layout(pipeline_layout)
                    .stages(std::slice::from_ref(&shader_stage))
                    .dynamic_state(&dynamic_state)
                    .viewport_state(&viewport_state)
                    .rasterization_state(&rasterization_state)
            })?
        };

        let fragment_shader_lib = {
            let mut shader_module =
                vk::ShaderModuleCreateInfo::default().code(fs_bytes.as_binary());
            let shader_stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .name(c"main")
                .push_next(&mut shader_module);

            let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default();

            create_library(device, GPF::FRAGMENT_SHADER, |desc| {
                desc.layout(pipeline_layout)
                    .stages(std::slice::from_ref(&shader_stage))
                    .depth_stencil_state(&depth_stencil_state)
            })?
        };

        let fragment_output_lib = {
            let color_attachment_formats = [fragment_output_desc.surface_format];
            let mut dyn_render = vk::PipelineRenderingCreateInfo::default()
                .color_attachment_formats(&color_attachment_formats);

            let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);

            create_library(device, GPF::FRAGMENT_OUTPUT_INTERFACE, |desc| {
                desc.multisample_state(&multisample_state)
                    .push_next(&mut dyn_render)
            })?
        };

        let pipeline = Self::link_libraries(
            device,
            &pipeline_layout,
            &vertex_input_lib,
            &vertex_shader_lib,
            &fragment_shader_lib,
            &fragment_output_lib,
        )?;

        Ok(Self {
            device: device.clone(),
            layout: pipeline_layout,
            pipeline,
            vertex_input_lib,
            vertex_shader_lib,
            fragment_shader_lib,
            fragment_output_lib,
        })
    }

    pub fn reload_vertex_lib(
        &mut self,
        shader_compiler: &ShaderCompiler,
        shader_path: impl AsRef<Path>,
    ) -> Result<()> {
        let vs_bytes = shader_compiler.compile(shader_path, shaderc::ShaderKind::Vertex)?;

        unsafe { self.device.destroy_pipeline(self.vertex_shader_lib, None) };

        let mut shader_module = vk::ShaderModuleCreateInfo::default().code(vs_bytes.as_binary());
        let shader_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .name(c"main")
            .push_next(&mut shader_module);
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);
        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
            .line_width(1.0)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE);
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let vertex_shader_lib = create_library(
            &self.device,
            vk::GraphicsPipelineLibraryFlagsEXT::PRE_RASTERIZATION_SHADERS,
            |desc| {
                desc.layout(self.layout)
                    .stages(std::slice::from_ref(&shader_stage))
                    .dynamic_state(&dynamic_state)
                    .viewport_state(&viewport_state)
                    .rasterization_state(&rasterization_state)
            },
        )?;

        self.vertex_shader_lib = vertex_shader_lib;

        Ok(())
    }

    pub fn reload_fragment_lib(
        &mut self,
        shader_compiler: &ShaderCompiler,
        shader_path: impl AsRef<Path>,
    ) -> Result<()> {
        let fs_bytes = shader_compiler.compile(shader_path, shaderc::ShaderKind::Fragment)?;

        unsafe { self.device.destroy_pipeline(self.fragment_shader_lib, None) };

        let mut shader_module = vk::ShaderModuleCreateInfo::default().code(fs_bytes.as_binary());
        let shader_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .name(c"main")
            .push_next(&mut shader_module);

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default();

        let fragment_shader_lib = create_library(
            &self.device,
            vk::GraphicsPipelineLibraryFlagsEXT::FRAGMENT_SHADER,
            |desc| {
                desc.layout(self.layout)
                    .stages(std::slice::from_ref(&shader_stage))
                    .depth_stencil_state(&depth_stencil_state)
            },
        )?;

        self.fragment_shader_lib = fragment_shader_lib;

        Ok(())
    }

    pub fn link(&mut self) -> Result<()> {
        unsafe { self.device.destroy_pipeline(self.pipeline, None) };
        self.pipeline = Self::link_libraries(
            &self.device,
            &self.layout,
            &self.vertex_input_lib,
            &self.vertex_shader_lib,
            &self.fragment_shader_lib,
            &self.fragment_output_lib,
        )?;

        Ok(())
    }

    fn link_libraries(
        device: &ash::Device,
        layout: &vk::PipelineLayout,
        vertex_input_lib: &vk::Pipeline,
        vertex_shader_lib: &vk::Pipeline,
        fragment_shader_lib: &vk::Pipeline,
        fragment_output_lib: &vk::Pipeline,
    ) -> Result<vk::Pipeline> {
        let libraries = [
            *vertex_input_lib,
            *vertex_shader_lib,
            *fragment_shader_lib,
            *fragment_output_lib,
        ];
        let pipeline = {
            let mut linking_info =
                vk::PipelineLibraryCreateInfoKHR::default().libraries(&libraries);
            let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
                .flags(vk::PipelineCreateFlags::LINK_TIME_OPTIMIZATION_EXT)
                .layout(*layout)
                .push_next(&mut linking_info);
            let pipeline = unsafe {
                device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
            };
            pipeline.map_err(|(_, err)| err)?[0]
        };

        Ok(pipeline)
    }
}

impl Drop for RenderPipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.vertex_input_lib, None);
            self.device.destroy_pipeline(self.vertex_shader_lib, None);
            self.device.destroy_pipeline(self.fragment_shader_lib, None);
            self.device.destroy_pipeline(self.fragment_output_lib, None);
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

fn create_library<'a, F>(
    device: &ash::Device,
    kind: vk::GraphicsPipelineLibraryFlagsEXT,
    f: F,
) -> VkResult<vk::Pipeline>
where
    F: FnOnce(vk::GraphicsPipelineCreateInfo<'a>) -> vk::GraphicsPipelineCreateInfo<'a>,
{
    let mut library_type = vk::GraphicsPipelineLibraryCreateInfoEXT::default().flags(kind);
    let pipeline = unsafe {
        let pipeline_info = vk::GraphicsPipelineCreateInfo::default().flags(
            vk::PipelineCreateFlags::LIBRARY_KHR
                | vk::PipelineCreateFlags::RETAIN_LINK_TIME_OPTIMIZATION_INFO_EXT,
        );

        // WARN: `let` introduces implicit copy on the struct that contains pointers
        let pipeline_info = f(pipeline_info).push_next(&mut library_type);

        device.create_graphics_pipelines(
            vk::PipelineCache::null(),
            std::slice::from_ref(&pipeline_info),
            None,
        )
    };

    Ok(pipeline.map_err(|(_, err)| err)?[0])
}

slotmap::new_key_type! {
    pub struct RenderHandle;
    pub struct ComputeHandle;
}

pub struct PipelineArena {
    pub render: RenderArena,
    pub compute: ComputeArena,
    pub path_mapping: AHashMap<PathBuf, AHashSet<Either<RenderHandle, ComputeHandle>>>,
    pub shader_compiler: ShaderCompiler,
    file_watcher: Watcher,
    device: Arc<RawDevice>,
}

impl PipelineArena {
    pub fn new(device: &Device, file_watcher: Watcher) -> Result<Self> {
        Ok(Self {
            render: RenderArena {
                pipelines: SlotMap::with_key(),
            },
            compute: ComputeArena {
                pipelines: SlotMap::with_key(),
            },
            shader_compiler: ShaderCompiler::new(&file_watcher)?,
            file_watcher,
            path_mapping: AHashMap::new(),
            device: device.device.clone(),
        })
    }

    pub fn create_compute_pipeline(
        &mut self,
        shader_path: impl AsRef<Path>,
        push_constant_ranges: &[vk::PushConstantRange],
        descriptor_set_layouts: &[vk::DescriptorSetLayout],
    ) -> Result<ComputeHandle> {
        let path = shader_path.as_ref().canonicalize()?;
        {
            self.file_watcher.watch_file(&path)?;
            let mut mapping = self.file_watcher.include_mapping.lock();
            mapping
                .entry(path.clone())
                .or_default()
                .insert(ShaderSource {
                    path: path.clone(),
                    kind: ShaderKind::Compute,
                });
        }
        let pipeline = ComputePipeline::new(
            &self.device,
            &self.shader_compiler,
            &path,
            push_constant_ranges,
            descriptor_set_layouts,
        )?;
        let handle = self.compute.pipelines.insert(pipeline);
        self.path_mapping
            .entry(path)
            .or_default()
            .insert(Either::Right(handle));
        Ok(handle)
    }

    pub fn create_render_pipeline(
        &mut self,
        vertex_input_desc: &VertexInputDesc,
        vertex_shader_desc: &VertexShaderDesc,
        fragment_shader_desc: &FragmentShaderDesc,
        fragment_output_desc: &FragmentOutputDesc,
        push_constant_ranges: &[vk::PushConstantRange],
        descriptor_set_layouts: &[vk::DescriptorSetLayout],
    ) -> Result<RenderHandle> {
        let vs_path = vertex_shader_desc.shader_path.canonicalize()?;
        let fs_path = fragment_shader_desc.shader_path.canonicalize()?;
        for (path, kind) in [
            (vs_path.clone(), ShaderKind::Vertex),
            (fs_path.clone(), ShaderKind::Fragment),
        ] {
            self.file_watcher.watch_file(&path)?;
            let mut mapping = self.file_watcher.include_mapping.lock();
            mapping
                .entry(path.clone())
                .or_default()
                .insert(ShaderSource { path, kind });
        }
        let pipeline = RenderPipeline::new(
            &self.device,
            &self.shader_compiler,
            vertex_input_desc,
            vertex_shader_desc,
            fragment_shader_desc,
            fragment_output_desc,
            push_constant_ranges,
            descriptor_set_layouts,
        )?;
        let handle = self.render.pipelines.insert(pipeline);
        self.path_mapping
            .entry(vs_path)
            .or_default()
            .insert(Either::Left(handle));
        self.path_mapping
            .entry(fs_path)
            .or_default()
            .insert(Either::Left(handle));
        Ok(handle)
    }

    pub fn get_pipeline<H: Handle>(&self, handle: H) -> &H::Pipeline {
        handle.get_pipeline(self)
    }

    pub fn get_pipeline_mut<H: Handle>(&mut self, handle: H) -> &mut H::Pipeline {
        handle.get_pipeline_mut(self)
    }
}

pub struct RenderArena {
    pub pipelines: SlotMap<RenderHandle, RenderPipeline>,
}

pub struct ComputeArena {
    pub pipelines: SlotMap<ComputeHandle, ComputePipeline>,
}

pub trait Handle {
    type Pipeline;
    fn get_pipeline(self, arena: &PipelineArena) -> &Self::Pipeline;
    fn get_pipeline_mut(self, arena: &mut PipelineArena) -> &mut Self::Pipeline;
}

impl Handle for RenderHandle {
    type Pipeline = RenderPipeline;

    fn get_pipeline(self, arena: &PipelineArena) -> &Self::Pipeline {
        &arena.render.pipelines[self]
    }

    fn get_pipeline_mut(self, arena: &mut PipelineArena) -> &mut Self::Pipeline {
        &mut arena.render.pipelines[self]
    }
}

impl Handle for ComputeHandle {
    type Pipeline = ComputePipeline;

    fn get_pipeline(self, arena: &PipelineArena) -> &Self::Pipeline {
        &arena.compute.pipelines[self]
    }

    fn get_pipeline_mut(self, arena: &mut PipelineArena) -> &mut Self::Pipeline {
        &mut arena.compute.pipelines[self]
    }
}
