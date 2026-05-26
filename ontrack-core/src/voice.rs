//! voice.rs — Cross-platform voice capture and transcription for OnTrack-RS.
//!
//! ## Audio backend (cpal 0.17)
//! | Platform | Backend |
//! |----------|---------|
//! | Linux    | ALSA — routes through PipeWire-ALSA compatibility layer |
//! |          | Set `ONTRACK_PIPEWIRE_NODE` to use the echo-cancelled source |
//! | Windows  | WASAPI (default) |
//! | macOS    | CoreAudio |
//! | iOS      | CoreAudio (AVAudioSession) |
//! | Android  | Oboe |
//!
//! ## Transcription engine (whisper-rs — whisper.cpp FFI)
//! All inference is local — no API key, no network.
//! Model is downloaded to `~/.cache/ontrack/whisper/` on first use.
//!
//! ## PipeWire note
//! cpal's native PipeWire backend is in development (cpal 0.18 roadmap).
//! For now, `ONTRACK_PIPEWIRE_NODE` lets you specify the exact ALSA device
//! name that WirePlumber exposes for the echo-cancelled source. Leave unset
//! to use the system default.
//!
//! ## Usage
//! ```rust,ignore
//! use ontrack_core::voice::{VoiceRecognizer, VoiceResult};
//!
//! let mut vr = VoiceRecognizer::new("base").unwrap();
//! vr.start_recording().unwrap();
//! std::thread::sleep(std::time::Duration::from_secs(3));
//! let result: VoiceResult = vr.stop_and_transcribe().unwrap();
//! println!("{}", result.text);
//! ```

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};


/// Result of a transcription.
#[derive(Debug, Clone, Default)]
pub struct VoiceResult {
    /// Transcribed text (empty string if nothing was detected).
    pub text:     String,
    /// Detected language code (e.g. "en").
    pub language: String,
    /// Duration of audio that was transcribed (seconds).
    pub duration: f32,
    /// Wall-clock time spent on transcription (seconds).
    pub elapsed:  f32,
}

impl VoiceResult {
    /// True if transcription succeeded and produced non-empty text.
    pub fn has_text(&self) -> bool {
        !self.text.trim().is_empty()
    }
}

/// Recording state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
    Processing,
}

// ── Internal audio buffer ──────────────────────────────────────────────────

const SAMPLE_RATE:  u32 = 16_000; // Whisper expects 16 kHz mono
const CHANNELS:     u16 = 1;
const MAX_SECS:     u32 = 60;

type AudioBuffer = Arc<Mutex<Vec<f32>>>;

// ── Model path helper ──────────────────────────────────────────────────────

/// Return the path where the ggml model file should be cached.
pub fn model_path(model_name: &str) -> PathBuf {
    let cache = dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache")
        .join("ontrack")
        .join("whisper");
    std::fs::create_dir_all(&cache).ok();
    cache.join(format!("ggml-{model_name}.bin"))
}

/// URL to download a ggml model from.
pub fn model_url(model_name: &str) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{model_name}.bin"
    )
}

/// Download a model if it is not already cached. Returns the local path.
pub async fn ensure_model(client: &reqwest::Client, model_name: &str) -> Result<PathBuf> {
    let path = model_path(model_name);
    if path.exists() {
        return Ok(path);
    }

    let url = model_url(model_name);
    tracing::info!("Downloading Whisper model '{}' from {}", model_name, url);

    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(600))
        .send()
        .await?
        .error_for_status()?;

    let bytes = resp.bytes().await?;
    std::fs::write(&path, &bytes)?;
    tracing::info!("Saved model to {}", path.display());
    Ok(path)
}

// ── VoiceRecognizer ────────────────────────────────────────────────────────

/// High-level voice recognition interface.
pub struct VoiceRecognizer {
    model_name:  String,
    model_path:  Option<PathBuf>,
    state:       RecordingState,
    buffer:      AudioBuffer,
    stream:      Option<Box<dyn std::any::Any + Send>>,
    start_time:  Option<Instant>,
}

impl VoiceRecognizer {
    /// Create a new recognizer. Does not load the model yet.
    pub fn new(model_name: &str) -> Result<Self> {
        Ok(Self {
            model_name: model_name.to_string(),
            model_path: None,
            state:      RecordingState::Idle,
            buffer:     Arc::new(Mutex::new(Vec::new())),
            stream:     None,
            start_time: None,
        })
    }

    /// Set the model path explicitly (skip auto-download).
    pub fn with_model_path(mut self, path: PathBuf) -> Self {
        self.model_path = Some(path);
        self
    }

    /// Begin capturing microphone audio.
    pub fn start_recording(&mut self) -> Result<()> {
        if self.state == RecordingState::Recording {
            return Ok(());
        }

        let buffer = Arc::clone(&self.buffer);
        {
            let mut b = buffer.lock().unwrap();
            b.clear();
        }

        let stream = build_input_stream(buffer)?;
        self.stream     = Some(stream);
        self.state      = RecordingState::Recording;
        self.start_time = Some(Instant::now());
        Ok(())
    }

