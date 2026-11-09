# Pokémon Inserts (desktop app)

A native desktop version of the inserts generator, written in Rust. Paste a
Limitless TCG set URL, adjust the options with a live preview, and click
**Generate PDF**. It's a single ~9 MB app with no Python required.

## Options

- **Full art** shows the card art blurred, with the number on top. The
  **Background blur** slider sets how blurry it is (0 = sharp).
- **Printer-friendly** is a white insert with a rounded outline, the number,
  the card name and the set code. It uses very little ink.
- **Show card name** adds the name under the number.
- **Double-sided (binder pages)** halves the paper. Each printed sheet
  becomes one 9-pocket binder page: cards 1–9 on the front, 10–18 on the
  back, and so on. Print with **double-sided, flip on long edge** at
  **100% / actual size**. Cut each insert out and put it in the same pocket
  position it had on the sheet. The card on its back then lines up with that
  pocket on the back of the binder page.
- **Also save each insert as a PNG** writes the individual images next to the PDF.

Output goes to `Documents/Pokemon Inserts/<SET>/` by default. Use **Change…** to
pick another folder.

## Build & run

Requires Rust (https://rustup.rs).

```bash
cd desktop
cargo run --release                                          # start the app
cargo run --release -- https://limitlesstcg.com/cards/DLR    # start with a set loaded
```

### Package

- **macOS:** `./bundle-macos.sh` creates `dist/Pokemon Inserts.app` and a
  `.dmg`. Add `--universal` for one app that runs on both Apple Silicon and
  Intel Macs. The app isn't notarized, so the first time, right-click it and
  choose **Open**.
- **Windows:** `cargo build --release` produces `target\release\pokemon-inserts.exe`.

### Releasing

Bump `version` in `Cargo.toml`, commit, then tag and push:

```bash
git tag v0.2.0 && git push origin v0.2.0
```

The **Release** GitHub Action builds the universal `.dmg` and the Windows
`.exe`, then publishes them on the repo's Releases page.

## Tests

```bash
cargo test                                                       # layout tests
INSERTS_OUT=/tmp/inserts cargo test --release -- --ignored       # full DLR run (needs internet)
```

## Notes

- Card numbers, names and images come from the set's list view
  (`?display=list`). Large (`_LG`) scans are used for sharper art.
- Card names use DejaVu Sans Bold (see `assets/DejaVu-LICENSE.txt`), which
  covers characters like é and ♀.
