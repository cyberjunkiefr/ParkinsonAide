//! Fichier de phrases et calcul du score.

use std::path::Path;

/// Lit un fichier texte (UTF-8, avec repli Windows-1252/Latin-1), une phrase par ligne.
pub fn load(path: &Path) -> Result<Vec<String>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Lecture impossible : {e}"))?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    };
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// Enregistre les phrases (les lignes vides sont ignorées).
pub fn save(path: &Path, phrases: &[String]) -> Result<(), String> {
    let mut out = String::new();
    for p in phrases.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        out.push_str(p);
        out.push('\n');
    }
    std::fs::write(path, out).map_err(|e| format!("Écriture impossible : {e}"))
}

/// Découpe en mots normalisés : minuscules, apostrophes et traits d'union = séparateurs.
fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(String::from)
        .collect()
}

/// Pourcentage (0–100) des mots de la phrase de référence retrouvés dans la transcription.
/// Chaque mot prononcé ne peut correspondre qu'à un seul mot de la référence.
pub fn score(reference: &str, spoken: &str) -> f32 {
    let reference = words(reference);
    if reference.is_empty() {
        return 0.0;
    }
    let mut remaining = words(spoken);
    let mut matches = 0;
    for w in &reference {
        if let Some(pos) = remaining.iter().position(|s| s == w) {
            remaining.swap_remove(pos);
            matches += 1;
        }
    }
    matches as f32 * 100.0 / reference.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_match() {
        assert_eq!(score("J'aime marcher dans le parc.", "j' aime marcher dans le parc"), 100.0);
    }

    #[test]
    fn no_match() {
        assert_eq!(score("Le chat est sur le tapis.", "bonjour"), 0.0);
    }

    #[test]
    fn partial_and_duplicates() {
        // "le" apparaît 2 fois dans la référence, une seule fois prononcé.
        assert_eq!(score("le chat le tapis", "le chat"), 50.0);
    }

    #[test]
    fn empty_reference() {
        assert_eq!(score("", "quelque chose"), 0.0);
    }
}
