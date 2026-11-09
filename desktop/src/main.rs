#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod pdf;
mod pipeline;
mod render;
mod scrape;
mod text;

use eframe::egui::{self, Color32, RichText, TextureHandle, TextureOptions};
use pipeline::{ImageCache, Job, Msg, Notifier};
use render::{RenderSettings, Style};
use scrape::CardSet;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

const PREVIEW_SCALE: f32 = 0.8;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Pokémon Inserts")
            .with_inner_size([1040.0, 760.0])
            .with_min_inner_size([860.0, 620.0]),
        ..Default::default()
    };
    // Optional: `pokemon-inserts <set url>` pre-fills and loads the set.
    let initial_url = std::env::args().nth(1);
    eframe::run_native(
        "Pokémon Inserts",
        options,
        Box::new(move |cc| {
            let mut app = App::new(&cc.egui_ctx);
            if let Some(url) = initial_url {
                app.url = url;
                app.start_load();
            }
            Ok(Box::new(app))
        }),
    )
}

struct App {
    url: String,
    set: Option<CardSet>,
    loading_set: bool,

    settings: RenderSettings,
    double_sided: bool,
    save_pngs: bool,
    out_root: PathBuf,

    preview_index: usize,
    preview_tex: Option<TextureHandle>,
    preview_key: Option<(usize, RenderSettings)>,
    art_fetching: HashSet<usize>,
    art_failed: HashSet<usize>,
    cache: ImageCache,

    generating: bool,
    progress: (usize, usize),
    log: Vec<(String, bool)>,
    last_pdf: Option<PathBuf>,

