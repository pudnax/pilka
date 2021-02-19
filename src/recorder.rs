use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io::{self, Write},
    process::{Child, Command, Output, Stdio},
    sync::mpsc,
};

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

pub fn ffmpeg_version() -> Result<Output, ProcessError> {
    let mut command = Command::new("ffmpeg");
    command.arg("-version");

    command.output().map_err(ProcessError::Other)
}

pub fn new_ffmpeg_command(width: u32, height: u32, filename: &str) -> Result<Child, ProcessError> {
    #[rustfmt::skip]
    let args = [
        "-framerate", "60",
        "-f", "rawvideo",
        "-pix_fmt", "rgba",
        "-i", "pipe:",
        "-c:v", "libx264",
        "-crf", "15",
        "-preset", "ultrafast",
        "-color_primaries", "bt709",
        "-color_trc", "bt709",
        "-colorspace", "bt709",
        "-color_range", "tv",
        "-chroma_sample_location", "center",
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

    dbg!(&command);

    // Not create terminal window
    #[cfg(windows)]
    command.creation_flags(0x08000000);

    let child = command.spawn().map_err(ProcessError::SpawnError)?;

    Ok(child)
}

pub fn record_thread(rx: mpsc::Receiver<RecordEvent>) {
    let mut process = None;

    while let Ok(event) = rx.recv() {
        match event {
            RecordEvent::Start(width, height) => {
                let filename = format!(
                    "record-{}.mp4",
                    chrono::Local::now().format("%d-%m-%Y-%H-%M-%S").to_string()
                );
                process = Some(new_ffmpeg_command(width, height, &filename).unwrap());
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