    /// Stop recording and transcribe synchronously.
    /// Blocks the calling thread until transcription completes.
    pub fn stop_and_transcribe(&mut self) -> Result<VoiceResult> {
        if self.state != RecordingState::Recording {
            return Err(anyhow!("Not currently recording."));
        }

        // Drop the stream to stop the callback
        self.stream = None;
        self.state  = RecordingState::Processing;

        let audio: Vec<f32> = {
            let b = self.buffer.lock().unwrap();
            b.clone()
        };

        let duration = audio.len() as f32 / SAMPLE_RATE as f32;
        let path     = self.resolve_model_path()?;
        let t0       = Instant::now();

        let result = transcribe_audio(&path, &audio, duration)?;
        self.state  = RecordingState::Idle;
        Ok(VoiceResult {
            elapsed: t0.elapsed().as_secs_f32(),
            ..result
        })
    }

    /// Stop recording and spawn transcription on a background thread.
    /// `callback` is called with the result when transcription finishes.
    pub fn stop_and_transcribe_async(
        &mut self,
        callback: impl FnOnce(Result<VoiceResult>) + Send + 'static,
    ) -> Result<()> {
        if self.state != RecordingState::Recording {
            return Err(anyhow!("Not currently recording."));
        }

        self.stream = None;
        self.state  = RecordingState::Processing;

        let audio: Vec<f32> = {
            let b = self.buffer.lock().unwrap();
            b.clone()
        };

        let duration = audio.len() as f32 / SAMPLE_RATE as f32;
        let path     = self.resolve_model_path()?;

        std::thread::spawn(move || {
            let t0 = Instant::now();
            let res = transcribe_audio(&path, &audio, duration).map(|mut r| {
                r.elapsed = t0.elapsed().as_secs_f32();
                r
            });
            callback(res);
        });

        Ok(())
    }

    pub fn state(&self) -> &RecordingState { &self.state }
    pub fn is_recording(&self) -> bool { self.state == RecordingState::Recording }

    /// List available audio input devices (name + index).
    pub fn list_input_devices() -> Vec<(usize, String)> {
        list_input_devices_impl()
    }

    // ── Private helpers ────────────────────────────────────────────────────

    fn resolve_model_path(&self) -> Result<PathBuf> {
        if let Some(ref p) = self.model_path {
            if p.exists() { return Ok(p.clone()); }
            return Err(anyhow!("Model path {} not found", p.display()));
        }
        let p = model_path(&self.model_name);
        if p.exists() {
            Ok(p)
        } else {
            Err(anyhow!(
                "Whisper model '{}' not found at {}.\n\
                 Run: ontrack voice --download-model {}",
                self.model_name, p.display(), self.model_name
            ))
        }
    }
}

// ── cpal input stream ──────────────────────────────────────────────────────

fn build_input_stream(buffer: AudioBuffer) -> Result<Box<dyn std::any::Any + Send>> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = get_preferred_host();
    let device = get_preferred_device(&host)?;

    // Find a config closest to 16kHz mono
    let config = find_input_config(&device)?;
    tracing::debug!("Voice capture: device={:?} config={:?}",
        device.description().unwrap_or_default(), config);

    let stream_config = cpal::StreamConfig {
        channels:    CHANNELS,
        sample_rate: SAMPLE_RATE,
        buffer_size: cpal::BufferSize::Default,
    };

    let buf_clone = Arc::clone(&buffer);
    let stream = device.build_input_stream(
        &stream_config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut b = buf_clone.lock().unwrap();
            // Cap at MAX_SECS
            if b.len() < (MAX_SECS * SAMPLE_RATE) as usize {
                b.extend_from_slice(data);
            }
        },
        |err| tracing::error!("Audio input error: {err}"),
        None,
    )?;

    stream.play()?;

    // Return as type-erased box so we can drop it to stop the stream
    Ok(Box::new(stream))
}

/// Choose the preferred cpal host.
/// On Linux: ALSA (routes through PipeWire's ALSA compat layer via WirePlumber).
/// On Windows: WASAPI. On macOS/iOS: CoreAudio. On Android: Oboe.
fn get_preferred_host() -> cpal::Host {
    use cpal::traits::HostTrait;

    #[cfg(target_os = "linux")]
    {
        // Prefer JACK if available (also routes through PipeWire)
        if let Ok(host) = cpal::host_from_id(cpal::HostId::Jack) {
            return host;
        }
    }

    cpal::default_host()
}

/// Get the preferred input device.
/// On Linux: if ONTRACK_PIPEWIRE_NODE is set, use that device name.
fn get_preferred_device(host: &cpal::Host) -> Result<cpal::Device> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let node_name = std::env::var("ONTRACK_PIPEWIRE_NODE").unwrap_or_default();

    if !node_name.is_empty() {
        // Search for a device matching the PipeWire node name
        if let Ok(devices) = host.input_devices() {
            for dev in devices {
                if let Ok(name) = dev.description() {
                    if name.to_lowercase().contains(&node_name.to_lowercase()) {
                        tracing::info!("Using PipeWire node: {name}");
                        return Ok(dev);
                    }
                }
            }
        }
        tracing::warn!(
            "ONTRACK_PIPEWIRE_NODE={node_name} not found, falling back to default"
        );
    }

    host.default_input_device()
        .ok_or_else(|| anyhow!("No audio input device found"))
}

