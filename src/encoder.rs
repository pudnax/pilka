use ffmpeg::{
    codec::{self, packet::Packet},
    encoder, format, log, media, packet, sys,
    util::{self, frame},
    Rational,
};
use ffmpeg_next as ffmpeg;

use std::io::Write;

pub fn encode_avframe<W: Write>(
    encoder: &mut encoder::Encoder,
    frame: &frame::Video,
    packet: &mut Packet,
    stream: &mut W,
) -> Result<(), Box<dyn std::error::Error>> {
    encoder.send_frame(frame)?;

    while encoder.receive_packet(packet).is_ok() {}
    stream.write_all(packet.data().unwrap())?;
    Ok(())
}

#[warn(clippy::many_single_char_names)]
fn copy_image_as_yuv(image: &[u8], frame: &mut frame::Video, (width, _h): (usize, usize)) {
    let linesize = unsafe { (*frame.as_ptr()).linesize };
    for (index, chunk) in image.chunks_exact(4).enumerate() {
        let row = index % width;
        let col = index / width;
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

fn add_stream(
    params: &VideoParams,
    format_context: &format::context::Output,
) -> Result<(), util::error::Error> {
    // *codec = avocdec_find_encoder(codec_id);
    let context = encoder::find(codec::Id::MPEG2VIDEO).unwrap();
    let mut codec = encoder::new().video()?;

    // ost->st = avformat_new_stream(oc, NULL);
    let mut stream = format_context.add_stream(context)?;

    // Not needed because ffmpeg-rs does it by default, derp
    // ost->st->id = oc->nb_streams - 1;
    // let num_streams = format_context.nb_streams() as i32 - 1; // NOTE: Weak spot
    // unsafe { (*stream.as_mut_ptr()).id = num_streams };

    stream.set_time_base(Rational::new(1, params.fps));

    codec.set_bit_rate(params.bitrate);
    codec.set_width(params.width as u32);
    codec.set_height(params.height as u32);
    codec.set_time_base((1, params.fps));
    codec.set_gop(10); // 12
    codec.set_frame_rate(Some((params.fps, 1)));
    codec.set_max_b_frames(2);
    // codec.set_mb_decision(encoder::Decision::RateDistortion); //  MPEG1Video
    codec.set_format(format::Pixel::YUV420P);

    if format_context
        .format()
        .flags()
        .contains(format::Flags::GLOBAL_HEADER)
    {
        codec.set_flags(codec::Flags::GLOBAL_HEADER);
    }

    Ok(())
}

// TODO: Where is error handling?!!?
fn alloc_pcture(format: format::Pixel, width: u32, height: u32) -> frame::video::Video {
    let mut frame = frame::video::Video::empty();
    unsafe {
        frame.alloc(format, width, height);
    }

    frame
}

fn open_video(
    video: encoder::Video,
    output_context: format::context::Output,
    codec: Codec,
    stream: ffmpeg::StreamMut,
    options: &Dictionary,
) -> Result<(), util::error::Error> {
    let res = video.open_as_with(codec, *options)?;

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct VideoParams {
    fps: i32,
    width: usize,
    height: usize,
    bitrate: usize,
}

struct EncoderContext {
    frame: frame::Video,
    context: ffmpeg::Codec,
    packet: frame::Packet,
    output_stream: format::context::Output,
    codec: encoder::video::Video,
}

impl EncoderContext {
    pub fn new(params: VideoParams, path: &std::path::Path) -> Result<Self, util::error::Error> {
        let context = encoder::find(codec::Id::MPEG2VIDEO).unwrap();
        let mut codec = encoder::new().video()?;

        codec.set_bit_rate(params.bitrate);
        codec.set_width(params.width as u32);
        codec.set_height(params.height as u32);
        codec.set_time_base((1, params.fps));
        codec.set_gop(10); // 12
        codec.set_frame_rate(Some((params.fps, 1)));
        codec.set_max_b_frames(2);
        // codec.set_mb_decision(encoder::Decision::RateDistortion); //  MPEG1Video
        codec.set_format(format::Pixel::YUV420P);

        let octx = format::output(&path)?;
        let mut frame = frame::Video::empty();

        frame.set_height(params.height as u32);
        frame.set_width(params.width as u32);
        frame.set_format(format::Pixel::YUV420P);

        let ret = unsafe { sys::av_frame_get_buffer(frame.as_mut_ptr(), 0) };
        if ret < 0 {
            panic!("Error on allocating frame with: {}", ret);
        }

        let packet = frame.packet();

        Ok(Self {
            frame,
            context,
            packet,
            output_stream: octx,
            codec,
        })
    }
}
