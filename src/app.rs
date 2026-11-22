use crate::{audio, config::Config, phrases, stt};
use egui::{Color32, Context, FontId, RichText, Rounding, Stroke, TextEdit, Ui, Vec2};
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

const PHRASE_COUNT: usize = 6;
/// Délai entre le relâchement du bouton et l'arrêt réel de l'enregistrement.
const RECORD_TAIL: Duration = Duration::from_millis(500);

const BACKGROUND: Color32 = Color32::from_rgb(150, 150, 150);
const PURPLE: Color32 = Color32::from_rgb(98, 30, 140);
const GREEN: Color32 = Color32::from_rgb(30, 160, 50);
const TIPS_TEXT: Color32 = Color32::from_rgb(248, 248, 248);
const BOLD: &str = "bold";
const TEXT: Color32 = Color32::from_rgb(20, 20, 20);
const CAPTION: Color32 = Color32::from_rgb(120, 120, 120);
const PHRASE_FONT: f32 = 24.0;

const ICON_RECORD: &[u8] = include_bytes!("../icons/record.png");
const ICON_PLAY: &[u8] = include_bytes!("../icons/play-button.png");
const ICON_DOWN: &[u8] = include_bytes!("../icons/down-arrow.png");
const ICON_UP: &[u8] = include_bytes!("../icons/up-arrow.png");

/// Résultat mémorisé pour une phrase du fichier.
#[derive(Clone)]
struct SpeechResult {
    text: String,
    score: f32,
}

enum RecState {
    Idle,
    /// Bouton maintenu ; `idx` = phrase du fichier en cours de lecture.
    Recording { idx: usize },
    /// Bouton relâché, on enregistre encore 0,5 s.
    Tail { idx: usize, until: Instant },
    Transcribing { idx: usize, rx: Receiver<(String, f32)> },
}

enum SttState {
    NotConfigured,
    Loading(Receiver<Result<stt::Stt, String>>, PathBuf),
    Ready(Arc<stt::Stt>),
    Failed(String),
}

pub struct ParkinsonApp {
    config: Config,

    phrases: Vec<String>,
    current_file: Option<PathBuf>,
    dirty: bool,
    /// Index (dans le fichier) de la phrase affichée en Phrase 1.
    offset: usize,
    /// Texte en cours de saisie dans la première ligne vide (ajout d'une phrase).
    new_phrase: String,

    /// Résultats par index de phrase ; remis à zéro au démarrage / chargement.
    results: HashMap<usize, SpeechResult>,
    error: Option<String>,

    rec: RecState,
    recorder: audio::Recorder,
    last_wav: Option<PathBuf>,
    playing: Arc<AtomicBool>,

    stt: SttState,

    tex_record: egui::TextureHandle,
    tex_play: egui::TextureHandle,
    tex_down: egui::TextureHandle,
    tex_up: egui::TextureHandle,

    show_settings: bool,
    model_path_buf: String,
    devices: Vec<String>,
    window_title: String,
}

fn load_texture(ctx: &Context, name: &str, bytes: &[u8]) -> egui::TextureHandle {
    let img = image::load_from_memory(bytes)
        .unwrap_or_else(|_| image::DynamicImage::new_rgba8(32, 32))
        .to_rgba8();
    let (w, h) = img.dimensions();
    let color_img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    ctx.load_texture(name, color_img, egui::TextureOptions::LINEAR)
}

fn row_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(4.0_f32, PURPLE))
        .inner_margin(egui::Margin::symmetric(16.0, 6.0))
        .rounding(Rounding::same(12.0))
}

/// Police grasse (les polices intégrées d'egui n'en ont pas) : Arial Bold de Windows.
fn install_bold_font(ctx: &Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut family: Vec<String> = Vec::new();
    for path in ["C:/Windows/Fonts/arialbd.ttf", "C:/Windows/Fonts/segoeuib.ttf"] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert("bold".into(), egui::FontData::from_owned(bytes));
            family.push("bold".into());
            break;
        }
    }
    // Repli sur les polices normales si aucune police grasse n'est trouvée.
    family.extend(fonts.families[&egui::FontFamily::Proportional].iter().cloned());
    fonts.families.insert(egui::FontFamily::Name(BOLD.into()), family);
    ctx.set_fonts(fonts);
}

fn bold(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name(BOLD.into()))
}

