use std::io;
use std::path::Path;

pub fn create_folder<P: AsRef<Path>>(name: P) -> io::Result<()> {
    match std::fs::create_dir(name) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use pilka_ash::{vk, VkInstance};

    #[test]
    #[allow(unused_variables)]
    fn check_init() {
        let validation_layers = if cfg!(debug_assertions) {
            vec!["VK_LAYER_KHRONOS_validation\0"]
        } else {
            vec![]
        };
        let extention_names = vec![];
        let instance = VkInstance::new(&validation_layers, &extention_names).unwrap();

        let (device, device_properties, queues) = instance.create_device_and_queues(None).unwrap();

        let swapchain_loader = instance.create_swapchain_loader(&device);

        let present_complete_semaphore = device.create_semaphore();

        let rendering_complete_semaphore = device.create_semaphore();

        let pipeline_cache_create_info = vk::PipelineCacheCreateInfo::builder();
        let pipeline_cache =
            unsafe { device.create_pipeline_cache(&pipeline_cache_create_info, None) }.unwrap();
    }
}
