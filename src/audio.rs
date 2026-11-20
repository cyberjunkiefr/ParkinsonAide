//! Capture micro (cpal) et lecture audio (rodio).
//!
//! Le flux d'entrée reste ouvert en permanence : quand le patient appuie sur le
//! bouton d'enregistrement, on active simplement la capture. Cela évite de
//! perdre le début de la phrase à cause du temps d'ouverture du périphérique.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use hound::{WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Fréquence d'échantillonnage attendue par Vosk.
pub const STT_SAMPLE_RATE: u32 = 16_000;

/// Quel canal du micro utiliser (le XVF3800 fournit 2 canaux traités).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ChannelMode {
    #[default]
    Mix,
    Left,
    Right,
}

impl ChannelMode {
    pub fn label(self) -> &'static str {
        match self {
            ChannelMode::Mix => "Mélange des canaux",
            ChannelMode::Left => "Canal gauche",
            ChannelMode::Right => "Canal droit",
        }
    }
}

#[derive(Default)]
struct Shared {
    capturing: AtomicBool,
    failed: AtomicBool,
    buf: Mutex<Vec<f32>>,
}

pub struct Recorder {
    stream: Option<cpal::Stream>,
    device_name: String,
    channels: u16,
    sample_rate: u32,
    shared: Arc<Shared>,
}

/// Noms des micros disponibles.
pub fn list_input_devices() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|it| it.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

fn is_xvf(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("respeaker") || n.contains("xvf")
}

/// Choisit le micro : celui demandé, sinon un ReSpeaker XVF3800, sinon le micro par défaut.
fn pick_device(preferred: Option<&str>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    let devices: Vec<cpal::Device> = host.input_devices().map(|d| d.collect()).unwrap_or_default();
    if let Some(pref) = preferred {
        if let Some(d) = devices.iter().find(|d| d.name().map(|n| n == pref).unwrap_or(false)) {
            return Some(d.clone());
        }
    }
    if let Some(d) = devices.iter().find(|d| d.name().map(|n| is_xvf(&n)).unwrap_or(false)) {
        return Some(d.clone());
    }
    host.default_input_device()
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let err_shared = Arc::clone(&shared);
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            if shared.capturing.load(Ordering::Relaxed) {
                if let Ok(mut buf) = shared.buf.lock() {
                    buf.extend(data.iter().map(|&s| s.to_sample::<f32>()));
                }
            }
        },
        move |e| {
            eprintln!("Erreur flux audio : {e}");
            err_shared.failed.store(true, Ordering::Relaxed);
        },
        None,
    )
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            stream: None,
            device_name: String::new(),
            channels: 1,
            sample_rate: 48_000,
            shared: Arc::new(Shared::default()),
        }
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Ferme le flux (par ex. après changement de micro dans les paramètres).
    pub fn close(&mut self) {
        self.stream = None;
        self.device_name.clear();
    }

    /// Ouvre le flux d'entrée s'il n'est pas déjà ouvert (ou s'il a échoué).
    pub fn ensure_open(&mut self, preferred: Option<&str>) -> Result<(), String> {
        if self.stream.is_some() && !self.shared.failed.load(Ordering::Relaxed) {
            return Ok(());
        }
        self.stream = None;
        self.shared = Arc::new(Shared::default());

        let device = pick_device(preferred).ok_or("Aucun microphone trouvé")?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("Configuration du micro impossible : {e}"))?;
        let stream_config: cpal::StreamConfig = config.clone().into();
        let shared = Arc::clone(&self.shared);

        let stream = match config.sample_format() {
            SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, shared),
            SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, shared),
            SampleFormat::I32 => build_stream::<i32>(&device, &stream_config, shared),
            SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, shared),
            SampleFormat::U8 => build_stream::<u8>(&device, &stream_config, shared),
            fmt => return Err(format!("Format audio non supporté : {fmt:?}")),
        }
        .map_err(|e| format!("Ouverture du micro impossible : {e}"))?;
        stream
            .play()
            .map_err(|e| format!("Démarrage du micro impossible : {e}"))?;

        self.channels = stream_config.channels.max(1);
        self.sample_rate = stream_config.sample_rate.0;
        self.device_name = device.name().unwrap_or_default();
        self.stream = Some(stream);
        Ok(())
    }

    /// Démarre la capture.
    pub fn begin(&mut self, preferred: Option<&str>) -> Result<(), String> {
        self.ensure_open(preferred)?;
        if let Ok(mut buf) = self.shared.buf.lock() {
            buf.clear();
        }
        self.shared.capturing.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Arrête la capture et renvoie le signal mono à 16 kHz.
    pub fn end(&mut self, mode: ChannelMode) -> Vec<f32> {
        self.shared.capturing.store(false, Ordering::Relaxed);
        let raw = self
            .shared
            .buf
            .lock()
            .map(|mut b| std::mem::take(&mut *b))
            .unwrap_or_default();
        let mono = to_mono(&raw, self.channels, mode);
        resample(&mono, self.sample_rate, STT_SAMPLE_RATE)
    }
}

/// Convertit un signal entrelacé multi-canaux en mono.
pub fn to_mono(samples: &[f32], channels: u16, mode: ChannelMode) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if ch == 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(ch)
        .map(|frame| match mode {
            ChannelMode::Mix => frame.iter().sum::<f32>() / ch as f32,
            ChannelMode::Left => frame[0],
            ChannelMode::Right => frame[1],
        })
        .collect()
}

/// Ré-échantillonnage simple (moyenne sur la fenêtre pour la décimation,
/// interpolation linéaire sinon).
pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() || from_rate == 0 {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let new_len = (samples.len() as f64 / ratio).floor() as usize;
    (0..new_len)
        .map(|i| {
            let start = i as f64 * ratio;
            if ratio > 1.0 {
                // Filtre passe-bas grossier : moyenne des échantillons couverts.
                let a = start as usize;
                let b = (((i + 1) as f64 * ratio) as usize).clamp(a + 1, samples.len());
                samples[a..b].iter().sum::<f32>() / (b - a) as f32
            } else {
                let idx = start as usize;
                let frac = (start - idx as f64) as f32;
                let a = samples.get(idx).copied().unwrap_or(0.0);
                let b = samples.get(idx + 1).copied().unwrap_or(a);
                a + (b - a) * frac
            }
        })
        .collect()
}

pub fn to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect()
}

/// Enregistre un signal mono en WAV 16 bits.
pub fn save_wav(samples: &[i16], sample_rate: u32, path: &Path) -> Result<(), String> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for &s in samples {
        writer.write_sample(s).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())
}

/// Joue un fichier WAV dans un fil séparé. `playing` est vrai pendant la lecture.
pub fn play_wav_async(path: PathBuf, playing: Arc<AtomicBool>, ctx: egui::Context) {
    playing.store(true, Ordering::Relaxed);
    std::thread::spawn(move || {
        let run = || -> Option<()> {
            use rodio::{Decoder, OutputStream, Sink};
            let (_stream, handle) = OutputStream::try_default().ok()?;
            let sink = Sink::try_new(&handle).ok()?;
            let file = std::fs::File::open(&path).ok()?;
            let source = Decoder::new(std::io::BufReader::new(file)).ok()?;
            sink.append(source);
            sink.sleep_until_end();
            Some(())
        };
        let _ = run();
        playing.store(false, Ordering::Relaxed);
        ctx.request_repaint();
    });
}
