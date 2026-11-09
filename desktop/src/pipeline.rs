//! Background work: loading a set, fetching preview art, and generating the
//! inserts + PDF. Results are reported back to the UI over a channel.

use crate::pdf;
use crate::render::{on_white, render, RenderSettings, Style, CARD_H, CARD_W};
use crate::scrape::{self, CardSet};
use image::codecs::jpeg::JpegEncoder;
use image::DynamicImage;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub type ImageCache = Arc<Mutex<HashMap<usize, Arc<DynamicImage>>>>;

pub enum Msg {
    SetLoaded(Result<CardSet, String>),
    PreviewArt(usize, Result<Arc<DynamicImage>, String>),
    Progress { done: usize, total: usize },
    Log(String),
    Finished(Result<PathBuf, String>),
}

#[derive(Clone)]
pub struct Notifier {
    tx: Sender<Msg>,
    ctx: eframe::egui::Context,
}

impl Notifier {
    pub fn new(tx: Sender<Msg>, ctx: eframe::egui::Context) -> Self {
        Self { tx, ctx }
    }
    pub fn send(&self, msg: Msg) {
        let _ = self.tx.send(msg);
        self.ctx.request_repaint();
    }
}

pub fn load_set(url: String, n: Notifier) {
    std::thread::spawn(move || {
        let res = scrape::client().and_then(|c| scrape::fetch_set(&c, &url));
        n.send(Msg::SetLoaded(res.map_err(|e| format!("{e:#}"))));
    });
}

pub fn load_preview_art(set: &CardSet, index: usize, cache: ImageCache, n: Notifier) {
    let card = set.cards[index].clone();
    std::thread::spawn(move || {
        let res = scrape::client()
            .and_then(|c| scrape::download_image(&c, &card))
            .map(Arc::new)
            .map_err(|e| format!("{e:#}"));
        if let Ok(img) = &res {
            cache.lock().unwrap().insert(index, img.clone());
        }
        n.send(Msg::PreviewArt(index, res));
    });
}

pub struct Job {
    pub set: CardSet,
    pub settings: RenderSettings,
    pub double_sided: bool,
    pub save_pngs: bool,
    pub out_root: PathBuf,
}

impl Job {
    pub fn out_dir(&self) -> PathBuf {
        self.out_root.join(&self.set.code)
    }

    pub fn pdf_name(&self) -> String {
        let style = match self.settings.style {
            Style::Art => "",
            Style::PrinterFriendly => "_printer-friendly",
        };
        let sides = if self.double_sided { "_double-sided" } else { "" };
        format!("{}_inserts{style}{sides}.pdf", self.set.code)
    }
}

pub fn generate(job: Job, cache: ImageCache, n: Notifier) {
    std::thread::spawn(move || {
        let res = run(&job, &cache, &n).map_err(|e| format!("{e:#}"));
        n.send(Msg::Finished(res));
    });
}

fn run(job: &Job, cache: &ImageCache, n: &Notifier) -> anyhow::Result<PathBuf> {
    let total = job.set.cards.len();
    let out_dir = job.out_dir();
    let png_dir = out_dir.join("inserts");
    std::fs::create_dir_all(&out_dir)?;
    if job.save_pngs {
        std::fs::create_dir_all(&png_dir)?;
    }

    let needs_art = job.settings.style == Style::Art;
    let client = scrape::client()?;
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let jpegs: Mutex<Vec<Option<Vec<u8>>>> = Mutex::new(vec![None; total]);

    n.send(Msg::Log(format!("Generating {total} inserts for {}…", job.set.title)));
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= total {
                    break;
                }
                let card = &job.set.cards[i];
                let label = format!("#{} {}", card.number, card.name);

                let art = if needs_art {
                    let cached = cache.lock().unwrap().get(&i).cloned();
                    match cached {
                        Some(a) => Some(a),
                        None => match scrape::download_image(&client, card) {
                            Ok(img) => {
                                let img = Arc::new(img);
                                cache.lock().unwrap().insert(i, img.clone());
                                Some(img)
                            }
                            Err(e) => {
                                n.send(Msg::Log(format!("Skipped {label}: {e:#}")));
                                None
                            }
                        },
                    }
                } else {
                    None
                };

                if !needs_art || art.is_some() {
                    let insert = render(card, &job.set.code, art.as_deref(), &job.settings, 1.0, false);
                    if job.save_pngs {
                        let path = png_dir.join(format!("{}.png", card.display_number()));
                        if let Err(e) = insert.save(&path) {
                            n.send(Msg::Log(format!("Could not save {}: {e}", path.display())));
                        }
                    }
                    let mut buf = Vec::new();
                    match JpegEncoder::new_with_quality(&mut buf, 92).encode_image(&on_white(&insert)) {
                        Ok(()) => jpegs.lock().unwrap()[i] = Some(buf),
                        Err(e) => n.send(Msg::Log(format!("Could not encode {label}: {e}"))),
                    }
                }
                let d = done.fetch_add(1, Ordering::SeqCst) + 1;
                n.send(Msg::Progress { done: d, total });
            });
        }
    });

    let jpegs = jpegs.into_inner().unwrap();
    let ok = jpegs.iter().filter(|j| j.is_some()).count();
    if ok == 0 {
        anyhow::bail!("No inserts could be created.");
    }
    n.send(Msg::Log("Building PDF…".into()));
    let bytes = pdf::build(&jpegs, CARD_W, CARD_H, job.double_sided);
    let pdf_path = out_dir.join(job.pdf_name());
    std::fs::write(&pdf_path, bytes)?;

    let pages = pdf::layout(total, job.double_sided).len();
    let summary = if job.double_sided {
        format!("{pages} pages ({} sheets, print double-sided, flip on long edge)", pages / 2)
    } else {
        format!("{pages} pages")
    };
    n.send(Msg::Log(format!("Done: {ok}/{total} inserts, {summary}.")));
    if job.save_pngs {
        n.send(Msg::Log(format!("PNG images saved in {}", png_dir.display())));
    }
    Ok(pdf_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Network test: `INSERTS_OUT=/some/dir cargo test --release -- --ignored`
    #[test]
    #[ignore]
    fn generates_dlr_all_modes() {
        let out_root = PathBuf::from(std::env::var("INSERTS_OUT").expect("set INSERTS_OUT"));
        let client = scrape::client().unwrap();
        let set = scrape::fetch_set(&client, "https://limitlesstcg.com/cards/DLR").unwrap();
        assert_eq!(set.code, "DLR");
        assert!(set.cards.len() > 100);
        let cache = ImageCache::default();
        let (tx, rx) = std::sync::mpsc::channel();
        let n = Notifier::new(tx, eframe::egui::Context::default());
        for style in [Style::Art, Style::PrinterFriendly] {
            for double_sided in [false, true] {
                let job = Job {
                    set: set.clone(),
                    settings: RenderSettings { style, blur: 20.0, show_name: true },
                    double_sided,
                    save_pngs: style == Style::PrinterFriendly && !double_sided,
                    out_root: out_root.clone(),
                };
                let pdf = run(&job, &cache, &n).unwrap();
                assert!(pdf.exists());
            }
        }
        for m in rx.try_iter() {
            if let Msg::Log(l) = m {
                println!("{l}");
            }
        }
    }
}
