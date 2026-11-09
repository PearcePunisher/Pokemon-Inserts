//! Fetches a set from Limitless TCG using the list view, which carries the
//! card number, name and image URL for every card on a single page.

use anyhow::{anyhow, bail, Context, Result};
use image::DynamicImage;
use reqwest::blocking::Client;
use reqwest::Url;
use scraper::{Html, Selector};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Card {
    /// Number exactly as printed on the site, e.g. "1" or "TG05".
    pub number: String,
    pub name: String,
    /// Candidate image URLs, best quality first.
    pub image_urls: Vec<String>,
}

impl Card {
    /// Number as shown on inserts: purely numeric numbers are zero-padded to 3.
    pub fn display_number(&self) -> String {
        match self.number.parse::<u32>() {
            Ok(n) => format!("{n:03}"),
            Err(_) => self.number.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CardSet {
    pub code: String,
    pub title: String,
    pub cards: Vec<Card>,
}

pub fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent("Mozilla/5.0 (compatible; PokemonInserts/0.1)")
        .timeout(Duration::from_secs(30))
        .build()?)
}

pub fn fetch_set(client: &Client, input_url: &str) -> Result<CardSet> {
    let mut url = Url::parse(input_url.trim()).context("That doesn't look like a valid URL")?;
    if !url.host_str().unwrap_or("").ends_with("limitlesstcg.com") {
        bail!("Please use a limitlesstcg.com cards URL, e.g. https://limitlesstcg.com/cards/DLR");
    }
    // Force the list display, keeping any other query parameters (searches etc.).
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| k != "display")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.query_pairs_mut().clear().extend_pairs(pairs).append_pair("display", "list");

    let code = url
        .path_segments()
        .and_then(|s| s.filter(|p| !p.is_empty()).last())
        .unwrap_or("set")
        .to_uppercase();

    let html = client.get(url.clone()).send()?.error_for_status()?.text()?;
    let doc = Html::parse_document(&html);

    let title = doc
        .select(&Selector::parse("title").unwrap())
        .next()
        .map(|t| t.text().collect::<String>())
        .map(|t| t.split(" – ").next().unwrap_or(&t).trim().to_string())
        .unwrap_or_else(|| code.clone());

    let row_sel = Selector::parse("table.card-list tr[data-hover]").unwrap();
    let td_sel = Selector::parse("td").unwrap();

    let mut cards = Vec::new();
    for row in doc.select(&row_sel) {
        let hover = row.value().attr("data-hover").unwrap_or_default();
        let tds: Vec<_> = row.select(&td_sel).collect();
        if tds.len() < 3 {
            continue;
        }
        let text = |i: usize| tds[i].text().collect::<String>().trim().to_string();
        let (number, name) = (text(1), text(2));
        if number.is_empty() || hover.is_empty() {
            continue;
        }
        cards.push(Card { number, name, image_urls: image_candidates(hover) });
    }

    if cards.is_empty() {
        bail!("No cards found on that page. Is it a Limitless TCG set or search URL?");
    }
    Ok(CardSet { code, title, cards })
}

/// The list view links tiny `_XS` thumbnails; prefer the large scan.
fn image_candidates(url: &str) -> Vec<String> {
    if url.contains("_XS.") {
        ["_LG.", "_SM.", "_XS."].iter().map(|s| url.replace("_XS.", s)).collect()
    } else {
        vec![url.to_string()]
    }
}

pub fn download_image(client: &Client, card: &Card) -> Result<DynamicImage> {
    let mut last_err = anyhow!("no image URL");
    for url in &card.image_urls {
        match client.get(url).send().and_then(|r| r.error_for_status()) {
            Ok(resp) => {
                let bytes = resp.bytes()?;
                return image::load_from_memory(&bytes).context("could not decode image");
            }
            Err(e) => last_err = e.into(),
        }
    }
    Err(last_err)
}
