//! Reconnaissance vocale hors ligne avec Vosk (open source, modèle français).

use std::path::{Path, PathBuf};

pub struct Stt {
    #[cfg(feature = "stt")]
    model: vosk::Model,
}

impl Stt {
    pub fn new(model_path: &Path) -> Result<Self, String> {
        #[cfg(feature = "stt")]
        {
            if !model_path.join("am").join("final.mdl").exists() {
                return Err(format!(
                    "Le dossier « {} » ne contient pas de modèle Vosk (am/final.mdl manquant).",
                    model_path.display()
                ));
            }
            vosk::set_log_level(vosk::LogLevel::Error);
            let path_str = model_path
                .to_str()
                .ok_or("Chemin de modèle invalide (caractères non UTF-8)")?;
            let model = vosk::Model::new(path_str).ok_or_else(|| {
                "Impossible de charger le modèle Vosk. Vérifiez le dossier du modèle.".to_string()
            })?;
            Ok(Self { model })
        }
        #[cfg(not(feature = "stt"))]
        {
            let _ = model_path;
            Err("La reconnaissance vocale n'est pas compilée (fonctionnalité « stt » désactivée).".to_string())
        }
    }

    /// Transcrit un signal mono 16 kHz en texte français.
    pub fn transcribe(&self, samples: &[i16]) -> String {
        #[cfg(feature = "stt")]
        {
            let Some(mut rec) = vosk::Recognizer::new(&self.model, crate::audio::STT_SAMPLE_RATE as f32)
            else {
                return String::new();
            };
            let mut parts: Vec<String> = Vec::new();
            let mut push = |r: vosk::CompleteResult| {
                let t = match r {
                    vosk::CompleteResult::Single(s) => s.text.to_string(),
                    vosk::CompleteResult::Multiple(m) => m
                        .alternatives
                        .first()
                        .map(|a| a.text.to_string())
                        .unwrap_or_default(),
                };
                if !t.trim().is_empty() {
                    parts.push(t.trim().to_string());
                }
            };
            // Par blocs : Vosk peut détecter une fin d'énoncé au milieu (pause).
            for chunk in samples.chunks(4000) {
                if let vosk::DecodingState::Finalized = rec.accept_waveform(chunk) {
                    push(rec.result());
                }
            }
            push(rec.final_result());
            parts.join(" ")
        }
        #[cfg(not(feature = "stt"))]
        {
            let _ = samples;
            String::new()
        }
    }
}

fn is_model_dir(p: &Path) -> bool {
    p.join("am").join("final.mdl").exists()
}

/// Cherche un modèle français à côté de l'exécutable (ou dans le dossier du projet).
pub fn find_default_model() -> Option<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        // exe dir, puis remonte (target/debug -> racine du projet)
        bases.extend(exe.ancestors().skip(1).take(4).map(Path::to_path_buf));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }
    for base in &bases {
        for name in ["vosk_small_fr", "vosk_fr", "model"] {
            let p = base.join(name);
            if is_model_dir(&p) {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(all(test, feature = "stt"))]
mod tests {
    /// Test manuel : `PA_TEST_WAV=fichier.wav cargo test -- --ignored`
    /// (WAV mono 16 kHz 16 bits).
    #[test]
    #[ignore]
    fn transcribe_wav() {
        let wav = std::env::var("PA_TEST_WAV").expect("PA_TEST_WAV non défini");
        let samples: Vec<i16> = hound::WavReader::open(wav)
            .unwrap()
            .samples::<i16>()
            .map(Result::unwrap)
            .collect();
        let model = super::find_default_model().expect("modèle introuvable");
        let stt = super::Stt::new(&model).unwrap();
        let text = stt.transcribe(&samples);
        println!("Transcription : {text}");
        assert!(!text.is_empty());
    }
}