    rx: Receiver<Msg>,
    notifier: Notifier,
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let out_root = dirs::document_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Pokemon Inserts");
        Self {
            url: String::new(),
            set: None,
            loading_set: false,
            settings: RenderSettings { style: Style::Art, blur: 20.0, show_name: false },
            double_sided: false,
            save_pngs: true,
            out_root,
            preview_index: 0,
            preview_tex: None,
            preview_key: None,
            art_fetching: HashSet::new(),
            art_failed: HashSet::new(),
            cache: Default::default(),
            generating: false,
            progress: (0, 0),
            log: Vec::new(),
            last_pdf: None,
            rx,
            notifier: Notifier::new(tx, ctx.clone()),
        }
    }

    fn busy(&self) -> bool {
        self.loading_set || self.generating
    }

    fn log(&mut self, msg: impl Into<String>, is_error: bool) {
        self.log.push((msg.into(), is_error));
    }

    fn start_load(&mut self) {
        if self.busy() || self.url.trim().is_empty() {
            return;
        }
        self.loading_set = true;
        self.log(format!("Loading {}…", self.url.trim()), false);
        pipeline::load_set(self.url.trim().to_string(), self.notifier.clone());
    }

    fn start_generate(&mut self) {
        let Some(set) = self.set.clone() else { return };
        self.generating = true;
        self.last_pdf = None;
        self.progress = (0, set.cards.len());
        let job = Job {
            set,
            settings: self.settings.clone(),
            double_sided: self.double_sided,
            save_pngs: self.save_pngs,
            out_root: self.out_root.clone(),
        };
        pipeline::generate(job, self.cache.clone(), self.notifier.clone());
    }

    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::SetLoaded(Ok(set)) => {
                    self.loading_set = false;
                    self.log(format!("Loaded {} – {} cards.", set.title, set.cards.len()), false);
                    self.cache.lock().unwrap().clear();
                    self.art_fetching.clear();
                    self.art_failed.clear();
                    self.preview_index = 0;
                    self.preview_key = None;
                    self.preview_tex = None;
                    self.last_pdf = None;
                    self.set = Some(set);
                }
                Msg::SetLoaded(Err(e)) => {
                    self.loading_set = false;
                    self.log(e, true);
                }
                Msg::PreviewArt(i, res) => {
                    self.art_fetching.remove(&i);
                    if let Err(e) = res {
                        self.art_failed.insert(i);
                        self.log(format!("Couldn't load art for preview: {e}"), true);
                    }
                }
                Msg::Progress { done, total } => self.progress = (done, total),
                Msg::Log(m) => self.log(m, false),
                Msg::Finished(res) => {
                    self.generating = false;
                    match res {
                        Ok(path) => {
                            self.log(format!("Saved {}", path.display()), false);
                            self.last_pdf = Some(path);
                        }
                        Err(e) => self.log(format!("Failed: {e}"), true),
                    }
                }
            }
        }
    }

    /// Re-renders the preview when the card or settings change.
    fn update_preview(&mut self, ctx: &egui::Context) {
        let Some(set) = &self.set else { return };
        let i = self.preview_index.min(set.cards.len() - 1);
        let key = (i, self.settings.clone());
        if self.preview_key.as_ref() == Some(&key) {
            return;
        }
        let art = self.cache.lock().unwrap().get(&i).cloned();
        let needs_art = self.settings.style == Style::Art;
        if needs_art && art.is_none() {
            if !self.art_fetching.contains(&i) && !self.art_failed.contains(&i) {
                self.art_fetching.insert(i);
                pipeline::load_preview_art(set, i, self.cache.clone(), self.notifier.clone());
            }
            if !self.art_failed.contains(&i) {
                return;
            }
        }
        let img = render::render(&set.cards[i], &set.code, art.as_deref(), &self.settings, PREVIEW_SCALE, true);
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [img.width() as usize, img.height() as usize],
            img.as_raw(),
        );
        self.preview_tex = Some(ctx.load_texture("preview", color, TextureOptions::LINEAR));
        self.preview_key = Some(key);
    }

    fn settings_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.heading("Insert style");
        ui.add_space(4.0);
        ui.radio_value(&mut self.settings.style, Style::Art, "Full art");
        ui.label(RichText::new("Blurred card art with the number on top.").weak().small());
        ui.radio_value(&mut self.settings.style, Style::PrinterFriendly, "Printer-friendly");
        ui.label(RichText::new("White insert with an outline, number and name. Uses very little ink.").weak().small());
        ui.add_space(8.0);

        ui.add_enabled_ui(self.settings.style == Style::Art, |ui| {
            ui.label("Background blur");
            ui.add(egui::Slider::new(&mut self.settings.blur, 0.0..=60.0).step_by(1.0).show_value(true));
        });
        ui.checkbox(&mut self.settings.show_name, "Show card name");

        ui.add_space(12.0);
        ui.separator();
        ui.heading("Printing");
        ui.add_space(4.0);
        ui.radio_value(&mut self.double_sided, false, "Single-sided");
        ui.radio_value(&mut self.double_sided, true, "Double-sided (binder pages)");
        if self.double_sided {
            ui.label(
                RichText::new(
                    "Each sheet makes one 9-pocket binder page: cards 1–9 on the front, 10–18 on \
                     the back. Print double-sided, flip on long edge, at 100% / actual size.",
                )
                .weak()
                .small(),
            );
        } else {
            ui.label(RichText::new("Print at 100% / actual size.").weak().small());
        }
        if let Some(set) = &self.set {
            let pages = pdf::layout(set.cards.len(), self.double_sided).len();
            let sheets = if self.double_sided { pages / 2 } else { pages };
            ui.add_space(4.0);
            ui.label(format!("{} cards → {pages} pages, {sheets} sheets of paper", set.cards.len()));
        }

        ui.add_space(12.0);
        ui.separator();
        ui.heading("Output");
        ui.add_space(4.0);
        ui.checkbox(&mut self.save_pngs, "Also save each insert as a PNG");
        ui.horizontal(|ui| {
            ui.label("Save to:");
            if ui.button("Change…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().set_directory(&self.out_root).pick_folder() {
                    self.out_root = dir;
                }
            }
        });
        ui.label(RichText::new(self.out_root.display().to_string()).small().monospace());

        ui.add_space(16.0);
        let can_generate = self.set.is_some() && !self.busy();
        let button = egui::Button::new(RichText::new("Generate PDF").size(16.0)).min_size(egui::vec2(ui.available_width(), 36.0));
        if ui.add_enabled(can_generate, button).clicked() {
            self.start_generate();
        }

        if self.generating {
            let (d, t) = self.progress;
            let frac = if t == 0 { 0.0 } else { d as f32 / t as f32 };
            ui.add(egui::ProgressBar::new(frac).text(format!("{d} / {t}")).animate(true));
        }
        if let Some(pdf) = self.last_pdf.clone() {
            ui.horizontal(|ui| {
                if ui.button("Open PDF").clicked() {
                    let _ = open::that(&pdf);
                }
                if ui.button("Show folder").clicked() {
                    if let Some(dir) = pdf.parent() {
                        let _ = open::that(dir);
                    }
                }
            });
        }
    }

    fn preview_panel(&mut self, ui: &mut egui::Ui) {
        let Some(set) = &self.set else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Paste a Limitless TCG set URL above and click Load set.\n\ne.g. https://limitlesstcg.com/cards/DLR")
                        .size(15.0)
                        .weak(),
                );
            });
            return;
        };
        let count = set.cards.len();
        let mut idx = self.preview_index;
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.heading(&set.title);
            ui.label(RichText::new(format!("{count} cards")).weak());
        });
        ui.horizontal(|ui| {
            ui.label("Preview:");
            if ui.add_enabled(idx > 0, egui::Button::new("◀")).clicked() {
                idx -= 1;
            }
            let current = &set.cards[idx];
            egui::ComboBox::from_id_salt("preview_card")
                .width(260.0)
                .selected_text(format!("#{} {}", current.display_number(), current.name))
                .show_ui(ui, |ui| {
                    for (i, c) in set.cards.iter().enumerate() {
                        ui.selectable_value(&mut idx, i, format!("#{} {}", c.display_number(), c.name));
                    }
                });
            if ui.add_enabled(idx + 1 < count, egui::Button::new("▶")).clicked() {
                idx += 1;
            }
        });
        self.preview_index = idx;
        ui.add_space(8.0);

        let avail = ui.available_size();
        let h = (avail.y - 8.0).min(avail.x * 1.4).max(100.0);
        let size = egui::vec2(h / 1.4, h);
        ui.vertical_centered(|ui| match &self.preview_tex {
            Some(tex) if self.preview_key.as_ref().is_some_and(|(i, _)| *i == idx) => {
                ui.add(egui::Image::new(tex).fit_to_exact_size(size));
            }
            _ => {
                ui.add_sized(size, egui::Spinner::new());
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages();
        self.update_preview(ctx);

        egui::TopBottomPanel::top("url").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Limitless TCG set URL:");
                let resp = ui.add_enabled(
                    !self.busy(),
                    egui::TextEdit::singleline(&mut self.url)
                        .hint_text("https://limitlesstcg.com/cards/DLR")
                        .desired_width(ui.available_width() - 110.0),
                );
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let label = if self.loading_set { "Loading…" } else { "Load set" };
                if ui.add_enabled(!self.busy(), egui::Button::new(label)).clicked() || enter {
                    self.start_load();
                }
            });
            ui.add_space(8.0);
        });

        egui::TopBottomPanel::bottom("log").resizable(true).default_height(110.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                for (line, is_err) in &self.log {
                    let text = RichText::new(line).small().monospace();
                    ui.label(if *is_err { text.color(Color32::from_rgb(220, 80, 80)) } else { text });
                }
            });
        });

        egui::SidePanel::left("settings").resizable(false).exact_width(300.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_enabled_ui(!self.generating, |ui| self.settings_panel(ui));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| self.preview_panel(ui));
    }
}
