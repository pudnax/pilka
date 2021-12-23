use color_eyre::*;

use crate::{SCREENSHOTS_FOLDER, SHADER_DUMP_FOLDER, SHADER_PATH};
use pilka_types::{ImageDimentions, ShaderFlavor};

use std::{
    ffi::OsStr,
    fs::File,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub fn print_help() {
    println!("\n- `F1`:   Print help");
    println!("- `F2`:   Toggle play/pause");
    println!("- `F3`:   Pause and step back one frame");
    println!("- `F4`:   Pause and step forward one frame");
    println!("- `F5`:   Restart playback at frame 0 (`Time` and `Pos` = 0)");
    println!("- `F6`:   Print parameters");
    println!("- `F7`:   Toggle profiler");
    println!("- `F8`:   Switch backend");
    println!("- `F10`:  Save shaders");
    println!("- `F11`:  Take Screenshot");
    println!("- `F12`:  Start/Stop record video");
    println!("- `ESC`:  Exit the application");
    println!("- `Arrows`: Change `Pos`\n");
}

pub fn create_folder<P: AsRef<Path>>(name: P) -> io::Result<()> {
    match std::fs::create_dir(name) {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
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
        let padded_bytes = image_dimentions.padded_bytes_per_row as _;
        let unpadded_bytes = image_dimentions.unpadded_bytes_per_row as _;
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
    let dump_folder = Path::new(SHADER_DUMP_FOLDER);
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
            File::create(&to)?;
        }
        std::fs::copy(path, &to)?;
        eprintln!("Saved: {}", &to.display());
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct PilkaSpec {
    pub frag: ShaderInfo,
    pub comp: ShaderInfo,
    pub vert: ShaderInfo,
    pub glsl_prelude: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ShaderInfo {
    pub path: PathBuf,
    pub ty: ShaderFlavor,
}

pub fn parse_folder(folder: &str) -> Result<PilkaSpec, Box<dyn std::error::Error>> {
    let mut frag_si = None;
    let mut comp_si = None;
    let mut vert_si = None;
    let mut prelude = None;
    let shader_dir = PathBuf::new().join(folder);
    for path in shader_dir.read_dir()? {
        let path = path?.path();

        let ty = match path.extension().and_then(OsStr::to_str) {
            Some("wgsl") => ShaderFlavor::Wgsl,
            Some("glsl" | "frag" | "comp" | "vert") => ShaderFlavor::Glsl,
            _ => {
                println!("This file have been ignored: {}", path.display());
                continue;
            }
        };

        let si = ShaderInfo { path, ty };
        let file_name = si.path.to_str().unwrap();
        if file_name.contains("frag") && frag_si.is_none() {
            frag_si = Some(si);
        } else if file_name.contains("comp") && comp_si.is_none() {
            comp_si = Some(si);
        } else if file_name.contains("vert") && vert_si.is_none() {
            vert_si = Some(si);
        } else if file_name.contains("prelude") {
            prelude = Some(si.path);
        } else {
            println!("This file have been rejected: {}", si.path.display());
        }
    }

    Ok(PilkaSpec {
        frag: frag_si.expect("Fragment shader is not provided"),
        vert: vert_si.expect("Vertex shader is not provided"),
        comp: comp_si.expect("Compute shader is not provided"),
        glsl_prelude: prelude,
    })
}
