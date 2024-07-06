use anyhow::Result;
use gpu_alloc::{GpuAllocator, MemoryBlock, Request, UsageFlags};
use gpu_alloc_ash::AshMemoryDevice;
use parking_lot::Mutex;
use std::{ffi::CStr, mem::ManuallyDrop, sync::Arc};

use ash::{
    khr,
    prelude::VkResult,
    vk::{self, DeviceMemory},
};

use crate::{align_to, ManagedImage, COLOR_SUBRESOURCE_MASK};

pub struct Device {
    pub physical_device: vk::PhysicalDevice,
    pub memory_properties: vk::PhysicalDeviceMemoryProperties,
    pub device_properties: vk::PhysicalDeviceProperties,
    pub descriptor_indexing_props: vk::PhysicalDeviceDescriptorIndexingProperties<'static>,
    pub command_pool: vk::CommandPool,
    pub main_queue_family_idx: u32,
    pub transfer_queue_family_idx: u32,
    pub allocator: Arc<Mutex<GpuAllocator<DeviceMemory>>>,
    pub device: Arc<RawDevice>,
    pub ext: Arc<DeviceExt>,
}

pub struct DeviceExt {
    pub dynamic_rendering: khr::dynamic_rendering::Device,
}

impl std::ops::Deref for Device {
    type Target = RawDevice;

    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

#[derive(Clone)]
pub struct RawDevice {
    inner: ash::Device,
}

impl RawDevice {
    pub fn new(inner: ash::Device) -> Self {
        Self { inner }
    }

    pub fn get_buffer_address(&self, buffer: vk::Buffer) -> u64 {
        unsafe {
            self.get_buffer_device_address(&vk::BufferDeviceAddressInfo::default().buffer(buffer))
        }
    }

    pub fn create_2d_view(&self, image: &vk::Image, format: vk::Format) -> VkResult<vk::ImageView> {
        let view = unsafe {
            self.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .image(*image)
                    .format(format)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1),
                    ),
                None,
            )?
        };
        Ok(view)
    }

    pub fn alloc_memory(
        &self,
        allocator: &mut GpuAllocator<DeviceMemory>,
        memory_reqs: vk::MemoryRequirements,
        usage: UsageFlags,
    ) -> Result<gpu_alloc::MemoryBlock<DeviceMemory>, gpu_alloc::AllocationError> {
        let memory_block = unsafe {
            allocator.alloc(
                AshMemoryDevice::wrap(self),
                Request {
                    size: memory_reqs.size,
                    align_mask: memory_reqs.alignment - 1,
                    usage: usage | UsageFlags::DEVICE_ADDRESS,
                    memory_types: memory_reqs.memory_type_bits,
                },
            )
        };
        memory_block
    }

    pub fn one_time_submit(
        &self,
        command_pool: &vk::CommandPool,
        queue: &vk::Queue,
        callbk: impl FnOnce(&Self, vk::CommandBuffer),
    ) -> VkResult<()> {
        let fence = unsafe { self.create_fence(&vk::FenceCreateInfo::default(), None)? };
        let command_buffer = unsafe {
            self.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(*command_pool)
                    .command_buffer_count(1)
                    .level(vk::CommandBufferLevel::PRIMARY),
            )?[0]
        };

        unsafe {
            self.begin_command_buffer(
                command_buffer,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;

            callbk(self, command_buffer);

            self.end_command_buffer(command_buffer)?;

            let submit_info =
                vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&command_buffer));

            self.queue_submit(*queue, &[submit_info], fence)?;
            self.wait_for_fences(&[fence], true, u64::MAX)?;

            self.destroy_fence(fence, None);
            self.free_command_buffers(*command_pool, &[command_buffer]);
        }

        Ok(())
    }

    pub fn blit_image(
        &self,
        command_buffer: &vk::CommandBuffer,
        src_image: &vk::Image,
        src_extent: vk::Extent2D,
        src_orig_layout: vk::ImageLayout,
        dst_image: &vk::Image,
        dst_extent: vk::Extent2D,
        dst_orig_layout: vk::ImageLayout,
    ) {
        let src_barrier = vk::ImageMemoryBarrier2::default()
            .subresource_range(COLOR_SUBRESOURCE_MASK)
            .image(*src_image)
            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_access_mask(vk::AccessFlags2::MEMORY_READ)
            .old_layout(src_orig_layout)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        let dst_barrier = vk::ImageMemoryBarrier2::default()
            .subresource_range(COLOR_SUBRESOURCE_MASK)
            .image(*dst_image)
            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_access_mask(vk::AccessFlags2::MEMORY_WRITE)
            .old_layout(dst_orig_layout)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL);
        let image_memory_barriers = &[src_barrier, dst_barrier];
        let dependency_info =
            vk::DependencyInfo::default().image_memory_barriers(image_memory_barriers);
        unsafe { self.cmd_pipeline_barrier2(*command_buffer, &dependency_info) };

        let src_offsets = [
            vk::Offset3D { x: 0, y: 0, z: 0 },
            vk::Offset3D {
                x: src_extent.width as _,
                y: src_extent.height as _,
                z: 1,
            },
        ];
        let dst_offsets = [
            vk::Offset3D { x: 0, y: 0, z: 0 },
            vk::Offset3D {
                x: dst_extent.width as _,
                y: dst_extent.height as _,
                z: 1,
            },
        ];
        let subresource_layer = vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_array_layer: 0,
            layer_count: 1,
            mip_level: 0,
        };
        let regions = [vk::ImageBlit2::default()
            .src_offsets(src_offsets)
            .dst_offsets(dst_offsets)
            .src_subresource(subresource_layer)
            .dst_subresource(subresource_layer)];
        let blit_info = vk::BlitImageInfo2::default()
            .src_image(*src_image)
            .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .dst_image(*dst_image)
            .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .regions(&regions)
            .filter(vk::Filter::NEAREST);
        unsafe { self.cmd_blit_image2(*command_buffer, &blit_info) };

        let src_barrier = src_barrier
            .src_access_mask(vk::AccessFlags2::MEMORY_READ)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(src_orig_layout);
        let dst_barrier = dst_barrier
            .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(match dst_orig_layout {
                vk::ImageLayout::UNDEFINED => vk::ImageLayout::GENERAL,
                _ => dst_orig_layout,
            });
        let image_memory_barriers = &[src_barrier, dst_barrier];
        let dependency_info =
            vk::DependencyInfo::default().image_memory_barriers(image_memory_barriers);
        unsafe { self.cmd_pipeline_barrier2(*command_buffer, &dependency_info) };
    }
}

