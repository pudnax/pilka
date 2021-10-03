use color_eyre::*;

use crate::{SCREENSHOTS_FOLDER, SHADER_DUMP_FOLDER, SHADER_PATH};
use pilka_ash::{ImageDimentions, PilkaRender};

use std::{
    fs::File,
    io,
    io::{BufWriter, Write},
    path::Path,
    time::{Duration, Instant},
};

pub fn create_folder<P: AsRef<Path>>(name: P) -> io::Result<()> {
    match std::fs::create_dir(name) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }

    Ok(())
}

pub struct Args {
    pub inner_size: Option<(u32, u32)>,
    pub record_time: Option<Duration>,
}

pub fn parse_args() -> Args {
    let mut inner_size = None;
    let mut record_time = None;
    for arg in std::env::args()
        .skip(1)
        .zip(std::env::args().skip(2))
        .step_by(2)
    {
        match arg.0.as_str() {
            "--record" => {
                record_time =
                    Some(Duration::from_secs_f32(arg.1.parse().expect(
                        &format!("Record duration should be a number: {}", arg.1)[..],
                    )))
            }
            "--size" => {
                let mut iter = arg
                    .1
                    .split('x')
                    .map(str::parse)
                    .map(|x| x.expect(&format!("Failed to parse window size: {}", arg.1)[..]));
                inner_size = Some((iter.next().unwrap(), iter.next().unwrap()));
            }
            _ => {}
        }
    }

    Args {
        record_time,
        inner_size,
    }
}

pub fn print_help() {
    println!("\n- `F1`:   Print help");
    println!("- `F2`:   Toggle play/pause");
    println!("- `F3`:   Pause and step back one frame");
    println!("- `F4`:   Pause and step forward one frame");
    println!("- `F5`:   Restart playback at frame 0 (`Time` and `Pos` = 0)");
    println!("- `F6`:   Print parameters");
    println!("- `F10`:  Save shaders");
    println!("- `F11`:  Take Screenshot");
    println!("- `F12`:  Start/Stop record video");
    println!("- `ESC`:  Exit the application");
    println!("- `Arrows`: Change `Pos`\n");
}

pub fn save_screenshot(
    frame: &'static [u8],
    image_dimentions: ImageDimentions,
) -> std::thread::JoinHandle<Result<()>> {
    std::thread::spawn(move || {
        let now = Instant::now();
        let screenshots_folder = Path::new(SCREENSHOTS_FOLDER);
        create_folder(screenshots_folder)?;
        let path = screenshots_folder.join(format!(
            "screenshot-{}.png",
            chrono::Local::now().format("%d-%m-%Y-%H-%M-%S")
        ));
        let file = File::create(path)?;
        let w = BufWriter::new(file);
        let mut encoder =
            png::Encoder::new(w, image_dimentions.width as _, image_dimentions.height as _);
        encoder.set_color(png::ColorType::RGBA);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()?
            .into_stream_writer_with_size(image_dimentions.unpadded_bytes_per_row);
        for chunk in frame
            .chunks(image_dimentions.padded_bytes_per_row)
            .map(|chunk| &chunk[..image_dimentions.unpadded_bytes_per_row])
        {
            writer.write_all(chunk)?;
        }
        writer.finish()?;
        eprintln!("Encode image: {:#?}", now.elapsed());
        Ok(())
    })
}

pub fn save_shaders(pilka: &PilkaRender) -> Result<()> {
    let dump_folder = std::path::Path::new(SHADER_DUMP_FOLDER);
    create_folder(dump_folder)?;
    let dump_folder =
        dump_folder.join(chrono::Local::now().format("%d-%m-%Y-%H-%M-%S").to_string());
    create_folder(&dump_folder)?;
    let dump_folder = dump_folder.join(SHADER_PATH);
    create_folder(&dump_folder)?;

    for path in pilka.shader_set.keys() {
        let to = dump_folder.join(path.strip_prefix(Path::new(SHADER_PATH).canonicalize()?)?);
        if !to.exists() {
            std::fs::create_dir_all(&to.parent().unwrap().canonicalize()?)?;
            std::fs::File::create(&to)?;
        }
        std::fs::copy(path, &to)?;
        eprintln!("Saved: {}", &to.display());
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
        // let pipeline_cache =
        //     unsafe { device.create_pipeline_cache(&pipeline_cache_create_info, None) }.unwrap();
    }
}
