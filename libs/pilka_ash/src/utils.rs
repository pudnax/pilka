use crate::device::VkDevice;
use ash::version::DeviceV1_0;
use ash::vk;

pub fn find_memory_type_index(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let best_suitable_index =
        find_memorytype_index_f(memory_req, memory_prop, flags, |property_flags, flags| {
            property_flags == flags
        });
    if best_suitable_index.is_some() {
        return best_suitable_index;
    }
    find_memorytype_index_f(memory_req, memory_prop, flags, |property_flags, flags| {
        property_flags & flags == flags
    })
}

fn find_memorytype_index_f<F: Fn(vk::MemoryPropertyFlags, vk::MemoryPropertyFlags) -> bool>(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
    f: F,
) -> Option<u32> {
    let mut memory_type_bits = memory_req.memory_type_bits;
    for (index, ref memory_type) in memory_prop.memory_types.iter().enumerate() {
        if memory_type_bits & 1 == 1 && f(memory_type.property_flags, flags) {
            return Some(index as u32);
        }
        memory_type_bits >>= 1;
    }
    None
}

#[allow(clippy::clippy::too_many_arguments)]
pub fn set_image_layout(
    device: &VkDevice,
    cmd_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    subresource_range: vk::ImageSubresourceRange,
    src_stage_mask: vk::PipelineStageFlags,
    dst_stage_mask: vk::PipelineStageFlags,
) {
    let mut image_memory_barrier = vk::ImageMemoryBarrier::builder()
        .old_layout(old_layout)
        .new_layout(new_layout)
        .image(image)
        .subresource_range(subresource_range);

    use vk::{AccessFlags, ImageLayout};
    image_memory_barrier.src_access_mask = match old_layout {
        ImageLayout::UNDEFINED => AccessFlags::empty(),
        ImageLayout::PREINITIALIZED => AccessFlags::HOST_WRITE,
        ImageLayout::COLOR_ATTACHMENT_OPTIMAL => AccessFlags::COLOR_ATTACHMENT_WRITE,
        ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL => {
            AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
        }
        ImageLayout::TRANSFER_SRC_OPTIMAL => AccessFlags::TRANSFER_READ,
        ImageLayout::TRANSFER_DST_OPTIMAL => AccessFlags::TRANSFER_WRITE,
        ImageLayout::SHADER_READ_ONLY_OPTIMAL => AccessFlags::SHADER_READ,
        _ => AccessFlags::empty(),
    };

    image_memory_barrier.dst_access_mask = match new_layout {
        ImageLayout::TRANSFER_DST_OPTIMAL => AccessFlags::TRANSFER_WRITE,
        ImageLayout::TRANSFER_SRC_OPTIMAL => AccessFlags::TRANSFER_READ,
        ImageLayout::COLOR_ATTACHMENT_OPTIMAL => AccessFlags::COLOR_ATTACHMENT_WRITE,
        ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL => {
            image_memory_barrier.dst_access_mask | AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
        }
        ImageLayout::SHADER_READ_ONLY_OPTIMAL => {
            if image_memory_barrier.src_access_mask.is_empty() {
                image_memory_barrier.src_access_mask =
                    AccessFlags::HOST_WRITE | AccessFlags::TRANSFER_WRITE;
            }
            AccessFlags::SHADER_READ
        }
        _ => AccessFlags::empty(),
    };

    let image_barriers = [image_memory_barrier.build()];
    unsafe {
        device.cmd_pipeline_barrier(
            cmd_buffer,
            src_stage_mask,
            dst_stage_mask,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &image_barriers,
        );
    }
}

pub fn set_image_layout_all_commands(
    device: &VkDevice,
    cmd_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    subresource_range: vk::ImageSubresourceRange,
) {
    set_image_layout(
        device,
        cmd_buffer,
        image,
        old_layout,
        new_layout,
        subresource_range,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::ALL_COMMANDS,
    );
}

/// # Safety
/// Until you're using it on not ZST or DST it's fine
pub unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    std::slice::from_raw_parts((p as *const T) as *const _, std::mem::size_of::<T>())
}

pub fn find_memorytype_index(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_prop.memory_types[..memory_prop.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_req.memory_type_bits != 0
                && (memory_type.property_flags & flags) == flags
        })
        .map(|(index, _memory_type)| index as _)
}

pub fn size_of_slice<T: Sized>(slice: &[T]) -> usize {
    std::mem::size_of::<T>() * slice.len()
}

pub fn make_spirv(data: &[u8]) -> std::borrow::Cow<[u32]> {
    const MAGIC_NUMBER: u32 = 0x723_0203;

    assert_eq!(
        data.len() % std::mem::size_of::<u32>(),
        0,
        "data size is not a multiple of 4"
    );

    let words = if data.as_ptr().align_offset(std::mem::align_of::<u32>()) == 0 {
        let (pre, words, post) = unsafe { data.align_to::<u32>() };
        debug_assert!(pre.is_empty());
        debug_assert!(post.is_empty());
        std::borrow::Cow::from(words)
    } else {
        let mut words = vec![0u32; data.len() / std::mem::size_of::<u32>()];
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), words.as_mut_ptr() as *mut u8, data.len());
        }
        std::borrow::Cow::from(words)
    };
    assert_eq!(
        words[0], MAGIC_NUMBER,
        "wrong magic word {:x}. Make sure you are using a binary SPIRV file.",
        words[0]
    );
    words
}

#[macro_export]
macro_rules! any {
    ($x:expr, $($y:expr),+ $(,)?) => {
        {
            false $(|| $x == $y)+
        }
    };
}

#[macro_export]
macro_rules! include_str_from_outdir {
    ($t: literal) => {
        include_str!(concat!(env!("OUT_DIR"), $t))
    };
}

#[macro_export]
macro_rules! include_bytes_from_outdir {
    ($t: literal) => {
        include_bytes!(concat!(env!("OUT_DIR"), $t))
    };
}

#[macro_export]
macro_rules! include_spirv_from_outdir {
    ($t: literal) => {
        crate::utils::make_spirv(crate::include_bytes_from_outdir!($t))
    };
}

#[macro_export]
macro_rules! tuple_as {
    ($e:expr, ( $T0:ty, $T1:ty, $T2:ty, $T3:ty, $T4:ty, $T5:ty ) ) => {
        (
            $e.0 as $T0,
            $e.1 as $T1,
            $e.2 as $T2,
            $e.3 as $T3,
            $e.4 as $T4,
            $e.5 as $T5,
        )
    };
    ($e:expr, ( $T0:ty, $T1:ty, $T2:ty, $T3:ty, $T4:ty ) ) => {
        (
            $e.0 as $T0,
            $e.1 as $T1,
            $e.2 as $T2,
            $e.3 as $T3,
            $e.4 as $T4,
        )
    };
    ($e:expr, ( $T0:ty, $T1:ty, $T2:ty, $T3:ty ) ) => {
        ($e.0 as $T0, $e.1 as $T1, $e.2 as $T2, $e.3 as $T3)
    };
    ($e:expr, ( $T0:ty, $T1:ty, $T2:ty ) ) => {
        ($e.0 as $T0, $e.1 as $T1, $e.2 as $T2)
    };
    ($e:expr, ( $T0:ty, $T1:ty ) ) => {
        ($e.0 as $T0, $e.1 as $T1)
    };
    ($e:expr, ( $T0:ty, ) ) => {
        ($e.0 as $T0,)
    };
}
