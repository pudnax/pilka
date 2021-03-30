use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{HostId, Stream};
use eyre::*;
use std::sync::mpsc::{Receiver, Sender};

pub struct AudioConfig {
    pub sample_rate: u32,
    pub num_channels: u16,
    pub host_id: HostId,
}

pub fn create_audio_stream() -> Result<(Stream, Receiver<[f32; 1024 * 2]>, AudioConfig)> {
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

    fn write_input_data<T>(input: &[T], tx: &Sender<[f32; 1024 * 2]>)
    where
        T: cpal::Sample,
    {
        let sample = input
            .iter()
            .map(|s| s.to_f32())
            .inspect(|x| print!("{:.03}, ", x))
            .sum::<f32>()
            / input.len() as f32;

        let mut s = [0f32; 1024 * 2];
        for (si, ii) in s.iter_mut().zip(input.iter()) {
            *si = ii.to_f32();
        }

        println!("{} : {}", input.len(), sample);
        // tx.send(sample.max(-1.0).min(1.0)).ok();

        tx.send(s);
    }
    let stream = device.build_input_stream(
        &config.into(),
        move |data, cx: &_| {
            write_input_data::<f32>(data, &audio_tx);
            dbg!(cx);
        },
        err_fn,
    )?;

    let config = AudioConfig {
        sample_rate,
        num_channels,
        host_id: host.id(),
    };
    Ok((stream, audio_rx, config))
}
