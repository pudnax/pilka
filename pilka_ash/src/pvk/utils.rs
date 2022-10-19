use ash::vk;

pub fn find_memory_type_index(
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
        $crate::utils::make_spirv(crate::include_bytes_from_outdir!($t))
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
