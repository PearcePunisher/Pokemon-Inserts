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
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

const PREVIEW_SCALE: f32 = 0.8;
/// Render scale for the small inserts shown in the page preview.
const THUMB_SCALE: f32 = 0.3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Insert,
    Page,
}

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
    spacing: pdf::Spacing,
    save_pngs: bool,
    out_root: PathBuf,

    view: View,
    page_index: usize,
    thumbs: HashMap<usize, TextureHandle>,
    thumbs_settings: Option<RenderSettings>,

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
            spacing: Default::default(),
            save_pngs: true,
            out_root,
            view: View::Insert,
            page_index: 0,
            thumbs: HashMap::new(),
            thumbs_settings: None,
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
            spacing: self.spacing,
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
                    self.page_index = 0;
                    self.thumbs.clear();
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

    /// Renders small inserts for the cards on the page being previewed.
    fn update_page_thumbs(&mut self, ctx: &egui::Context) {
        let Some(set) = &self.set else { return };
        if self.thumbs_settings.as_ref() != Some(&self.settings) {
            self.thumbs.clear();
            self.thumbs_settings = Some(self.settings.clone());
        }
        let pages = pdf::layout(set.cards.len(), self.double_sided);
        let page = self.page_index.min(pages.len() - 1);
        let needs_art = self.settings.style == Style::Art;
        for i in pages[page].iter().flatten().copied() {
            if self.thumbs.contains_key(&i) {
                continue;
            }
            let art = self.cache.lock().unwrap().get(&i).cloned();
            if needs_art && art.is_none() {
                if !self.art_fetching.contains(&i) && !self.art_failed.contains(&i) {
                    self.art_fetching.insert(i);
                    pipeline::load_preview_art(set, i, self.cache.clone(), self.notifier.clone());
                }
                if !self.art_failed.contains(&i) {
                    continue;
                }
            }
            let img = render::render(&set.cards[i], &set.code, art.as_deref(), &self.settings, THUMB_SCALE, true);
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [img.width() as usize, img.height() as usize],
                img.as_raw(),
            );
            self.thumbs.insert(i, ctx.load_texture(format!("thumb{i}"), color, TextureOptions::LINEAR));
        }
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
        ui.add_space(8.0);
        ui.label("Spacing between inserts");
        ui.checkbox(&mut self.spacing.no_h_gap, "No horizontal gap (columns touch)");
        ui.checkbox(&mut self.spacing.no_v_gap, "No vertical gap (rows touch)");
        ui.label(
            RichText::new("Without a gap, neighbouring inserts share an edge, so one cut separates them.")
                .weak()
                .small(),
        );
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
        let mut view = self.view;
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.heading(&set.title);
            ui.label(RichText::new(format!("{count} cards")).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.selectable_value(&mut view, View::Page, "Page");
                ui.selectable_value(&mut view, View::Insert, "Insert");
            });
        });
        if view != self.view {
            if view == View::Page {
                // Jump to the page holding the insert being previewed.
                self.page_index = self.preview_index / 9;
            }
            self.view = view;
        }
        match self.view {
            View::Insert => self.insert_preview(ui),
            View::Page => self.page_preview(ui),
        }
    }

    fn insert_preview(&mut self, ui: &mut egui::Ui) {
        let Some(set) = &self.set else { return };
        let count = set.cards.len();
        let mut idx = self.preview_index;
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

    /// Shows a whole printed page exactly as it will be laid out in the PDF.
    fn page_preview(&mut self, ui: &mut egui::Ui) {
        let Some(set) = &self.set else { return };
        let pages = pdf::layout(set.cards.len(), self.double_sided);
        let total = pages.len();
        let mut p = self.page_index.min(total - 1);
        ui.horizontal(|ui| {
            if ui.add_enabled(p > 0, egui::Button::new("◀")).clicked() {
                p -= 1;
            }
            let mut label = format!("Page {} of {total}", p + 1);
            if self.double_sided {
                let side = if p.is_multiple_of(2) { "front" } else { "back" };
                label += &format!("  ·  sheet {}, {side}", p / 2 + 1);
            }
            ui.label(label);
            if ui.add_enabled(p + 1 < total, egui::Button::new("▶")).clicked() {
                p += 1;
            }
            ui.label(RichText::new("Click an insert to view it larger.").weak().small());
        });
        self.page_index = p;
        ui.add_space(8.0);

        let ratio = pdf::PAGE_W / pdf::PAGE_H;
        let avail = ui.available_size();
        let h = (avail.y - 12.0).min((avail.x - 12.0) / ratio).max(100.0);
        let size = egui::vec2(h * ratio, h);
        let mut clicked = None;
        ui.vertical_centered(|ui| {
            let (page, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(page.translate(egui::vec2(3.0, 3.0)), 2.0, Color32::from_black_alpha(70));
            painter.rect_filled(page, 0.0, Color32::WHITE);
            let k = page.width() / pdf::PAGE_W;
            let rounding = pdf::CORNER_RADIUS * k;
            for (cell, [x, y, w, h]) in pdf::cell_rects(self.spacing).into_iter().enumerate() {
                let r = egui::Rect::from_min_size(page.min + egui::vec2(x * k, y * k), egui::vec2(w * k, h * k));
                let Some(i) = pages[p][cell] else {
                    painter.rect_stroke(r, rounding, egui::Stroke::new(1.0_f32, Color32::from_gray(225)));
                    continue;
                };
                match self.thumbs.get(&i) {
                    Some(tex) => {
                        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                        painter.image(tex.id(), r, uv, Color32::WHITE);
                    }
                    None => {
                        painter.rect_filled(r, rounding, Color32::from_gray(235));
                        painter.text(
                            r.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("#{}", set.cards[i].display_number()),
                            egui::FontId::proportional(14.0),
                            Color32::from_gray(150),
                        );
                    }
                }
                let card = &set.cards[i];
                let resp = ui
                    .interact(r, ui.id().with(("page_cell", cell)), egui::Sense::click())
                    .on_hover_text(format!("#{} {}", card.display_number(), card.name))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() {
                    clicked = Some(i);
                }
            }
        });
        if let Some(i) = clicked {
            self.preview_index = i;
            self.view = View::Insert;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages();
        match self.view {
            View::Insert => self.update_preview(ctx),
            View::Page => self.update_page_thumbs(ctx),
        }

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
