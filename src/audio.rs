//! Playback. Uses rodio when built with the `audio` feature and a sound device
//! is available; otherwise a silent clock keeps the UI behaving the same way.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

pub struct Audio {
    backend: Backend,
    volume: f32,
    /// RMS of the samples being played, written by the audio thread (f32 bits).
    level: Arc<AtomicU32>,
}

enum Backend {
    #[cfg(feature = "audio")]
    Rodio { _device: rodio::MixerDeviceSink, player: rodio::Player },
    Clock(Clock),
}

#[derive(Default)]
struct Clock {
    loaded: Option<Duration>,
    offset: Duration,
    started: Option<Instant>,
}

impl Clock {
    fn position(&self) -> Duration {
        self.offset + self.started.map(|s| s.elapsed()).unwrap_or_default()
    }
}

impl Audio {
    pub fn new() -> Self {
        #[cfg(feature = "audio")]
        if let Ok(mut device) = rodio::DeviceSinkBuilder::open_default_sink() {
            device.log_on_drop(false);
            let player = rodio::Player::connect_new(device.mixer());
            player.pause();
            return Self { backend: Backend::Rodio { _device: device, player }, volume: 0.8, level: Arc::default() };
        }
        Self { backend: Backend::Clock(Clock::default()), volume: 0.8, level: Arc::default() }
    }

    /// True when sound actually reaches a device.
    pub fn is_silent(&self) -> bool {
        matches!(self.backend, Backend::Clock(_))
    }

    /// Starts the file; returns its length when the decoder knows it.
    pub fn load(&mut self, path: &Path, duration: Duration) -> Result<Option<Duration>, String> {
        match &mut self.backend {
            #[cfg(feature = "audio")]
            Backend::Rodio { player, .. } => {
                let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
                let source = rodio::Decoder::try_from(file).map_err(|e| e.to_string())?;
                let length = rodio::Source::total_duration(&source);
                player.clear();
                player.append(Meter { inner: source, level: self.level.clone(), sum: 0.0, count: 0 });
                player.set_volume(self.volume * self.volume);
                player.play();
                Ok(length)
            }
            Backend::Clock(clock) => {
                let _ = path;
                *clock = Clock { loaded: Some(duration), offset: Duration::ZERO, started: Some(Instant::now()) };
                Ok(None)
            }
        }
    }

    pub fn play(&mut self) {
        match &mut self.backend {
            #[cfg(feature = "audio")]
            Backend::Rodio { player, .. } => player.play(),
            Backend::Clock(c) => {
                if c.started.is_none() {
                    c.started = Some(Instant::now());
                }
            }
        }
    }

    pub fn pause(&mut self) {
        match &mut self.backend {
            #[cfg(feature = "audio")]
            Backend::Rodio { player, .. } => player.pause(),
            Backend::Clock(c) => {
                c.offset = c.position();
                c.started = None;
            }
        }
    }

    pub fn stop(&mut self) {
        match &mut self.backend {
            #[cfg(feature = "audio")]
            Backend::Rodio { player, .. } => {
                player.clear();
                player.pause();
            }
            Backend::Clock(c) => *c = Clock::default(),
        }
    }

    pub fn seek(&mut self, to: Duration) {
        match &mut self.backend {
            #[cfg(feature = "audio")]
            Backend::Rodio { player, .. } => {
                let _ = player.try_seek(to);
            }
            Backend::Clock(c) => {
                c.offset = to;
                if c.started.is_some() {
                    c.started = Some(Instant::now());
                }
            }
        }
    }

    pub fn position(&self) -> Duration {
        match &self.backend {
            #[cfg(feature = "audio")]
            Backend::Rodio { player, .. } => player.get_pos(),
            Backend::Clock(c) => c.position(),
        }
    }

    /// The loaded song has played to its end.
    pub fn finished(&self) -> bool {
        match &self.backend {
            #[cfg(feature = "audio")]
            Backend::Rodio { player, .. } => player.empty(),
            Backend::Clock(c) => c.loaded.is_some_and(|d| c.position() >= d),
        }
    }

