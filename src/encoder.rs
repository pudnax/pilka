use ffmpeg::{
    codec::{self, packet::Packet},
    encoder, format, log, media,
    util::frame::Frame,
    Rational,
};
use ffmpeg_next as ffmpeg;

use std::io::Write;

pub fn encode_avframe<W: Write>(
    context: &mut codec::context::Context,
    encoder: &mut encoder::Encoder,
    frame: &mut Frame,
    packet: &mut Packet,
    stream: &mut W,
) -> Result<(), Box<dyn std::error::Error>> {
    encoder.send_frame(frame)?;

    while encoder.receive_packet(packet).is_ok() {}
    stream.write_all(packet.data().unwrap())?;
    Ok(())
}
