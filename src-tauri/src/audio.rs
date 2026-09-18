use std::{
    mem,
    sync::{Arc, Mutex},
    time::Instant,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Error, SampleFormat, Stream, StreamConfig, SupportedStreamConfig,
};
use serde::Serialize;

const TARGET_SAMPLE_RATE: u32 = 16_000;
const MIN_SPEECH_MS: u64 = 200;
const RMS_THRESHOLD: f32 = 0.004;
const PEAK_THRESHOLD: f32 = 0.02;

#[derive(Default)]
pub struct AudioRecorder {
    active: Option<ActiveRecording>,
}

struct ActiveRecording {
    stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    input_sample_rate: u32,
    started_at: Instant,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordingStats {
    pub duration_ms: u64,
    pub speech_duration_ms: u64,
    pub sample_count: usize,
    pub rms: f32,
    pub peak: f32,
    pub has_speech: bool,
}

pub struct RecordingCapture {
    pub samples: Vec<f32>,
    pub stats: RecordingStats,
}

impl AudioRecorder {
    pub fn start_recording(&mut self) -> anyhow::Result<()> {
        if self.active.is_some() {
            return Ok(());
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No microphone input device is available"))?;
        let config = preferred_input_config(&device)?;
        let input_sample_rate = config.sample_rate();
        let stream_config: StreamConfig = config.into();
        let channels = usize::from(stream_config.channels).max(1);
        let samples = Arc::new(Mutex::new(Vec::with_capacity(
            input_sample_rate as usize * 20,
        )));
        let err_fn = |err| eprintln!("audio input stream error: {err}");

        let stream = match config.sample_format() {
            SampleFormat::F32 => build_stream::<f32, _>(
                &device,
                &stream_config,
                channels,
                samples.clone(),
                |v| v,
                err_fn,
            )?,
            SampleFormat::F64 => build_stream::<f64, _>(
                &device,
                &stream_config,
                channels,
                samples.clone(),
                |v| v as f32,
                err_fn,
            )?,
            SampleFormat::I8 => build_stream::<i8, _>(
                &device,
                &stream_config,
                channels,
                samples.clone(),
                |v| v as f32 / i8::MAX as f32,
                err_fn,
            )?,
            SampleFormat::I16 => build_stream::<i16, _>(
                &device,
                &stream_config,
                channels,
                samples.clone(),
                |v| v as f32 / i16::MAX as f32,
                err_fn,
            )?,
            SampleFormat::I32 => build_stream::<i32, _>(
                &device,
                &stream_config,
                channels,
                samples.clone(),
                |v| v as f32 / i32::MAX as f32,
                err_fn,
            )?,
            SampleFormat::U8 => build_stream::<u8, _>(
                &device,
                &stream_config,
                channels,
                samples.clone(),
                |v| (v as f32 / u8::MAX as f32) * 2.0 - 1.0,
                err_fn,
            )?,
            SampleFormat::U16 => build_stream::<u16, _>(
                &device,
                &stream_config,
                channels,
                samples.clone(),
                |v| (v as f32 / u16::MAX as f32) * 2.0 - 1.0,
                err_fn,
            )?,
            SampleFormat::U32 => build_stream::<u32, _>(
                &device,
                &stream_config,
                channels,
                samples.clone(),
                |v| (v as f32 / u32::MAX as f32) * 2.0 - 1.0,
                err_fn,
            )?,
            format => {
                return Err(anyhow::anyhow!(
                    "Unsupported microphone sample format: {format:?}"
                ));
            }
        };

        stream.play()?;
        self.active = Some(ActiveRecording {
            stream,
            samples,
            input_sample_rate,
            started_at: Instant::now(),
        });
        Ok(())
    }

    pub fn stop_recording(&mut self) -> anyhow::Result<RecordingCapture> {
        let active = self
            .active
            .take()
            .ok_or_else(|| anyhow::anyhow!("Recording is not active"))?;
        let elapsed = active.started_at.elapsed();
        drop(active.stream);

        let mut raw = active
            .samples
            .lock()
            .map_err(|_| anyhow::anyhow!("audio buffer lock poisoned"))
            .map(|mut guard| mem::take(&mut *guard))?;

        if active.input_sample_rate != TARGET_SAMPLE_RATE {
            raw = resample_linear(&raw, active.input_sample_rate, TARGET_SAMPLE_RATE);
        }

        let stats = analyze(&raw, elapsed.as_millis() as u64);
        Ok(RecordingCapture {
            samples: raw,
            stats,
        })
    }

    pub fn is_recording(&self) -> bool {
        self.active.is_some()
    }
}

fn preferred_input_config(device: &Device) -> anyhow::Result<SupportedStreamConfig> {
    if let Ok(configs) = device.supported_input_configs() {
        for range in configs {
            if range.channels() == 0 {
                continue;
            }
            if range.min_sample_rate() <= TARGET_SAMPLE_RATE
                && range.max_sample_rate() >= TARGET_SAMPLE_RATE
                && matches!(
                    range.sample_format(),
                    SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
                )
            {
                return Ok(range.with_sample_rate(TARGET_SAMPLE_RATE));
            }
        }
    }

    Ok(device.default_input_config()?)
}

fn build_stream<T, F>(
    device: &Device,
    config: &StreamConfig,
    channels: usize,
    samples: Arc<Mutex<Vec<f32>>>,
    convert: F,
    err_fn: impl FnMut(Error) + Send + 'static,
) -> anyhow::Result<Stream>
where
    T: cpal::SizedSample + Copy + Send + 'static,
    F: Fn(T) -> f32 + Send + Sync + Copy + 'static,
{
    let stream = device.build_input_stream(
        *config,
        move |input: &[T], _| {
            if let Ok(mut buffer) = samples.try_lock() {
                for frame in input.chunks(channels) {
                    let mono = frame.iter().copied().map(convert).sum::<f32>() / frame.len() as f32;
                    buffer.push(mono.clamp(-1.0, 1.0));
                }
            }
        },
        err_fn,
        None,
    )?;
    Ok(stream)
}

fn resample_linear(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if samples.is_empty() || source_rate == target_rate {
        return samples.to_vec();
    }

    let ratio = source_rate as f64 / target_rate as f64;
    let output_len = ((samples.len() as f64) / ratio).round().max(1.0) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let source_pos = i as f64 * ratio;
        let index = source_pos.floor() as usize;
        let frac = (source_pos - index as f64) as f32;
        let a = samples.get(index).copied().unwrap_or(0.0);
        let b = samples.get(index + 1).copied().unwrap_or(a);
        output.push(a + (b - a) * frac);
    }

