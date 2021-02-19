use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{HostId, Stream};
use eyre::*;
use std::sync::mpsc::{Receiver, Sender};

pub struct AudioConfig {
    pub sample_rate: u32,
    pub num_channels: u16,
    pub host_id: HostId,
}

pub fn create_audio_stream() -> Result<(Stream, Receiver<f32>, AudioConfig)> {
    let host = cpal::default_host();

    let device = host
        .default_input_device()
        .context("failed to find input device")?;

    let config = device.default_input_config()?;
    let sample_rate = config.sample_rate().0;
    let num_channels = config.channels();

    let err_fn = move |err| {
        eprintln!("an error occured on stream: {}", err);
    };

    let (audio_tx, audio_rx) = std::sync::mpsc::channel();

    fn write_input_data<T>(input: &[T], tx: &Sender<f32>)
    where
        T: cpal::Sample,
    {
        let sample = input
            .iter()
            .map(|s| cpal::Sample::from(s))
            .map(|s: T| s.to_f32())
            .sum::<f32>()
            / input.len() as f32;

        tx.send(sample.max(-1.0).min(1.0)).ok();
    }
    let stream = device.build_input_stream(
        &config.into(),
        move |data, _: &_| write_input_data::<f32>(data, &audio_tx),
        err_fn,
    )?;

    let config = AudioConfig {
        sample_rate,
        num_channels,
        host_id: host.id(),
    };
    Ok((stream, audio_rx, config))
}