impl std::ops::Deref for RawDevice {
    type Target = ash::Device;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Device {
    pub fn one_time_submit(
        &self,
        queue: &vk::Queue,
        callbk: impl FnOnce(&RawDevice, vk::CommandBuffer),
    ) -> VkResult<()> {
        self.device
            .one_time_submit(&self.command_pool, queue, callbk)
    }

    pub fn alloc_memory(
        &self,
        memory_reqs: vk::MemoryRequirements,
        usage: UsageFlags,
    ) -> Result<gpu_alloc::MemoryBlock<DeviceMemory>, gpu_alloc::AllocationError> {
        let mut allocator = self.allocator.lock();
        self.device.alloc_memory(&mut allocator, memory_reqs, usage)
    }

    pub fn capture_image_data(
        &self,
        queue: &vk::Queue,
        src_image: &vk::Image,
        extent: vk::Extent2D,
        callback: impl FnOnce(ManagedImage),
    ) -> Result<()> {
        let dst_image = ManagedImage::new(
            self,
            &vk::ImageCreateInfo::default()
                .extent(vk::Extent3D {
                    width: align_to(extent.width, 2),
                    height: align_to(extent.height, 2),
                    depth: 1,
                })
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_SRGB)
                .usage(vk::ImageUsageFlags::TRANSFER_DST)
                .samples(vk::SampleCountFlags::TYPE_1)
                .mip_levels(1)
                .array_layers(1)
                .tiling(vk::ImageTiling::LINEAR),
            UsageFlags::DOWNLOAD,
        )?;

        self.one_time_submit(queue, |device, command_buffer| {
            device.blit_image(
                &command_buffer,
                src_image,
                extent,
                vk::ImageLayout::PRESENT_SRC_KHR,
                &dst_image.image,
                extent,
                vk::ImageLayout::UNDEFINED,
            );
        })?;

        callback(dst_image);

        Ok(())
    }

