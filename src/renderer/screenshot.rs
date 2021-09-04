use super::images::VkImage;
use pilka_ash::ash::{
    pilka_util::return_aligned, prelude::VkResult, vk, VkCommandPool, VkDevice, VkDeviceProperties,
};

pub struct ScreenshotCtx<'a> {
    pub fence: vk::Fence,
    pub commbuf: vk::CommandBuffer,
    pub image: VkImage,
    pub blit_image: Option<VkImage>,
    pub extent: vk::Extent3D,
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
}