    output
}

const WINDOW_SAMPLES: usize = TARGET_SAMPLE_RATE as usize / 10; // 100ms windows

fn analyze(samples: &[f32], elapsed_ms: u64) -> RecordingStats {
    if samples.is_empty() {
        return RecordingStats {
            duration_ms: elapsed_ms,
            speech_duration_ms: 0,
            sample_count: 0,
            rms: 0.0,
            peak: 0.0,
            has_speech: false,
        };
    }

    let mut peak = 0.0_f32;
    let mut energy = 0.0_f64;
    let mut speech_windows = 0_usize;
    let mut total_windows = 0_usize;

    for chunk in samples.chunks(WINDOW_SAMPLES) {
        total_windows += 1;
        let mut chunk_energy = 0.0_f64;
        let mut chunk_peak = 0.0_f32;
        for &s in chunk {
            let abs = s.abs();
            chunk_peak = chunk_peak.max(abs);
            chunk_energy += f64::from(s * s);
            peak = peak.max(abs);
            energy += f64::from(s * s);
        }
        let chunk_rms = (chunk_energy / chunk.len() as f64).sqrt() as f32;
        if chunk_rms > RMS_THRESHOLD || chunk_peak > PEAK_THRESHOLD {
            speech_windows += 1;
        }
    }

    let rms = (energy / samples.len() as f64).sqrt() as f32;
    let sample_duration_ms =
        ((samples.len() as f64 / TARGET_SAMPLE_RATE as f64) * 1000.0).round() as u64;
    let duration_ms = sample_duration_ms.max(elapsed_ms);
    let speech_duration_ms = if total_windows > 0 {
        (speech_windows as u64 * duration_ms) / total_windows as u64
    } else {
        0
    };
    let has_speech = duration_ms >= MIN_SPEECH_MS && (rms > RMS_THRESHOLD || peak > PEAK_THRESHOLD);

    RecordingStats {
        duration_ms,
        speech_duration_ms,
        sample_count: samples.len(),
        rms,
        peak,
        has_speech,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_linear_keeps_empty_input_empty() {
        assert!(resample_linear(&[], 48_000, TARGET_SAMPLE_RATE).is_empty());
    }

    #[test]
    fn resample_linear_downsamples_to_target_length() {
        let samples = vec![0.5; 48_000];
        let resampled = resample_linear(&samples, 48_000, TARGET_SAMPLE_RATE);
        assert_eq!(resampled.len(), TARGET_SAMPLE_RATE as usize);
    }

    #[test]
    fn analyze_rejects_silence() {
        let stats = analyze(&vec![0.0; TARGET_SAMPLE_RATE as usize], 1_000);
        assert!(!stats.has_speech);
        assert_eq!(stats.rms, 0.0);
    }

    #[test]
    fn analyze_accepts_sustained_signal() {
        let stats = analyze(&vec![0.05; TARGET_SAMPLE_RATE as usize / 2], 500);
        assert!(stats.has_speech);
        assert!(stats.rms > RMS_THRESHOLD);
        assert!(stats.peak > PEAK_THRESHOLD);
    }
}
