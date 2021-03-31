use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{HostId, PlayStreamError, Stream};
use eyre::*;
use std::sync::{Arc, Mutex};

use rustfft::{num_complex::Complex32, Fft, FftPlanner};

const FFT_SIZE: usize = 1024 * 2;
const AMPLIFICATION: f32 = 1.;

pub struct AudioContext {
    pub sample_rate: u32,
    pub num_channels: u16,
    pub host_id: HostId,
    stream: Stream,
    sample_buffer: Arc<Mutex<Vec<Complex32>>>,
    fft: Arc<dyn Fft<f32>>,
    scratch_area: Vec<Complex32>,
}

impl AudioContext {
    pub fn new() -> Result<Self> {
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

        let buff = Arc::new(Mutex::new(vec![Complex32::default(); FFT_SIZE]));
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let scratch_area = vec![Complex32::default(); fft.get_inplace_scratch_len()];

        let stream = device.build_input_stream(
            &config.into(),
            {
                let buff2 = Arc::clone(&buff);
                move |data, _: &_| {
                    let mut buff3 = buff2.lock().unwrap();
                    write_input_data::<f32>(data, &mut buff3);
                }
            },
            err_fn,
        )?;

        let config = AudioContext {
            sample_rate,
            num_channels,
            host_id: host.id(),
            stream,
            sample_buffer: buff,
            fft,
            scratch_area,
        };
        Ok(config)
    }
    pub fn play(&mut self) -> Result<(), PlayStreamError> {
        self.stream.play()
    }

    pub fn get_fft(&mut self, out: &mut [f32]) {
        let mut buf = self.sample_buffer.lock().unwrap();
        self.fft
            .process_with_scratch(&mut buf, &mut self.scratch_area);

        let scaling = 2. / (buf.len() as f32 * buf.len() as f32);
        out.iter_mut()
            .zip(buf.iter().map(|s| s.norm() * scaling))
            .for_each(|(l, r)| *l = r);
    }
}

fn write_input_data<T>(input: &[T], buff: &mut [Complex32])
where
    T: cpal::Sample,
{
    let size = input.len().min(buff.len());

    buff.rotate_right(size);
    buff.iter_mut()
        .skip(size)
        .zip(input.iter().map(|s| s.to_f32()))
        .for_each(|(l, r)| *l = (r * AMPLIFICATION).into());
}