    /// Loudness right now (RMS, before volume), 0 when nothing is sounding.
    pub fn level(&self) -> f32 {
        match &self.backend {
            #[cfg(feature = "audio")]
            Backend::Rodio { player, .. } if !player.is_paused() && !player.empty() => f32::from_bits(self.level.load(Ordering::Relaxed)),
            _ => 0.0,
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        #[cfg(feature = "audio")]
        if let Backend::Rodio { player, .. } = &self.backend {
            player.set_volume(volume * volume); // perceptual curve
        }
    }
}

/// Passes samples through untouched while measuring their loudness for the visuals.
#[cfg(feature = "audio")]
struct Meter<S> {
    inner: S,
    level: Arc<AtomicU32>,
    sum: f32,
    count: u32,
}

#[cfg(feature = "audio")]
impl<S: rodio::Source> Iterator for Meter<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<rodio::Sample> {
        let sample = self.inner.next()?;
        let v = sample as f32;
        self.sum += v * v;
        self.count += 1;
        if self.count >= 1024 {
            let rms = (self.sum / self.count as f32).sqrt();
            self.level.store(rms.to_bits(), Ordering::Relaxed);
            self.sum = 0.0;
            self.count = 0;
        }
        Some(sample)
    }
}

#[cfg(feature = "audio")]
impl<S: rodio::Source> rodio::Source for Meter<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
    }
}

#[cfg(all(test, feature = "audio"))]
mod tests {
    /// YouTube songs arrive as m4a (AAC); make sure rodio's decoder handles that container.
    #[test]
    fn decodes_m4a_aac() {
        let dir = std::env::temp_dir().join(format!("musify-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("tone.m4a");
        let status = std::process::Command::new("ffmpeg")
            .args(["-loglevel", "error", "-y", "-f", "lavfi", "-i", "sine=f=440:d=2", "-c:a", "aac", "-b:a", "128k"])
            .arg(&file)
            .status();
        if !status.is_ok_and(|s| s.success()) {
            eprintln!("ffmpeg not available; skipping");
            return;
        }
        let decoder = rodio::Decoder::try_from(std::fs::File::open(&file).unwrap()).expect("rodio decodes m4a/AAC");
        let samples = decoder.count();
        std::fs::remove_dir_all(&dir).ok();
        assert!(samples > 44_100, "decoded {samples} samples");
    }
}

#[cfg(all(test, feature = "audio"))]
mod youtube_file {
    /// `MUSIFY_TEST_AUDIO=path cargo test -- --ignored` decodes a real downloaded file.
    #[test]
    #[ignore]
    fn decodes_downloaded_file() {
        let path = std::env::var("MUSIFY_TEST_AUDIO").expect("set MUSIFY_TEST_AUDIO");
        let decoder = rodio::Decoder::try_from(std::fs::File::open(path).unwrap()).expect("decodes");
        let samples = decoder.count();
        assert!(samples > 44_100 * 60, "decoded {samples} samples");
    }
}

#[cfg(all(test, feature = "audio"))]
mod meter_levels {
    use super::*;
    /// `MUSIFY_TEST_AUDIO=path cargo test meter -- --ignored --nocapture` prints the meter's readings.
    #[test]
    #[ignore]
    fn meter_reads_file() {
        let path = std::env::var("MUSIFY_TEST_AUDIO").expect("set MUSIFY_TEST_AUDIO");
        let level = Arc::new(AtomicU32::new(0));
        let decoder = rodio::Decoder::try_from(std::fs::File::open(path).unwrap()).unwrap();
        let mut meter = Meter { inner: decoder, level: level.clone(), sum: 0.0, count: 0 };
        let mut readings = Vec::new();
        for (i, _) in (&mut meter).enumerate() {
            if i % 44_100 == 0 {
                readings.push(f32::from_bits(level.load(Ordering::Relaxed)));
            }
            if i > 44_100 * 20 {
                break;
            }
        }
        println!("rms per second: {readings:.3?}");
    }
}
