use color_eyre::*;

use crate::{SCREENSHOTS_FOLDER, SHADER_DUMP_FOLDER, SHADER_PATH};
use pilka_types::ImageDimentions;

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
    frame: Vec<u8>,
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
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let padded_bytes = image_dimentions.padded_bytes_per_row.try_into().unwrap();
        let unpadded_bytes = image_dimentions.unpadded_bytes_per_row.try_into().unwrap();
        let mut writer = encoder
            .write_header()?
            .into_stream_writer_with_size(unpadded_bytes)?;
        for chunk in frame
            .chunks(padded_bytes)
            .map(|chunk| &chunk[..unpadded_bytes])
        {
            writer.write_all(chunk)?;
        }
        writer.finish()?;
        eprintln!("Encode image: {:#.2?}", now.elapsed());
        Ok(())
    })
}

pub fn save_shaders<P: AsRef<Path>>(paths: &[P]) -> Result<()> {
    let dump_folder = std::path::Path::new(SHADER_DUMP_FOLDER);
    create_folder(dump_folder)?;
    let dump_folder =
        dump_folder.join(chrono::Local::now().format("%d-%m-%Y-%H-%M-%S").to_string());
    create_folder(&dump_folder)?;
    let dump_folder = dump_folder.join(SHADER_PATH);
    create_folder(&dump_folder)?;

    for path in paths {
        let to = dump_folder.join(
            path.as_ref()
                .strip_prefix(Path::new(SHADER_PATH).canonicalize()?)?,
        );
        if !to.exists() {
            std::fs::create_dir_all(&to.parent().unwrap().canonicalize()?)?;
            std::fs::File::create(&to)?;
        }
        std::fs::copy(path, &to)?;
        eprintln!("Saved: {}", &to.display());
    }

    Ok(())
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PushConstant {
    pub pos: [f32; 3],
    pub time: f32,
    pub wh: [f32; 2],
    pub mouse: [f32; 2],
    pub mouse_pressed: u32,
    pub frame: u32,
    pub time_delta: f32,
    pub record_period: f32,
}

impl PushConstant {
    pub fn as_slice(&self) -> &[u8] {
        unsafe { any_as_u8_slice(self) }
    }

    pub fn size() -> u32 {
        std::mem::size_of::<Self>() as _
    }
}

/// # Safety
/// Until you're using it on not ZST or DST it's fine
pub unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    std::slice::from_raw_parts((p as *const T) as *const _, std::mem::size_of::<T>())
}

impl Default for PushConstant {
    fn default() -> Self {
        Self {
            pos: [0.; 3],
            time: 0.,
            wh: [1920.0, 780.],
            mouse: [0.; 2],
            mouse_pressed: false as _,
            frame: 0,
            time_delta: 1. / 60.,
            record_period: 10.,
        }
    }
}

impl std::fmt::Display for PushConstant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let time = Duration::from_secs_f32(self.time);
        let time_delta = Duration::from_secs_f32(self.time_delta);
        write!(
            f,
            "position:\t{:?}\n\
             time:\t\t{:#.2?}\n\
             time delta:\t{:#.3?}, fps: {:#.2?}\n\
             width, height:\t{:?}\nmouse:\t\t{:.2?}\n\
             frame:\t\t{}\nrecord_period:\t{}\n",
            self.pos,
            time,
            time_delta,
            1. / self.time_delta,
            self.wh,
            self.mouse,
            self.frame,
            self.record_period
        )
    }
}
