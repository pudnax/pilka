use color_eyre::*;
use crossbeam_channel::{Receiver, Sender};
use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io::{self, BufWriter, Write},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use pilka_types::ImageDimentions;

use super::utils::create_folder;
use crate::VIDEO_FOLDER;

pub enum RecordEvent {
    Start(ImageDimentions),
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

pub struct Recorder {
    process: Child,
    image_dimentions: ImageDimentions,
}

pub fn new_ffmpeg_command(
    image_dimentions: ImageDimentions,
    filename: &str,
) -> Result<Recorder, ProcessError> {
    #[rustfmt::skip]
    let args = [
        "-framerate", "60",
        "-pix_fmt", "rgba",
        "-f", "rawvideo",
        "-i", "pipe:",
        "-c:v", "libx264",
        "-crf", "15",
        "-preset", "ultrafast",
        "-tune", "animation",
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
        .arg(format!(
            "{}x{}",
            image_dimentions.unpadded_bytes_per_row / 4,
            image_dimentions.height
        ))
        .args(&args)
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

    let child = command.spawn().map_err(ProcessError::SpawnError)?;

    Ok(Recorder {
        process: child,
        image_dimentions,
    })
}

pub fn record_thread(rx: crossbeam_channel::Receiver<RecordEvent>) {
    puffin::profile_function!();

    let mut recorder = None;

    while let Ok(event) = rx.recv() {
        match event {
            RecordEvent::Start(image_dimentions) => {
                puffin::profile_scope!("Start Recording");

                create_folder(VIDEO_FOLDER).unwrap();
                let dir_path = Path::new(VIDEO_FOLDER);
                let filename = dir_path.join(format!(
                    "record-{}.mp4",
                    chrono::Local::now().format("%d-%m-%Y-%H-%M-%S").to_string()
                ));
                recorder =
                    Some(new_ffmpeg_command(image_dimentions, filename.to_str().unwrap()).unwrap());
            }
            RecordEvent::Record(frame) => {
                puffin::profile_scope!("Process Frame");

                if let Some(ref mut recorder) = recorder {
                    let writer = recorder.process.stdin.as_mut().unwrap();
                    let mut writer = BufWriter::new(writer);

                    let padded_bytes = recorder.image_dimentions.padded_bytes_per_row as _;
                    let unpadded_bytes = recorder.image_dimentions.unpadded_bytes_per_row as _;
                    for chunk in frame
                        .chunks(padded_bytes)
                        .map(|chunk| &chunk[..unpadded_bytes])
                    {
                        writer.write_all(chunk).unwrap();
                    }
                    // writer.write_all(&frame).unwrap();
                    writer.flush().unwrap();
                }
            }
            RecordEvent::Finish => {
                puffin::profile_scope!("Stop Recording");

                if let Some(ref mut process) = recorder {
                    process.process.wait().unwrap();
                }
                drop(recorder);
                recorder = None;
            }
        }
    }
}

/// ---------------RecordTimer------------------
/// |<-              until                   ->|
/// |<-  start_rx  ->|<-      counter        ->|
///                  |<-         tx          ->|
pub struct RecordTimer {
    until: Option<Duration>,
    pub counter: Option<Instant>,
    start_rx: Option<Receiver<()>>,
    tx: Sender<RecordEvent>,
}

impl RecordTimer {
    const NUM_SKIPPED_FRAMES: usize = 3;
    pub fn new(until: Option<Duration>, tx: Sender<RecordEvent>) -> (Self, Sender<()>) {
        let (start_tx, start_rx) = crossbeam_channel::bounded(Self::NUM_SKIPPED_FRAMES);
        let counter = None;
        (
            Self {
                until,
                counter,
                start_rx: Some(start_rx),
                tx,
            },
            start_tx,
        )
    }

    pub fn update(
        &mut self,
        video_recording: &mut bool,
        image_dimentions: ImageDimentions,
    ) -> Result<()> {
        if let Some(until) = self.until {
            if let Some(ref start_rx) = self.start_rx {
                if start_rx.is_full() {
                    self.counter = Some(Instant::now());
                    self.tx.send(RecordEvent::Start(image_dimentions))?;
                    *video_recording = true;

                    self.start_rx = None;
                }
            }

            if let Some(now) = self.counter {
                if until < now.elapsed() {
                    *video_recording = false;
                    self.tx.send(RecordEvent::Finish).unwrap();
                    std::thread::sleep(Duration::from_millis(100));
                    std::process::exit(0);
                }
            }
        }
        Ok(())
    }
}
