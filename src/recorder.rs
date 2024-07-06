use anyhow::{Context, Result};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    process::{Child, Command, Stdio},
    thread::JoinHandle,
    time::Instant,
};

use crate::{create_folder, ImageDimensions, ManagedImage, SCREENSHOT_FOLDER, VIDEO_FOLDER};
use crossbeam_channel::{Receiver, Sender};

pub enum RecordEvent {
    Start(ImageDimensions),
    Record(ManagedImage),
    Finish,
    Screenshot(ManagedImage),
    CloseThread,
}

pub struct Recorder {
    pub sender: Sender<RecordEvent>,
    ffmpeg_installed: bool,
    pub ffmpeg_version: String,
    pub thread_handle: Option<JoinHandle<()>>,
    is_active: bool,
}

impl Recorder {
    pub fn new() -> Self {
        let mut command = Command::new("ffmpeg");
        command.arg("-version");
        let (version, installed) = match command.output() {
            Ok(output) => (
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap()
                    .to_string(),
                true,
            ),
            Err(e) => (e.to_string(), false),
        };

        let (tx, rx) = crossbeam_channel::unbounded();
        let thread_handle = std::thread::spawn(move || record_thread(rx));

        Self {
            sender: tx,
            ffmpeg_installed: installed,
            ffmpeg_version: version,
            thread_handle: Some(thread_handle),
            is_active: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn ffmpeg_installed(&self) -> bool {
        self.ffmpeg_installed
    }

    pub fn screenshot(&self, image: ManagedImage) {
        let _ = self
            .sender
            .send(RecordEvent::Screenshot(image))
            .context("Failed to send screenshot");
    }

    pub fn start(&mut self, dims: ImageDimensions) {
        self.is_active = true;
        self.send(RecordEvent::Start(dims));
    }

    pub fn record(&self, image: ManagedImage) {
        self.send(RecordEvent::Record(image));
    }

    pub fn finish(&mut self) {
        self.is_active = false;
        self.send(RecordEvent::Finish);
    }

    pub fn close_thread(&self) {
        self.sender.send(RecordEvent::CloseThread).unwrap();
    }

    pub fn send(&self, event: RecordEvent) {
        if !(self.ffmpeg_installed || matches!(event, RecordEvent::Screenshot(_))) {
            return;
        }
        self.sender.send(event).unwrap()
    }
}

struct RecorderThread {
    process: Child,
}

fn new_ffmpeg_command(image_dimensions: ImageDimensions, filename: &str) -> Result<RecorderThread> {
    #[rustfmt::skip]
    let args = [
        "-framerate", "60",
        "-pix_fmt", "rgba",
        "-f", "rawvideo",
        "-i", "pipe:",
        "-c:v", "libx264",
        "-crf", "23",
        // "-preset", "ultrafast",
        "-tune", "animation",
        "-color_primaries", "bt709",
        "-color_trc", "bt709",
        "-colorspace", "bt709",
        "-color_range", "tv",
        "-chroma_sample_location", "center",
        "-pix_fmt", "yuv420p",
        // "-movflags", "+faststart",
        "-y",
    ];

    let mut command = Command::new("ffmpeg");
    command
        .arg("-video_size")
        .arg(format!(
            "{}x{}",
            image_dimensions.width, image_dimensions.height
        ))
        .args(args)
        .arg(filename)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    #[cfg(windows)]
    {
        const WINAPI_UM_WINBASE_CREATE_NO_WINDOW: u32 = 0x08000000;
        // Not create terminal window
        command.creation_flags(WINAPI_UM_WINBASE_CREATE_NO_WINDOW);
    }

    let child = command.spawn()?;

    Ok(RecorderThread { process: child })
}

fn record_thread(rx: Receiver<RecordEvent>) {
    let mut recorder = None;

    while let Ok(event) = rx.recv() {
        match event {
            RecordEvent::Start(image_dimensions) => {
                create_folder(VIDEO_FOLDER).unwrap();
                let dir_path = Path::new(VIDEO_FOLDER);
                let filename = dir_path.join(format!(
                    "record-{}.mp4",
                    chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")
                ));
                recorder =
                    Some(new_ffmpeg_command(image_dimensions, filename.to_str().unwrap()).unwrap());
            }
            RecordEvent::Record(mut frame) => {
                if let Some(ref mut recorder) = recorder {
                    let writer = recorder.process.stdin.as_mut().unwrap();
                    let mut writer = BufWriter::new(writer);

                    let padded_bytes = frame.image_dimensions.padded_bytes_per_row as _;
                    let unpadded_bytes = frame.image_dimensions.unpadded_bytes_per_row as _;
                    let data = match frame.map_memory() {
                        Ok(data) => data,
                        Err(err) => {
                            eprintln!("Failed to map memory: {err}");
                            continue;
                        }
                    };

                    for chunk in data
                        .chunks(padded_bytes)
                        .map(|chunk| &chunk[..unpadded_bytes])
                    {
                        let _ = writer.write_all(chunk);
                    }
                    let _ = writer.flush();
                }
            }
            RecordEvent::Finish => {
                if let Some(ref mut p) = recorder {
                    p.process.wait().unwrap();
                }
                recorder = None;
                eprintln!("Recording finished");
            }
            RecordEvent::Screenshot(mut frame) => {
                let image_dimensions = frame.image_dimensions;
                let data = match frame.map_memory() {
                    Ok(data) => data,
                    Err(err) => {
                        eprintln!("Failed to map memory: {err}");
                        continue;
                    }
                };

                let _ = save_screenshot(data, image_dimensions).map_err(|err| eprintln!("{err}"));
            }
            RecordEvent::CloseThread => {
                return;
            }
        }
    }
}

pub fn save_screenshot(frame: &[u8], image_dimensions: ImageDimensions) -> Result<()> {
    let now = Instant::now();
    let screenshots_folder = Path::new(SCREENSHOT_FOLDER);
    create_folder(screenshots_folder)?;
    let path = screenshots_folder.join(format!(
        "screenshot-{}.png",
        chrono::Local::now().format("%Y-%m-%d_%H-%M-%S%.9f")
    ));
    let file = File::create(path)?;
    let w = BufWriter::new(file);
    let mut encoder =
        png::Encoder::new(w, image_dimensions.width as _, image_dimensions.height as _);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let padded_bytes = image_dimensions.padded_bytes_per_row;
    let unpadded_bytes = image_dimensions.unpadded_bytes_per_row;
    let mut writer = encoder
        .write_header()?
        .into_stream_writer_with_size(unpadded_bytes)?;
    writer.set_filter(png::FilterType::Paeth);
    writer.set_adaptive_filter(png::AdaptiveFilterType::Adaptive);
    for chunk in frame
        .chunks(padded_bytes)
        .map(|chunk| &chunk[..unpadded_bytes])
    {
        writer.write_all(chunk)?;
    }
    writer.finish()?;
    eprintln!("Encode image: {:#.2?}", now.elapsed());
    Ok(())
}
