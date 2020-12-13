use ash::vk;

pub struct VkImage {
    image: vk::Image,
    image_memory: vk::DeviceMemory,
    image_view: vk::ImageView,
    extent: vk::Extent2D,
}