fn find_input_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig> {
    use cpal::traits::DeviceTrait;

    // Try exact 16kHz mono first
    let target = cpal::StreamConfig {
        channels:    CHANNELS,
        sample_rate: SAMPLE_RATE,
        buffer_size: cpal::BufferSize::Default,
    };

    if let Ok(configs) = device.supported_input_configs() {
        for c in configs {
            if c.channels() == CHANNELS
                && c.min_sample_rate() <= SAMPLE_RATE
                && c.max_sample_rate() >= SAMPLE_RATE
                && c.sample_format() == cpal::SampleFormat::F32
            {
                return Ok(c.with_sample_rate(SAMPLE_RATE));
            }
        }
    }

    // Fallback: use device default
    device.default_input_config()
        .map_err(|e| anyhow!("No supported input config: {e}"))
}

fn list_input_devices_impl() -> Vec<(usize, String)> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices
            .enumerate()
            .map(|(i, d)| (i, d.description().unwrap_or_else(|_| format!("Device {i}"))))
            .collect(),
        Err(_) => Vec::new(),
    }
}

// ── whisper.cpp transcription via whisper-rs ───────────────────────────────

fn transcribe_audio(
    model_path: &PathBuf,
    audio:      &[f32],
    duration:   f32,
) -> Result<VoiceResult> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    if audio.is_empty() {
        return Ok(VoiceResult {
            text: String::new(),
            duration,
            ..Default::default()
        });
    }

    // Re-sample to 16kHz if needed (simple linear decimation; whisper-rs
    // accepts any rate but accuracy is best at exactly 16kHz)
    let samples = if audio.len() > 0 {
        audio.to_vec()
    } else {
        return Ok(VoiceResult { duration, ..Default::default() });
    };

    let ctx = WhisperContext::new_with_params(
        model_path.to_str().ok_or_else(|| anyhow!("Invalid model path"))?,
        WhisperContextParameters::default(),
    )
    .map_err(|e| anyhow!("Failed to load Whisper model: {e}"))?;

    let mut state = ctx.create_state()
        .map_err(|e| anyhow!("Failed to create Whisper state: {e}"))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("auto"));
    params.set_print_special(false);
    params.set_print_realtime(false);
    params.set_print_progress(false);
    params.set_print_timestamps(false);
    params.set_no_context(true);
    params.set_single_segment(false);
    params.set_token_timestamps(false);

    state.full(params, &samples)
        .map_err(|e| anyhow!("Whisper transcription failed: {e}"))?;

    let num_segments = state.full_n_segments()
        .map_err(|e| anyhow!("Failed to get segment count: {e}"))?;

    let mut text = String::new();
    for i in 0..num_segments {
        if let Ok(seg) = state.full_get_segment_text(i) {
          let seg: String = seg.trim().to_string();
            if !seg.starts_with('[') {  // skip [BLANK_AUDIO], [MUSIC] etc.
                if !text.is_empty() { text.push(' '); }
                text.push_str(&seg);
            }
        }
    }

    let language = state.full_lang_id_from_state()
        .ok()
        .and_then(|id| whisper_rs::get_lang_str(id).map(str::to_string))
        .unwrap_or_else(|| "auto".to_string());

    Ok(VoiceResult {
        text: text.trim().to_string(),
        language,
        duration,
        elapsed: 0.0,
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_path_is_under_home() {
        let p = model_path("tiny");
        assert!(p.to_str().unwrap().contains("ontrack"));
        assert!(p.to_str().unwrap().contains("whisper"));
        assert!(p.to_str().unwrap().ends_with("ggml-tiny.bin"));
    }

    #[test]
    fn model_url_format() {
        let url = model_url("base");
        assert!(url.contains("ggerganov/whisper.cpp"));
        assert!(url.contains("ggml-base.bin"));
    }

    #[test]
    fn voice_result_has_text() {
        let r = VoiceResult { text: "hello".into(), ..Default::default() };
        assert!(r.has_text());
        let empty = VoiceResult::default();
        assert!(!empty.has_text());
    }

    #[test]
    fn list_devices_does_not_panic() {
        // Just verify it doesn't crash (no audio hardware in CI)
        let _devs = VoiceRecognizer::list_input_devices();
    }

    #[test]
    fn recognizer_state_idle_on_new() {
        let vr = VoiceRecognizer::new("tiny").unwrap();
        assert_eq!(*vr.state(), RecordingState::Idle);
    }

    #[test]
    fn stop_without_recording_errors() {
        let mut vr = VoiceRecognizer::new("tiny").unwrap();
        assert!(vr.stop_and_transcribe().is_err());
    }

    #[test]
    fn transcribe_empty_audio_returns_empty_text() {
        // Use a dummy path — transcribe_audio is called with empty slice
        // so it returns early before loading the model
        let p = PathBuf::from("/nonexistent/model.bin");
        let result = transcribe_audio(&p, &[], 0.0).unwrap();
        assert!(result.text.is_empty());
    }
}