    pub fn create_host_buffer(
        &self,
        size: u64,
        usage: vk::BufferUsageFlags,
        memory_usage: gpu_alloc::UsageFlags,
    ) -> Result<HostBuffer> {
        let buffer = unsafe {
            self.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS),
                None,
            )?
        };
        let mem_requirements = unsafe { self.get_buffer_memory_requirements(buffer) };

        let mut memory =
            self.alloc_memory(mem_requirements, memory_usage | UsageFlags::HOST_ACCESS)?;
        unsafe { self.bind_buffer_memory(buffer, *memory.memory(), memory.offset()) }?;

        let address = unsafe {
            self.get_buffer_device_address(&vk::BufferDeviceAddressInfo::default().buffer(buffer))
        };

        let ptr = unsafe {
            memory.map(
                AshMemoryDevice::wrap(self),
                memory.offset(),
                memory.size() as usize,
            )?
        };
        let data = unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr(), size as _) };

        Ok(HostBuffer {
            address,
            size,
            buffer,
            memory: ManuallyDrop::new(memory),
            data,
            device: self.device.clone(),
            allocator: self.allocator.clone(),
        })
    }

    pub fn create_host_buffer_typed<T>(
        &self,
        usage: vk::BufferUsageFlags,
        memory_usage: gpu_alloc::UsageFlags,
    ) -> Result<HostBufferTyped<T>> {
        let byte_size = (size_of::<T>()) as vk::DeviceSize;
        let buffer = unsafe {
            self.device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(byte_size)
                    .usage(usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS),
                None,
            )?
        };
        let mem_requirements = unsafe { self.get_buffer_memory_requirements(buffer) };

        let mut memory =
            self.alloc_memory(mem_requirements, memory_usage | UsageFlags::HOST_ACCESS)?;
        unsafe { self.bind_buffer_memory(buffer, *memory.memory(), memory.offset()) }?;

        let address = unsafe {
            self.get_buffer_device_address(&vk::BufferDeviceAddressInfo::default().buffer(buffer))
        };

        let ptr = unsafe {
            memory.map(
                AshMemoryDevice::wrap(&self.device),
                memory.offset(),
                memory.size() as usize,
            )?
        };
        let ptr = unsafe { &mut *ptr.as_ptr().cast::<T>() };

        Ok(HostBufferTyped {
            address,
            buffer,
            memory: ManuallyDrop::new(memory),
            data: ptr,
            device: self.device.clone(),
            allocator: self.allocator.clone(),
        })
    }

    pub fn get_info(&self) -> RendererInfo {
        RendererInfo {
            device_name: self.get_device_name().unwrap().to_string(),
            device_type: self.get_device_type().to_string(),
            vendor_name: self.get_vendor_name().to_string(),
        }
    }
    pub fn get_device_name(&self) -> Result<&str, std::str::Utf8Error> {
        unsafe { CStr::from_ptr(self.device_properties.device_name.as_ptr()) }.to_str()
    }
    pub fn get_device_type(&self) -> &str {
        match self.device_properties.device_type {
            vk::PhysicalDeviceType::CPU => "CPU",
            vk::PhysicalDeviceType::INTEGRATED_GPU => "INTEGRATED_GPU",
            vk::PhysicalDeviceType::DISCRETE_GPU => "DISCRETE_GPU",
            vk::PhysicalDeviceType::VIRTUAL_GPU => "VIRTUAL_GPU",
            _ => "OTHER",
        }
    }
    pub fn get_vendor_name(&self) -> &str {
        match self.device_properties.vendor_id {
            0x1002 => "AMD",
            0x1010 => "ImgTec",
            0x10DE => "NVIDIA Corporation",
            0x13B5 => "ARM",
            0x5143 => "Qualcomm",
            0x8086 => "INTEL Corporation",
            _ => "Unknown vendor",
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
            {
                let mut allocator = self.allocator.lock();
                allocator.cleanup(AshMemoryDevice::wrap(&self.device));
            }
            self.device.destroy_device(None);
        }
    }
}

pub struct HostBuffer {
    pub address: u64,
    pub size: u64,
    pub buffer: vk::Buffer,
    pub memory: ManuallyDrop<MemoryBlock<DeviceMemory>>,
    pub data: &'static mut [u8],
    device: Arc<RawDevice>,
    allocator: Arc<Mutex<GpuAllocator<DeviceMemory>>>,
}

impl std::ops::Deref for HostBuffer {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl std::ops::DerefMut for HostBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}

impl Drop for HostBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
            {
                let mut allocator = self.allocator.lock();
                let memory = ManuallyDrop::take(&mut self.memory);
                allocator.dealloc(AshMemoryDevice::wrap(&self.device), memory);
            }
        }
    }
}

pub struct HostBufferTyped<T: 'static> {
    pub address: u64,
    pub buffer: vk::Buffer,
    pub memory: ManuallyDrop<MemoryBlock<DeviceMemory>>,
    pub data: &'static mut T,
    device: Arc<RawDevice>,
    allocator: Arc<Mutex<GpuAllocator<DeviceMemory>>>,
}

impl<T> std::ops::Deref for HostBufferTyped<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<T> std::ops::DerefMut for HostBufferTyped<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}

impl<T> Drop for HostBufferTyped<T> {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
            {
                let mut allocator = self.allocator.lock();
                let memory = ManuallyDrop::take(&mut self.memory);
                allocator.dealloc(AshMemoryDevice::wrap(&self.device), memory);
            }
        }
    }
}

#[derive(Debug)]
pub struct RendererInfo {
    pub device_name: String,
    pub device_type: String,
    pub vendor_name: String,
}

impl std::fmt::Display for RendererInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Vendor name: {}", self.vendor_name)?;
        writeln!(f, "Device name: {}", self.device_name)?;
        writeln!(f, "Device type: {}", self.device_type)?;
        Ok(())
    }
}
