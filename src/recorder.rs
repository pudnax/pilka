use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io::{self, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
};

use crate::create_folder;
use crate::VIDEO_FOLDER;

pub enum RecordEvent {
    Start(u32, u32),
    Record(Vec<u8>),
    Finish,
}

#[derive(Debug)]
pub enum ProcessError {
    SpawnError(io::Error),
    Other(io::Error),
}

impl Display for ProcessError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ProcessError::SpawnError(_) => {
                write!(f, "Could not start ffmpeg. Make sure you have\nffmpeg installed and present in PATH")
            }
            ProcessError::Other(e) => {
                write!(f, "{}", e)
            }
        }
    }
}

impl std::error::Error for ProcessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ProcessError::Other(e) => Some(e),
            ProcessError::SpawnError(_) => None,
        }
    }
}

pub fn ffmpeg_version() -> Result<(String, bool), ProcessError> {
    let mut command = Command::new("ffmpeg");
    command.arg("-version");

    let res = match command.output().map_err(ProcessError::Other) {
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
    Ok(res)
}

pub fn new_ffmpeg_command(width: u32, height: u32, filename: &str) -> Result<Child, ProcessError> {
    #[rustfmt::skip]
    let args = [
        "-framerate", "60",
        "-pix_fmt", "rgba",
        "-f", "rawvideo",
        "-i", "pipe:",
        "-c:v", "libx264",
        "-crf", "15",
        "-preset", "ultrafast",
        "-color_primaries", "bt709",
        "-color_trc", "bt709",
        "-colorspace", "bt709",
        "-color_range", "tv",
        "-chroma_sample_location", "center",
        "-pix_fmt", "yuv420p",
        "-movflags", "+faststart",
        "-y",
    ];

    let mut command = Command::new("ffmpeg");
    command
        .arg("-video_size")
        .arg(&format!("{}x{}", width, height)[..])
        .args(&args)
        .arg(filename)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    // Not create terminal window
    #[cfg(windows)]
    command.creation_flags(0x08000000);

    let child = command.spawn().map_err(ProcessError::SpawnError)?;

    Ok(child)
}

pub fn record_thread(rx: mpsc::Receiver<RecordEvent>) {
    let mut process = None;
    create_folder(VIDEO_FOLDER).unwrap();

    while let Ok(event) = rx.recv() {
        match event {
            RecordEvent::Start(width, height) => {
                let dir_path = Path::new(VIDEO_FOLDER);
                let filename = dir_path.join(format!(
                    "record-{}.mp4",
                    chrono::Local::now().format("%d-%m-%Y-%H-%M-%S").to_string()
                ));
                process =
                    Some(new_ffmpeg_command(width, height, filename.to_str().unwrap()).unwrap());
            }
            RecordEvent::Record(frame) => {
                if let Some(ref mut process) = process {
                    let writer = process.stdin.as_mut().unwrap();
                    writer.write_all(&frame).unwrap();
                    writer.flush().unwrap();
                }
            }
            RecordEvent::Finish => {
                if let Some(ref mut process) = process {
                    process.wait().unwrap();
                }
                drop(process);
                process = None;
            }
        }
    }
}
