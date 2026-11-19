//! Microphone capture. cpal streams are not `Send`, so the stream lives on its own thread
//! and is controlled through a channel.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

pub const WHISPER_RATE: u32 = 16_000;

enum Cmd {
    Start(String, Sender<Result<()>>),
    Stop(Sender<Recording>),
}

pub struct Recording {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

#[derive(Clone)]
pub struct Recorder {
    tx: Sender<Cmd>,
}

impl Recorder {
    pub fn spawn() -> Self {
        let (tx, rx) = channel::<Cmd>();
        std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
                let mut stream: Option<(Stream, u32)> = None;
                for cmd in rx {
                    match cmd {
                        Cmd::Start(microphone, reply) => {
                            buffer.lock().unwrap().clear();
                            stream = None;
                            match open_stream(buffer.clone(), &microphone) {
                                Ok(s) => {
                                    stream = Some(s);
                                    let _ = reply.send(Ok(()));
                                }
                                Err(e) => {
                                    let _ = reply.send(Err(e));
                                }
                            }
                        }
                        Cmd::Stop(reply) => {
                            let rate = stream.as_ref().map(|s| s.1).unwrap_or(WHISPER_RATE);
                            stream = None; // dropping the stream stops capture
                            let samples = std::mem::take(&mut *buffer.lock().unwrap());
                            let _ = reply.send(Recording {
                                samples,
                                sample_rate: rate,
                            });
                        }
                    }
                }
            })
            .expect("spawn audio thread");
        Self { tx }
    }

    /// `microphone` is a name from `microphones`, or empty for the system default.
    pub fn start(&self, microphone: &str) -> Result<()> {
        let (tx, rx) = channel();
        self.tx
            .send(Cmd::Start(microphone.to_string(), tx))
            .map_err(|_| anyhow!("audio thread stopped"))?;
        rx.recv().map_err(|_| anyhow!("audio thread stopped"))?
    }

    pub fn stop(&self) -> Result<Recording> {
        let (tx, rx) = channel();
        self.tx
            .send(Cmd::Stop(tx))
            .map_err(|_| anyhow!("audio thread stopped"))?;
        rx.recv().map_err(|_| anyhow!("audio thread stopped"))
    }
}

/// The names of the microphones plugged in.
pub fn microphones() -> Vec<String> {
    let host = cpal::default_host();
    let devices = host.input_devices().into_iter().flatten();
    devices.filter_map(|device| device.name().ok()).collect()
}

/// A microphone that's been unplugged falls back to the system default, rather than failing.
fn find_microphone(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    let chosen = host
        .input_devices()
        .ok()?
        .find(|device| device.name().is_ok_and(|found| found == name));
    chosen.or_else(|| host.default_input_device())
}

fn open_stream(buffer: Arc<Mutex<Vec<f32>>>, microphone: &str) -> Result<(Stream, u32)> {
    let host = cpal::default_host();
    let device =
        find_microphone(&host, microphone).ok_or_else(|| anyhow!("No microphone found"))?;
    let supported = device.default_input_config()?;
    let channels = supported.channels() as usize;
    let rate = supported.sample_rate().0;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let err = |e| eprintln!("audio stream error: {e}");

    macro_rules! build {
        ($t:ty, $conv:expr) => {{
            let buf = buffer.clone();
            device.build_input_stream(
                &config,
                move |data: &[$t], _| {
                    let mut b = buf.lock().unwrap();
                    for frame in data.chunks(channels) {
                        let sum: f32 = frame.iter().map(|s| $conv(*s)).sum();
                        b.push(sum / channels as f32);
                    }
                },
                err,
                None,
            )?
        }};
    }

    let stream = match format {
        SampleFormat::F32 => build!(f32, |s: f32| s),
        SampleFormat::I16 => build!(i16, |s: i16| s as f32 / i16::MAX as f32),
        SampleFormat::U16 => build!(u16, |s: u16| (s as f32 - 32768.0) / 32768.0),
        SampleFormat::I32 => build!(i32, |s: i32| s as f32 / i32::MAX as f32),
        other => return Err(anyhow!("Unsupported microphone sample format {other:?}")),
    };
    stream.play()?;
    Ok((stream, rate))
}

/// Linear resampling to 16 kHz mono, which is what Whisper expects.
pub fn resample_to_16k(samples: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == WHISPER_RATE || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / WHISPER_RATE as f64;
    let out_len = (samples.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = (pos - idx as f64) as f32;
            let a = samples[idx];
            let b = *samples.get(idx + 1).unwrap_or(&a);
            a + (b - a) * frac
        })
        .collect()
}

/// Root-mean-square loudness, used to skip silent recordings.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_length_and_identity() {
        let one_sec_48k = vec![0.5f32; 48_000];
        let out = resample_to_16k(&one_sec_48k, 48_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-6));
        let same = vec![0.1f32; 100];
        assert_eq!(resample_to_16k(&same, 16_000), same);
    }

    #[test]
    fn rms_of_silence_is_zero() {
        assert_eq!(rms(&[0.0; 10]), 0.0);
        assert!((rms(&[1.0, -1.0]) - 1.0).abs() < 1e-6);
    }
}
