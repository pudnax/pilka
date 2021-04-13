use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::HostId;
use eyre::*;
use std::sync::{Arc, Mutex};
use std::thread;

use rustfft::{num_complex::Complex32, Fft, FftPlanner};

pub const FFT_SIZE: usize = 1024 * 2;
const AMPLIFICATION: f32 = 1.;

pub struct AudioContext {
    pub sample_rate: u32,
    pub num_channels: u16,
    pub host_id: HostId,
    sample_buffer: Arc<Mutex<Vec<Complex32>>>,
    fft: Arc<dyn Fft<f32>>,
    inner: Vec<Complex32>,
    scratch_area: Vec<Complex32>,
}

fn start_audio_thread() -> Result<AudioContext> {
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

    let inner = vec![Complex32::default(); FFT_SIZE];
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
    match stream.play() {
        Err(e) => Err(Report::from(e)),
        Ok(()) => Ok(AudioContext {
            sample_rate,
            num_channels,
            host_id: host.id(),
            sample_buffer: buff,
            fft,
            inner,
            scratch_area,
        }),
    }
}

impl AudioContext {
    pub fn new() -> Result<Self> {
        // Start the audio stream on another thread, to work around winit + cpal COM incompatibilities
        // on Windows.
        let context = thread::spawn(start_audio_thread)
            .join()
            .expect("Audio thread crashed")?;
        Ok(context)
    }

    pub fn get_fft(&mut self, out: &mut [f32]) {
        let mut buf = self.sample_buffer.lock().unwrap();
        self.fft
            .process_outofplace_with_scratch(&mut buf, &mut self.inner, &mut self.scratch_area);

        let scaling = 2. / (buf.len() as f32 * buf.len() as f32);
        out.iter_mut()
            .zip(self.inner.iter().map(|s| s.norm() * scaling))
            .for_each(|(l, r)| *l = r);
    }
}

fn write_input_data<T>(input: &[T], buff: &mut [Complex32])
where
    T: cpal::Sample,
{
    let buff_size = buff.len();
    let diff = buff_size - input.len().min(buff_size);

    buff.rotate_right(diff);
    buff.iter_mut()
        .skip(diff)
        .zip(input.iter().map(|s| s.to_f32()))
        .for_each(|(l, r)| *l = (r * AMPLIFICATION).into());
}
