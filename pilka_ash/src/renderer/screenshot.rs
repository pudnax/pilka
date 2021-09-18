use std::mem::size_of;

use super::images::VkImage;
use crate::{
    pvk::{utils::return_aligned, VkCommandPool, VkDevice, VkDeviceProperties},
    VkQueue,
};
use ash::{
    prelude::VkResult,
    vk::{self, SubresourceLayout},
};

pub type Frame<'a> = (&'a [u8], ImageDimentions);

pub struct ScreenshotCtx<'a> {
    fence: vk::Fence,
    commbuf: vk::CommandBuffer,
    image: VkImage,
    blit_image: Option<VkImage>,
    extent: vk::Extent3D,
    format: vk::Format,
    pub data: &'a [u8],
}

impl<'a> ScreenshotCtx<'a> {
    pub fn init(
        device: &VkDevice,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        command_pool: &VkCommandPool,
        extent: vk::Extent2D,
        src_format: vk::Format,
        need2steps: bool,
    ) -> VkResult<Self> {
        let commandbuf_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool.pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let commbuf = unsafe { device.allocate_command_buffers(&commandbuf_allocate_info) }?[0];
        let fence = device.create_fence(false)?;
        let extent = vk::Extent3D {
            width: extent.width,
            height: return_aligned(extent.height, 2),
            depth: 1,
        };

        let dst_format = match src_format {
            vk::Format::B8G8R8A8_SRGB => vk::Format::R8G8B8A8_SRGB,
            vk::Format::B8G8R8A8_UNORM => vk::Format::R8G8B8_UNORM,
            vk::Format::B8G8R8A8_UINT => vk::Format::R8G8B8A8_UINT,
            vk::Format::B8G8R8A8_SINT => vk::Format::R8G8B8A8_SINT,
            vk::Format::B8G8R8A8_SNORM => vk::Format::R8G8B8A8_SNORM,
            vk::Format::B8G8R8A8_USCALED => vk::Format::R8G8B8A8_USCALED,
            vk::Format::B8G8R8A8_SSCALED => vk::Format::R8G8B8A8_SSCALED,
            _ => vk::Format::R8G8B8_UNORM,
        };
        let mut image_create_info = vk::ImageCreateInfo::builder()
            .format(dst_format)
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .array_layers(1)
            .mip_levels(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::LINEAR)
            .usage(vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let mut image_memory_flags = vk::MemoryPropertyFlags::HOST_VISIBLE
            | vk::MemoryPropertyFlags::HOST_CACHED
            | vk::MemoryPropertyFlags::HOST_COHERENT;

        let blit_image = if need2steps {
            let image = VkImage::new(
                device,
                memory_properties,
                &image_create_info,
                image_memory_flags,
            )?;
            image_create_info.tiling = vk::ImageTiling::OPTIMAL;
            image_create_info.usage =
                vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC;
            image_memory_flags = vk::MemoryPropertyFlags::DEVICE_LOCAL;
            Some(image)
        } else {
            None
        };

        let image = VkImage::new(
            device,
            memory_properties,
            &image_create_info,
            image_memory_flags,
        )?;
        let data = unsafe {
            let image = if let Some(ref blit_image) = blit_image {
                blit_image
            } else {
                &image
            };
            std::slice::from_raw_parts_mut(
                device.map_memory(
                    image.memory,
                    0,
                    image.memory_requirements.size,
                    vk::MemoryMapFlags::empty(),
                )? as *mut u8,
                image.memory_requirements.size as usize,
            )
        };

        Ok(Self {
            fence,
            commbuf,
            image,
            blit_image,
            data,
            extent,
            format: dst_format,
        })
    }

    pub fn destroy(&mut self, device: &VkDevice) {
        unsafe {
            if let Some(ref blit_image) = self.blit_image {
                device.unmap_memory(blit_image.memory);

                device.free_memory(blit_image.memory, None);
                device.destroy_image(blit_image.image, None);
            } else {
                device.unmap_memory(self.image.memory);
            }
            device.destroy_fence(self.fence, None);
            device.destroy_image(self.image.image, None);
            device.free_memory(self.image.memory, None);
        }
    }

    pub fn realloc(
        &mut self,
        device: &VkDevice,
        device_properties: &VkDeviceProperties,
        mut extent: vk::Extent3D,
    ) -> VkResult<()> {
        if self.extent != extent {
            extent.height = return_aligned(extent.height, 2);
            self.extent = extent;

            unsafe { device.destroy_image(self.image.image, None) };

            let mut image_create_info = vk::ImageCreateInfo::builder()
                .format(self.format)
                .image_type(vk::ImageType::TYPE_2D)
                .extent(extent)
                .array_layers(1)
                .mip_levels(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::LINEAR)
                .usage(vk::ImageUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let mut image_memory_flags = vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_CACHED
                | vk::MemoryPropertyFlags::HOST_COHERENT;

            if let Some(ref mut blit_image) = self.blit_image {
                unsafe { device.destroy_image(blit_image.image, None) };

                blit_image.image = unsafe { device.create_image(&image_create_info, None) }?;
                blit_image.memory_requirements =
                    unsafe { device.get_image_memory_requirements(blit_image.image) };
                image_create_info.tiling = vk::ImageTiling::OPTIMAL;
                image_create_info.usage =
                    vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC;
            }

            self.image.image = unsafe { device.create_image(&image_create_info, None)? };
            self.image.memory_requirements =
                unsafe { device.get_image_memory_requirements(self.image.image) };

            if (extent.width * extent.height * 4) as usize > self.data.len() {
                if let Some(ref mut blit_image) = self.blit_image {
                    unsafe { device.unmap_memory(blit_image.memory) };
                    unsafe { device.free_memory(blit_image.memory, None) }
                    blit_image.memory = device.alloc_memory(
                        &device_properties.memory,
                        blit_image.memory_requirements,
                        image_memory_flags,
                    )?;
                    image_memory_flags = vk::MemoryPropertyFlags::DEVICE_LOCAL;
                } else {
                    unsafe { device.unmap_memory(self.image.memory) };
                }
                unsafe { device.free_memory(self.image.memory, None) }

                self.image.memory = device.alloc_memory(
                    &device_properties.memory,
                    self.image.memory_requirements,
                    image_memory_flags,
                )?;

                self.data = unsafe {
                    let image = if let Some(ref blit_image) = self.blit_image {
                        blit_image
                    } else {
                        &self.image
                    };
                    std::slice::from_raw_parts_mut(
                        device.map_memory(
                            image.memory,
                            0,
                            image.memory_requirements.size,
                            vk::MemoryMapFlags::empty(),
                        )? as *mut u8,
                        image.memory_requirements.size as usize,
                    )
                }
            }

            if let Some(ref mut blit_image) = self.blit_image {
                unsafe { device.bind_image_memory(blit_image.image, blit_image.memory, 0) }?;
            }
            unsafe { device.bind_image_memory(self.image.image, self.image.memory, 0) }?;
        }

        Ok(())
    }

    pub fn capture_frame(
        &mut self,
        device: &VkDevice,
        device_properties: &VkDeviceProperties,
        present_image: ash::vk::Image,
        queue: &VkQueue,
    ) -> VkResult<Frame> {
        let copybuffer = self.commbuf;
        unsafe {
            device.reset_command_buffer(copybuffer, vk::CommandBufferResetFlags::RELEASE_RESOURCES)
        }?;
        let cmd_begininfo = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { device.begin_command_buffer(copybuffer, &cmd_begininfo) }?;

        let extent = self.extent;

        self.realloc(device, device_properties, extent)?;

        // let present_image = self.swapchain.images[self.command_pool.active_command];
        let copy_image = self.image.image;
        let dst_stage = vk::PipelineStageFlags::TRANSFER;
        let src_stage = vk::PipelineStageFlags::TRANSFER;

        let transport_barrier = |image, old_layout, new_layout| {
            device.set_image_layout(
                copybuffer, image, old_layout, new_layout, src_stage, dst_stage,
            )
        };

        use vk::ImageLayout;
        transport_barrier(
            present_image,
            ImageLayout::PRESENT_SRC_KHR,
            ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        transport_barrier(
            copy_image,
            ImageLayout::UNDEFINED,
            ImageLayout::TRANSFER_DST_OPTIMAL,
        );

        device.blit_image(copybuffer, present_image, copy_image, extent, self.extent);

        if let Some(ref blit_image) = self.blit_image {
            transport_barrier(
                blit_image.image,
                ImageLayout::UNDEFINED,
                ImageLayout::TRANSFER_DST_OPTIMAL,
            );

            transport_barrier(
                copy_image,
                ImageLayout::TRANSFER_DST_OPTIMAL,
                ImageLayout::TRANSFER_SRC_OPTIMAL,
            );

            device.copy_image(copybuffer, copy_image, blit_image.image, self.extent);
        }

        transport_barrier(
            if let Some(ref blit_image) = self.blit_image {
                blit_image.image
            } else {
                copy_image
            },
            ImageLayout::TRANSFER_DST_OPTIMAL,
            ImageLayout::GENERAL,
        );

        transport_barrier(
            present_image,
            ImageLayout::TRANSFER_SRC_OPTIMAL,
            ImageLayout::PRESENT_SRC_KHR,
        );

        unsafe { device.end_command_buffer(copybuffer) }?;
        let submit_commbuffers = [copybuffer];
        let submit_infos = [vk::SubmitInfo::builder()
            .command_buffers(&submit_commbuffers)
            .build()];
        unsafe { device.queue_submit(queue.queue, &submit_infos, self.fence) }?;
        unsafe { device.wait_for_fences(&[self.fence], true, u64::MAX) }?;
        unsafe { device.reset_fences(&[self.fence]) }?;

        let (subresource_layout, image_dimentions) = self.image_dimentions(device);

        Ok((&self.data[..subresource_layout.size as _], image_dimentions))
    }

    pub fn image_dimentions(&self, device: &VkDevice) -> (SubresourceLayout, ImageDimentions) {
        let image = if let Some(ref blit_image) = self.blit_image {
            blit_image
        } else {
            &self.image
        };
        let image_dimentions = ImageDimentions::new(
            self.extent.width as _,
            self.extent.height as _,
            image.memory_requirements.alignment as _,
        );
        let subresource_layout = unsafe {
            device.get_image_subresource_layout(
                image.image,
                vk::ImageSubresource {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    array_layer: 0,
                },
            )
        };
        (subresource_layout, image_dimentions)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImageDimentions {
    pub width: usize,
    pub height: usize,
    pub padded_bytes_per_row: usize,
    pub unpadded_bytes_per_row: usize,
}

impl ImageDimentions {
    fn new(width: usize, height: usize, align: usize) -> Self {
        let bytes_per_pixel = size_of::<[u8; 4]>();
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let padded_bytes_per_row_padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padded_bytes_per_row_padding;
        Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
        }
    }
}
