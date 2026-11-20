use std::path::PathBuf;

fn main() {
    // Uniquement avec la fonctionnalité "stt" (Vosk).
    if std::env::var("CARGO_FEATURE_STT").is_err() {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let vosk_dir = std::env::var("VOSK_LIB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("vosk"));
    println!("cargo:rustc-link-search=native={}", vosk_dir.display());
    println!("cargo:rerun-if-env-changed=VOSK_LIB_PATH");

    // Copie les DLL Vosk à côté de l'exécutable (target/<profil>/).
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    if let Some(profile_dir) = out.ancestors().nth(3) {
        if let Ok(entries) = std::fs::read_dir(&vosk_dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dll")) {
                    let dest = profile_dir.join(p.file_name().unwrap());
                    if !dest.exists() {
                        let _ = std::fs::copy(&p, &dest);
                    }
                }
            }
        }
    }
}