fn score_color(score: f32) -> Color32 {
    if score >= 80.0 {
        Color32::from_rgb(0, 140, 60)
    } else if score >= 50.0 {
        Color32::from_rgb(210, 130, 0)
    } else {
        Color32::from_rgb(190, 0, 0)
    }
}

fn file_name(path: &Option<PathBuf>) -> String {
    path.as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Sans titre".to_string())
}

impl ParkinsonApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        ctx.set_visuals(egui::Visuals::light());
        install_bold_font(ctx);
        ctx.style_mut(|s| {
            s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(17.0));
            s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(17.0));
        });

        let config = Config::load();

        let mut app = Self {
            phrases: Vec::new(),
            current_file: None,
            dirty: false,
            offset: 0,
            new_phrase: String::new(),
            results: HashMap::new(),
            error: None,
            rec: RecState::Idle,
            recorder: audio::Recorder::new(),
            last_wav: None,
            playing: Arc::new(AtomicBool::new(false)),
            stt: SttState::NotConfigured,
            tex_record: load_texture(ctx, "record", ICON_RECORD),
            tex_play: load_texture(ctx, "play", ICON_PLAY),
            tex_down: load_texture(ctx, "down", ICON_DOWN),
            tex_up: load_texture(ctx, "up", ICON_UP),
            show_settings: false,
            model_path_buf: String::new(),
            devices: Vec::new(),
            window_title: String::new(),
            config,
        };

        // Dernier fichier utilisé (au premier lancement : phrases.txt s'il existe,
        // d'abord dans Documents\ParkinsonAide où l'installateur le dépose).
        let first = app.config.last_file.clone().filter(|p| p.exists()).or_else(|| {
            let mut bases: Vec<PathBuf> =
                dirs::document_dir().map(|d| d.join("ParkinsonAide")).into_iter().collect();
            bases.extend(std::env::current_dir());
            if let Ok(exe) = std::env::current_exe() {
                bases.extend(exe.ancestors().skip(1).take(4).map(Path::to_path_buf));
            }
            bases.into_iter().map(|b| b.join("phrases.txt")).find(|p| p.exists())
        });
        if let Some(path) = first {
            if let Err(e) = app.load_file(&path) {
                app.error = Some(format!("{} : {e}", path.display()));
            }
        }

        // Modèle de reconnaissance vocale : configuré, sinon détection automatique.
        let model = app
            .config
            .vosk_model_path
            .clone()
            .filter(|p| p.exists())
            .or_else(stt::find_default_model);
        if let Some(p) = model {
            app.model_path_buf = p.to_string_lossy().to_string();
            app.start_loading_model(p, ctx.clone());
        }

        // Ouvre le micro tout de suite (évite de couper le début du premier enregistrement).
        if let Err(e) = app.recorder.ensure_open(app.config.input_device.as_deref()) {
            app.error = Some(e);
        }
        app
    }

    // ── modèle STT ───────────────────────────────────────────────────────────
    fn start_loading_model(&mut self, path: PathBuf, ctx: Context) {
        let (tx, rx) = mpsc::channel();
        let p = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(stt::Stt::new(&p));
            ctx.request_repaint();
        });
        self.stt = SttState::Loading(rx, path);
    }

    fn poll_model(&mut self) {
        if let SttState::Loading(rx, path) = &self.stt {
            if let Ok(res) = rx.try_recv() {
                let path = path.clone();
                self.stt = match res {
                    Ok(s) => {
                        self.config.vosk_model_path = Some(path);
                        self.config.save();
                        SttState::Ready(Arc::new(s))
                    }
                    Err(e) => SttState::Failed(e),
                };
            }
        }
    }

    fn stt_status(&self) -> String {
        match &self.stt {
            SttState::NotConfigured => "Aucun modèle Vosk trouvé. Choisissez le dossier du modèle français.".into(),
            SttState::Loading(_, p) => format!("Chargement du modèle : {} …", p.display()),
            SttState::Ready(_) => format!("Modèle chargé : {}", self.model_path_buf),
            SttState::Failed(e) => format!("Erreur : {e}"),
        }
    }

    // ── navigation ───────────────────────────────────────────────────────────
    fn can_go_down(&self) -> bool {
        self.offset > 0
    }

    /// Désactivé quand la dernière phrase du fichier est dans Phrase 1.
    fn can_go_up(&self) -> bool {
        self.offset + 1 < self.phrases.len()
    }

    fn busy_recording(&self) -> bool {
        matches!(self.rec, RecState::Recording { .. } | RecState::Tail { .. })
    }

    // ── enregistrement ───────────────────────────────────────────────────────
    fn handle_record_button(&mut self, held: bool, ctx: &Context) {
        match self.rec {
            RecState::Idle if held => {
                if self.offset >= self.phrases.len() {
                    return;
                }
                self.error = None;
                match self.recorder.begin(self.config.input_device.as_deref()) {
                    Ok(()) => {
                        self.results.remove(&self.offset);
                        self.rec = RecState::Recording { idx: self.offset };
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            RecState::Recording { idx } if !held => {
                self.rec = RecState::Tail { idx, until: Instant::now() + RECORD_TAIL };
            }
            RecState::Tail { idx, until } => {
                let now = Instant::now();
                if now >= until {
                    self.finish_recording(idx, ctx);
                } else {
                    ctx.request_repaint_after(until - now);
                }
            }
            _ => {}
        }
        if let RecState::Recording { .. } = self.rec {
            ctx.request_repaint();
        }
    }

    fn finish_recording(&mut self, idx: usize, ctx: &Context) {
        self.rec = RecState::Idle;
        let samples = audio::to_i16(&self.recorder.end(self.config.channel_mode));
        if samples.len() < (audio::STT_SAMPLE_RATE as usize) / 4 {
            self.error = Some("Enregistrement trop court : maintenez le bouton rouge pendant la lecture.".into());
            return;
        }

        let wav = std::env::temp_dir().join("parkinson_aide_enregistrement.wav");
        match audio::save_wav(&samples, audio::STT_SAMPLE_RATE, &wav) {
            Ok(()) => self.last_wav = Some(wav),
            Err(e) => self.error = Some(format!("Sauvegarde de l'enregistrement impossible : {e}")),
        }

        let SttState::Ready(model) = &self.stt else {
            self.error = Some(match &self.stt {
                SttState::Loading(..) => "Le modèle de reconnaissance vocale est encore en cours de chargement. Réessayez dans un instant.".into(),
                _ => "Reconnaissance vocale indisponible : configurez le modèle dans Paramètres.".into(),
            });
            return;
        };
        let model = Arc::clone(model);
        let reference = self.phrases.get(idx).cloned().unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let text = model.transcribe(&samples);
            let score = phrases::score(&reference, &text);
            let _ = tx.send((text, score));
            ctx.request_repaint();
        });
        self.rec = RecState::Transcribing { idx, rx };
    }

    fn poll_transcription(&mut self) {
        if let RecState::Transcribing { idx, rx } = &self.rec {
            match rx.try_recv() {
                Ok((text, score)) => {
                    self.results.insert(*idx, SpeechResult { text, score });
                    self.rec = RecState::Idle;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("La transcription a échoué.".into());
                    self.rec = RecState::Idle;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    // ── fichiers ─────────────────────────────────────────────────────────────
    fn load_file(&mut self, path: &Path) -> Result<(), String> {
        let p = phrases::load(path)?;
        self.phrases = p;
        self.current_file = Some(path.to_path_buf());
        self.reset_session();
        self.config.last_file = Some(path.to_path_buf());
        self.config.save();
        Ok(())
    }

    fn reset_session(&mut self) {
        self.offset = 0;
        self.dirty = false;
        self.results.clear();
        self.new_phrase.clear();
        self.error = None;
    }

    /// Demande s'il faut enregistrer les modifications. Renvoie `false` si l'utilisateur annule.
    fn confirm_discard(&mut self) -> bool {
        if !self.dirty {
            return true;
        }
        let answer = MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title("Modifications non enregistrées")
            .set_description(format!(
                "Le fichier « {} » a été modifié.\nVoulez-vous enregistrer les modifications ?",
                file_name(&self.current_file)
            ))
            .set_buttons(MessageButtons::YesNoCancel)
            .show();
        match answer {
            MessageDialogResult::Yes => self.save_file(),
            MessageDialogResult::No => true,
            _ => false,
        }
    }

    fn new_file(&mut self) {
        if !self.confirm_discard() {
            return;
        }
        self.phrases.clear();
        self.current_file = None;
        self.reset_session();
    }

    fn open_file(&mut self) {
        if !self.confirm_discard() {
            return;
        }
        let mut dlg = FileDialog::new()
            .add_filter("Fichier texte", &["txt"])
            .add_filter("Tous les fichiers", &["*"])
            .set_title("Ouvrir un fichier de phrases");
        if let Some(dir) = self.current_file.as_ref().and_then(|p| p.parent()) {
            dlg = dlg.set_directory(dir);
        }
        if let Some(path) = dlg.pick_file() {
            if let Err(e) = self.load_file(&path) {
                self.error = Some(e);
            }
        }
    }

    /// Renvoie `true` si le fichier a été enregistré.
    fn save_file(&mut self) -> bool {
        match self.current_file.clone() {
            Some(path) => self.write_file(&path),
            None => self.save_file_as(),
        }
    }

    fn save_file_as(&mut self) -> bool {
        let mut dlg = FileDialog::new()
            .add_filter("Fichier texte", &["txt"])
            .set_title("Enregistrer sous")
            .set_file_name(file_name(&self.current_file).replace("Sans titre", "phrases.txt"));
        if let Some(dir) = self.current_file.as_ref().and_then(|p| p.parent()) {
            dlg = dlg.set_directory(dir);
        }
        let Some(mut path) = dlg.save_file() else { return false };
        if path.extension().is_none() {
            path.set_extension("txt");
        }
        if self.write_file(&path) {
            self.current_file = Some(path.clone());
            self.config.last_file = Some(path);
            self.config.save();
            true
        } else {
            false
        }
    }

    fn write_file(&mut self, path: &Path) -> bool {
        match phrases::save(path, &self.phrases) {
            Ok(()) => {
                self.dirty = false;
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    // ── édition ──────────────────────────────────────────────────────────────
    fn insert_phrase(&mut self, at: usize) {
        let at = at.min(self.phrases.len());
        self.phrases.insert(at, String::new());
        self.results = self
            .results
            .drain()
            .map(|(k, v)| (if k >= at { k + 1 } else { k }, v))
            .collect();
        self.dirty = true;
    }

    fn delete_phrase(&mut self, at: usize) {
        if at >= self.phrases.len() {
            return;
        }
        self.phrases.remove(at);
        self.results.remove(&at);
        self.results = self
            .results
            .drain()
            .map(|(k, v)| (if k > at { k - 1 } else { k }, v))
            .collect();
        if self.offset > 0 && self.offset >= self.phrases.len() {
            self.offset = self.phrases.len() - 1;
        }
        self.dirty = true;
    }

    // ── interface ────────────────────────────────────────────────────────────
    fn menu_bar(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Fichier", |ui| {
                    if ui.button("Nouveau").clicked() {
                        ui.close_menu();
                        self.new_file();
                    }
                    if ui.button("Ouvrir…").clicked() {
                        ui.close_menu();
                        self.open_file();
                    }
                    if ui.button("Enregistrer").clicked() {
                        ui.close_menu();
                        self.save_file();
                    }
                    if ui.button("Enregistrer sous…").clicked() {
                        ui.close_menu();
                        self.save_file_as();
                    }
                    ui.separator();
                    if ui.button("Quitter").clicked() {
                        ui.close_menu();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Édition", |ui| {
                    let has = self.offset < self.phrases.len();
                    if ui.button("Insérer une phrase avant la phrase 1").clicked() {
                        ui.close_menu();
                        self.insert_phrase(self.offset);
                    }
                    if ui.button("Insérer une phrase après la phrase 1").clicked() {
                        ui.close_menu();
                        self.insert_phrase(self.offset + 1);
                    }
                    if ui.add_enabled(has, egui::Button::new("Supprimer la phrase 1")).clicked() {
                        ui.close_menu();
                        self.delete_phrase(self.offset);
                    }
                    ui.separator();
                    if ui.button("Effacer les résultats").clicked() {
                        ui.close_menu();
                        self.results.clear();
                    }
                });
                ui.menu_button("Paramètres", |ui| {
                    if ui.button("Micro et reconnaissance vocale…").clicked() {
                        ui.close_menu();
                        self.devices = audio::list_input_devices();
                        self.show_settings = true;
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let star = if self.dirty { " *" } else { "" };
                    ui.label(RichText::new(format!("{}{star}", file_name(&self.current_file))).weak());
                    if self.offset < self.phrases.len() {
                        ui.label(
                            RichText::new(format!("Phrase {} / {}", self.offset + 1, self.phrases.len())).weak(),
                        );
                    }
                });
            });
        });
    }

    fn settings_window(&mut self, ctx: &Context) {
        let mut open = self.show_settings;
        egui::Window::new("Paramètres")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.heading("Microphone");
                let current = self.config.input_device.clone();
                let shown = current.clone().unwrap_or_else(|| "Automatique (ReSpeaker XVF3800 si présent)".into());
                let mut changed = false;
                egui::ComboBox::from_id_source("device")
                    .selected_text(shown)
                    .width(480.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(current.is_none(), "Automatique (ReSpeaker XVF3800 si présent)").clicked() {
                            self.config.input_device = None;
                            changed = true;
                        }
                        for d in &self.devices {
                            if ui.selectable_label(current.as_deref() == Some(d), d).clicked() {
                                self.config.input_device = Some(d.clone());
                                changed = true;
                            }
                        }
                    });
                egui::ComboBox::from_id_source("channel")
                    .selected_text(self.config.channel_mode.label())
                    .width(480.0)
                    .show_ui(ui, |ui| {
                        for m in [audio::ChannelMode::Mix, audio::ChannelMode::Left, audio::ChannelMode::Right] {
                            if ui.selectable_value(&mut self.config.channel_mode, m, m.label()).changed() {
                                self.config.save();
                            }
                        }
                    });
                if changed && !self.busy_recording() {
                    self.config.save();
                    self.recorder.close();
                    if let Err(e) = self.recorder.ensure_open(self.config.input_device.as_deref()) {
                        self.error = Some(e);
                    }
                }
                let name = self.recorder.device_name();
                ui.label(if name.is_empty() { "Micro utilisé : aucun".to_string() } else { format!("Micro utilisé : {name}") });

                ui.add_space(10.0);
                ui.separator();
                ui.heading("Reconnaissance vocale (Vosk, français)");
                ui.horizontal(|ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.model_path_buf)
                            .hint_text("Dossier du modèle, ex. vosk_small_fr")
                            .desired_width(380.0),
                    );
                    if ui.button("Parcourir…").clicked() {
                        if let Some(p) = FileDialog::new().set_title("Dossier du modèle Vosk").pick_folder() {
                            self.model_path_buf = p.to_string_lossy().to_string();
                        }
                    }
                });
                let loading = matches!(self.stt, SttState::Loading(..));
                if ui.add_enabled(!loading, egui::Button::new("Charger le modèle")).clicked() {
                    self.start_loading_model(PathBuf::from(self.model_path_buf.trim()), ctx.clone());
                }
                ui.label(self.stt_status());
                ui.label(
                    RichText::new("Le petit modèle (vosk_small_fr) se charge vite ; le grand (vosk_fr) est plus précis mais demande plus de mémoire et de temps.")
                        .small()
                        .weak(),
                );
            });
        self.show_settings = open;
    }

    fn caption(ui: &mut Ui, text: &str) {
        ui.label(RichText::new(text).size(13.0).color(CAPTION));
    }

    fn phrase_row(&mut self, ui: &mut Ui, n: usize) {
        let idx = self.offset + n - 1;
        row_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(64.0);
            Self::caption(ui, &format!("Phrase {n}"));
            let font = FontId::proportional(PHRASE_FONT);
            if idx < self.phrases.len() {
                let mut text = self.phrases[idx].clone();
                let resp = ui.add(
                    TextEdit::multiline(&mut text)
                        .font(font)
                        .text_color(TEXT)
                        .frame(false)
                        .desired_rows(1)
                        .desired_width(f32::INFINITY),
                );
                if resp.changed() {
                    self.phrases[idx] = text.replace(['\r', '\n'], " ");
                    self.results.remove(&idx);
                    self.dirty = true;
                }
            } else if idx == self.phrases.len() {
                // Première ligne vide : taper ici ajoute une phrase au fichier.
                let resp = ui.add(
                    TextEdit::multiline(&mut self.new_phrase)
                        .font(font)
                        .text_color(TEXT)
                        .frame(false)
                        .desired_rows(1)
                        .hint_text("Nouvelle phrase…")
                        .desired_width(f32::INFINITY),
                );
                if resp.changed() && !self.new_phrase.trim().is_empty() {
                    let t = std::mem::take(&mut self.new_phrase).replace(['\r', '\n'], " ");
                    self.phrases.push(t);
                    self.dirty = true;
                    // Garde le focus sur la même ligne (devenue une phrase existante).
                    resp.request_focus();
                }
            } else {
                ui.add_space(PHRASE_FONT + 6.0);
            }
        });
    }

    fn icon_button(&self, ui: &mut Ui, tex: &egui::TextureHandle, enabled: bool, selected: bool, tip: &str) -> egui::Response {
        let mut img = egui::Image::new(egui::load::SizedTexture::new(tex.id(), Vec2::splat(56.0)));
        if !enabled {
            img = img.tint(Color32::from_white_alpha(70));
        }
        ui.add_enabled(enabled, egui::ImageButton::new(img).frame(false).selected(selected))
            .on_hover_text(tip)
            .on_disabled_hover_text(tip)
    }

    fn speech_row(&mut self, ui: &mut Ui, ctx: &Context) {
        row_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(76.0);
            ui.horizontal(|ui| {
                let buttons_w = 4.0 * 64.0;
                let text_w = (ui.available_width() - buttons_w - 110.0).max(100.0);

                // Texte reconnu / état
                ui.vertical(|ui| {
                    ui.set_width(text_w);
                    Self::caption(ui, "Parole");
                    let result = self.results.get(&self.offset).cloned();
                    let (msg, color) = match (&self.rec, &self.error, &result) {
                        (RecState::Recording { .. }, ..) => ("● Enregistrement… lisez la phrase 1 à voix haute.".to_string(), Color32::from_rgb(200, 0, 0)),
                        (RecState::Tail { .. }, ..) => ("● Fin de l'enregistrement…".to_string(), Color32::from_rgb(200, 0, 0)),
                        (RecState::Transcribing { .. }, ..) => ("Reconnaissance en cours…".to_string(), CAPTION),
                        (_, Some(e), _) => (e.clone(), Color32::from_rgb(170, 0, 0)),
                        (_, _, Some(r)) if r.text.is_empty() => ("(aucune parole reconnue)".to_string(), CAPTION),
                        (_, _, Some(r)) => (r.text.clone(), TEXT),
                        _ if matches!(self.stt, SttState::Loading(..)) => ("Chargement du modèle de reconnaissance vocale…".to_string(), CAPTION),
                        _ => ("Maintenez le bouton rouge et lisez la phrase 1 à voix haute.".to_string(), CAPTION),
                    };
                    let size = if result.is_some() && self.error.is_none() && matches!(self.rec, RecState::Idle) { PHRASE_FONT } else { 18.0 };
                    ui.add(egui::Label::new(RichText::new(msg).size(size).color(color)).wrap());
                });

                // Score
                ui.vertical(|ui| {
                    ui.set_width(100.0);
                    if let Some(r) = self.results.get(&self.offset) {
                        Self::caption(ui, "Score");
                        ui.label(RichText::new(format!("{:.0} %", r.score)).size(30.0).strong().color(score_color(r.score)));
                    }
                });

                // Boutons
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let idle = matches!(self.rec, RecState::Idle | RecState::Transcribing { .. });
                    let recording = self.busy_recording();

                    let up = self.icon_button(ui, &self.tex_up, self.can_go_up() && idle, false, "Phrase suivante");
                    if up.clicked() {
                        self.offset += 1;
                        self.error = None;
                    }
                    let down = self.icon_button(ui, &self.tex_down, self.can_go_down() && idle, false, "Phrase précédente");
                    if down.clicked() {
                        self.offset -= 1;
                        self.error = None;
                    }
                    let can_play = self.last_wav.is_some() && !recording && !self.playing.load(Ordering::Relaxed);
                    if self.icon_button(ui, &self.tex_play, can_play, false, "Écouter le dernier enregistrement").clicked() {
                        if let Some(p) = self.last_wav.clone() {
                            audio::play_wav_async(p, Arc::clone(&self.playing), ctx.clone());
                        }
                    }
                    let can_record = self.offset < self.phrases.len()
                        && !matches!(self.rec, RecState::Transcribing { .. })
                        && !self.playing.load(Ordering::Relaxed);
                    let rec = self.icon_button(
                        ui,
                        &self.tex_record,
                        can_record || recording,
                        recording,
                        "Maintenir enfoncé pendant la lecture de la phrase 1",
                    );
                    // egui « oublie » l'appui sur un bouton après ~0,8 s (ce n'est plus un clic) :
                    // on démarre sur l'appui du bouton, puis on suit l'état du bouton de la souris.
                    let held = if matches!(self.rec, RecState::Recording { .. }) {
                        ctx.input(|i| i.pointer.primary_down())
                    } else {
                        rec.is_pointer_button_down_on()
                    };
                    self.handle_record_button(held, ctx);
                });
            });
        });
    }

    /// Conseils pour le patient (en bas à gauche).
    fn tips(ui: &mut Ui) {
        const TIPS: [&str; 4] = [
            "Inspirez bien avant chaque phrase.",
            "Parlez plus fort que d'habitude : ce qui vous semble « trop fort » est souvent juste le bon volume.",
            "Faites-le tous les jours, 10 à 15 minutes, debout ou bien assis droit.",
            "Un orthophoniste peut proposer un programme adapté (par exemple la méthode LSVT LOUD, conçue pour la maladie de Parkinson).",
        ];
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.add_space(14.0);
        ui.label(RichText::new("Conseils pour l'exercice").font(bold(21.0)).color(TIPS_TEXT));
        ui.add_space(10.0);
        // Liste à puces en gras, lignes suivantes alignées sous le texte (retrait suspendu).
        for t in TIPS {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_sized([22.0, 22.0], egui::Label::new(RichText::new("•").font(bold(19.0)).color(TIPS_TEXT)));
                ui.add(egui::Label::new(RichText::new(t).font(bold(19.0)).color(TIPS_TEXT)).wrap());
            });
        }
    }

    /// Moyenne des scores des phrases enregistrées (champ ScoreTotal, en bas à droite).
    fn score_total(&self, ui: &mut Ui) {
        let scores: Vec<f32> = self
            .results
            .iter()
            .filter(|(i, _)| **i < self.phrases.len())
            .map(|(_, r)| r.score)
            .collect();
        let average = (!scores.is_empty()).then(|| scores.iter().sum::<f32>() / scores.len() as f32);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            egui::Frame::none()
                .fill(Color32::WHITE)
                .stroke(Stroke::new(7.0_f32, GREEN))
                .inner_margin(egui::Margin::symmetric(16.0, 6.0))
                .rounding(Rounding::same(14.0))
                .show(ui, |ui| {
                    ui.set_width(260.0);
                    ui.set_min_height(64.0);
                    ui.vertical(|ui| {
                        Self::caption(ui, "Score total");
                        match average {
                            Some(a) => {
                                ui.label(RichText::new(format!("{a:.0} %")).size(30.0).strong().color(score_color(a)));
                                ui.label(
                                    RichText::new(format!("sur {} / {} phrases", scores.len(), self.phrases.len()))
                                        .size(13.0)
                                        .color(CAPTION),
                                );
                            }
                            None => {
                                ui.label(RichText::new("—").size(30.0).color(CAPTION));
                            }
                        }
                    });
                });
        });
    }

    fn update_title(&mut self, ctx: &Context) {
        let star = if self.dirty { " *" } else { "" };
        let title = format!("ParkinsonAide – {}{star}", file_name(&self.current_file));
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
    }
}

impl eframe::App for ParkinsonApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_model();
        self.poll_transcription();

        // Fermeture : proposer d'enregistrer les modifications.
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.confirm_discard() {
                self.dirty = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // Raccourcis clavier
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save_file();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.open_file();
        }

        self.update_title(ctx);
        self.menu_bar(ctx);
        if self.show_settings {
            self.settings_window(ctx);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BACKGROUND).inner_margin(egui::Margin::symmetric(28.0, 20.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;
                    self.phrase_row(ui, 1);
                    self.speech_row(ui, ctx);
                    for n in 2..=PHRASE_COUNT {
                        self.phrase_row(ui, n);
                    }
                    ui.add_space(4.0);
                    ui.horizontal_top(|ui| {
                        let tips_w = (ui.available_width() - 320.0).max(200.0);
                        ui.vertical(|ui| {
                            ui.set_width(tips_w);
                            Self::tips(ui);
                        });
                        self.score_total(ui);
                    });
                });
            });
    }
}
