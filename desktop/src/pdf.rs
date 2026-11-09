//! Lays inserts out 3x3 on US Letter pages.
//!
//! Double-sided mode targets two-sided 9-pocket binder pages: each printed
//! sheet becomes one binder page, front = cards 1-9, back = cards 10-18.
//! Print duplex with "flip on long edge". Flipping the paper and flipping the
//! binder page both mirror left/right, so the two mirrors cancel and the back
//! page uses the same natural left-to-right order as the front.

use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};

const PAGE_W: f32 = 612.0; // 8.5in
const PAGE_H: f32 = 792.0; // 11in
const CARD_W: f32 = 180.0; // 2.5in
const CARD_H: f32 = 252.0; // 3.5in
const GAP: f32 = 2.88; // 0.04in
const COLS: usize = 3;
const ROWS: usize = 3;
const PER_PAGE: usize = COLS * ROWS;

/// For each page, the card index (if any) in each of the 9 cells.
pub fn layout(card_count: usize, double_sided: bool) -> Vec<[Option<usize>; PER_PAGE]> {
    let page = |start: usize| {
        let mut cells = [None; PER_PAGE];
        for (i, cell) in cells.iter_mut().enumerate() {
            if start + i < card_count {
                *cell = Some(start + i);
            }
        }
        cells
    };
    let mut pages = Vec::new();
    if double_sided {
        let sheets = card_count.div_ceil(PER_PAGE * 2);
        for s in 0..sheets {
            pages.push(page(s * PER_PAGE * 2));
            pages.push(page(s * PER_PAGE * 2 + PER_PAGE));
        }
    } else {
        for p in 0..card_count.div_ceil(PER_PAGE) {
            pages.push(page(p * PER_PAGE));
        }
    }
    pages
}

/// Builds the PDF. `jpegs[i]` is the encoded insert for card `i` (or `None`
/// if it failed; its cell is left blank so binder positions stay correct).
pub fn build(jpegs: &[Option<Vec<u8>>], img_w: u32, img_h: u32, double_sided: bool) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let mut alloc = Ref::new(1);
    let catalog_id = alloc.bump();
    let tree_id = alloc.bump();

    let image_ids: Vec<Option<Ref>> = jpegs
        .iter()
        .map(|j| {
            j.as_ref().map(|data| {
                let id = alloc.bump();
                let mut x = pdf.image_xobject(id, data);
                x.filter(Filter::DctDecode);
                x.width(img_w as i32);
                x.height(img_h as i32);
                x.color_space().device_rgb();
                x.bits_per_component(8);
                id
            })
        })
        .collect();

    // Center the grid so front and back line up when printed duplex.
    let grid_w = COLS as f32 * CARD_W + (COLS - 1) as f32 * GAP;
    let grid_h = ROWS as f32 * CARD_H + (ROWS - 1) as f32 * GAP;
    let left = (PAGE_W - grid_w) / 2.0;
    let top = (PAGE_H - grid_h) / 2.0;

    let mut page_ids = Vec::new();
    for cells in layout(jpegs.len(), double_sided) {
        let page_id = alloc.bump();
        let content_id = alloc.bump();
        page_ids.push(page_id);

        let mut content = Content::new();
        let mut names = Vec::new();
        for (cell, card) in cells.iter().enumerate() {
            let Some(id) = card.and_then(|c| image_ids[c]) else { continue };
            let (r, c) = (cell / COLS, cell % COLS);
            let x = left + c as f32 * (CARD_W + GAP);
            let y = PAGE_H - (top + r as f32 * (CARD_H + GAP)) - CARD_H;
            let name = format!("Im{cell}");
            content.save_state();
            content.transform([CARD_W, 0.0, 0.0, CARD_H, x, y]);
            content.x_object(Name(name.as_bytes()));
            content.restore_state();
            names.push((name, id));
        }
        pdf.stream(content_id, &content.finish());

        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, PAGE_W, PAGE_H));
        page.parent(tree_id);
        page.contents(content_id);
        let mut resources = page.resources();
        let mut xobjects = resources.x_objects();
        for (name, id) in &names {
            xobjects.pair(Name(name.as_bytes()), *id);
        }
        xobjects.finish();
        resources.finish();
        page.finish();
    }

    pdf.pages(tree_id).kids(page_ids.iter().copied()).count(page_ids.len() as i32);
    pdf.catalog(catalog_id).pages(tree_id);
    pdf.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_sided_fills_pages_in_order() {
        let pages = layout(10, false);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0][0], Some(0));
        assert_eq!(pages[1][0], Some(9));
        assert_eq!(pages[1][1], None);
    }

    #[test]
    fn double_sided_pairs_match_binder_pockets() {
        let pages = layout(141, true);
        assert_eq!(pages.len(), 16); // 8 sheets
        let (front, back) = (pages[0], pages[1]);
        for r in 0..ROWS {
            for c in 0..COLS {
                // Duplex long-edge flip: paper cell (r, c) is backed by back-page cell (r, 2-c).
                let paper_back = back[r * COLS + (COLS - 1 - c)];
                // The pocket at front (r, c) is seen at (r, 2-c) from the back of the binder page,
                // which is binder slot 9 + r*3 + (2-c).
                assert_eq!(front[r * COLS + c], Some(r * COLS + c));
                assert_eq!(paper_back, Some(PER_PAGE + r * COLS + (COLS - 1 - c)));
            }
        }
    }
}
