use ffmpeg::{
    codec, encoder, format, packet, sys,
    util::{self, error, frame},
    Dictionary,
};
use ffmpeg_next as ffmpeg;
use std::{io::Write, time::Instant};

// const DEFAULT_X264_OPTS: &str = "preset=medium";
pub const DEFAULT_X264_OPTS: &str = "preset=veryslow,crf=18";

#[allow(clippy::many_single_char_names)]
pub fn copy_as_yuv_image(image: &[u8], frame: &mut frame::Video, (width, _h): (u32, u32)) {
    let linesize = unsafe { (*frame.as_ptr()).linesize };
    for (index, chunk) in image.chunks_exact(4).enumerate() {
        let row = index % width as usize;
        let col = index / width as usize;
        let r = chunk[0] as f32;
        let g = chunk[1] as f32;
        let b = chunk[2] as f32;

        let y = (0.257 * r) + (0.504 * g) + (0.098 * b) + 16.;
        let u = -(0.148 * r) - (0.291 * g) + (0.439 * b) + 128.;
        let v = (0.439 * r) - (0.368 * g) - (0.071 * b) + 128.;

        frame.data_mut(0)[(row * linesize[0] as usize + col)] = y as u8;
        frame.data_mut(1)[(row >> 1) * linesize[1] as usize + (col >> 1)] = u as u8;
        frame.data_mut(2)[(row >> 1) * linesize[2] as usize + (col >> 1)] = v as u8;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VideoParams {
    pub fps: i32,
    pub width: u32,
    pub height: u32,
    pub bitrate: usize,
}

pub struct Recorder {
    pub encoder: encoder::Video,
    logging_enabled: bool,
    pub frame_count: usize,
    last_log_frame_count: usize,
    starting_time: Instant,
    last_log_time: Instant,
}

impl Recorder {
    pub fn new(
        params: &VideoParams,
        x264_opts: Dictionary,
        enable_logging: bool,
    ) -> Result<Self, error::Error> {
        let codec = encoder::find(codec::Id::MPEG2VIDEO).unwrap();

        let context = codec::Context::new();
        let mut encoder = context.encoder().video()?;

        encoder.set_bit_rate(params.bitrate);
        encoder.set_width(params.width);
        encoder.set_height(params.height);
        encoder.set_aspect_ratio((params.width as i32, params.height as i32));
        encoder.set_time_base((1, params.fps));
        encoder.set_gop(10); // 12
        encoder.set_frame_rate(Some((params.fps, 1)));
        encoder.set_max_b_frames(2);
        // encoder.set_mb_decision(encoder::Decision::RateDistortion); //  MPEG1Video
        encoder.set_format(format::Pixel::YUV420P);

        let encoder = encoder.open_as_with(codec, x264_opts)?;
        Ok(Self {
            encoder,
            logging_enabled: enable_logging,
            frame_count: 0,
            last_log_frame_count: 0,
            starting_time: Instant::now(),
            last_log_time: Instant::now(),
        })
    }

    pub fn encode<W: Write>(
        &mut self,
        frame: &util::frame::Frame,
        packet: &mut packet::Packet,
        f: &mut W,
    ) -> Result<(), util::error::Error> {
        self.encoder.send_frame(frame)?;

        loop {
            self.frame_count += 1;
            self.log_progress();

            match self.encoder.receive_packet(packet) {
                Ok(_) => {}
                Err(error::Error::Other {
                    errno: error::EAGAIN,
                })
                | Err(error::Error::Eof) => return Ok(()),
                Err(e) => panic!("Error on video recording with: {}", e),
            }
            f.write_all(packet.data().unwrap()).unwrap();
        }
    }

    fn log_progress(&mut self) {
        if !self.logging_enabled
            || (self.frame_count - self.last_log_frame_count < 10
                && self.last_log_time.elapsed().as_secs_f64() < 1.0)
        {
            return;
        }
        println!(
            "time elpased: \t{:8.2}\tframe count: {:8}",
            self.starting_time.elapsed().as_secs_f64(),
            self.frame_count,
        );
        self.last_log_frame_count = self.frame_count;
        self.last_log_time = Instant::now();
    }
}

pub fn parse_opts<'a>(s: String) -> Option<Dictionary<'a>> {
    let mut dict = Dictionary::new();
    for keyval in s.split_terminator(',') {
        let tokens: Vec<&str> = keyval.split('=').collect();
        match tokens[..] {
            [key, val] => dict.set(key, val),
            _ => return None,
        }
    }
    Some(dict)
}

pub struct Frame {
    pub data: Vec<u8>,
    pub wh: (u32, u32),
}

pub enum RecordEvent {
    Start((u32, u32)),
    Continue(Frame),
    End,
}

pub fn alloc_picture(format: format::Pixel, width: u32, height: u32) -> frame::video::Video {
    let mut frame = frame::video::Video::empty();
    unsafe {
        frame.set_format(format);
        frame.set_width(width);
        frame.set_height(height);

        sys::av_frame_get_buffer(frame.as_mut_ptr(), 0);
    }
    assert!(unsafe { sys::av_frame_make_writable(frame.as_mut_ptr()) } >= 0);

    frame
}
