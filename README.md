# Pokemon Inserts Generator

A simple cross-platform desktop app to:
- Scrape card images from a Limitless TCG set URL
- Generate numbered insert images
- Layout inserts into a printable PDF (3x3 per page, US Letter)

## Quick start for non‑technical users (macOS & Windows)

Assumption: You already have Python installed.

1) Get the project folder
- Download this project as a ZIP and unzip it, or copy the whole folder to your computer.

2) Open a terminal
- macOS: Applications > Utilities > Terminal
- Windows: Open Command Prompt (or PowerShell)

3) Go to the project folder
- Tip: You can drag the folder into the terminal window after typing `cd `

```bash
cd path/to/Pokemon-Inserts
```

4) Create a private Python environment (recommended)
- This keeps the app’s dependencies separate from your system

macOS/Linux:
```bash
python -m venv .venv
source .venv/bin/activate
```

Windows (Command Prompt):
```cmd
python -m venv .venv
.venv\Scripts\activate
```

Windows (PowerShell):
```powershell
python -m venv .venv
.venv\Scripts\Activate.ps1
```

5) Install the app’s requirements
```bash
pip install -r requirements.txt
```
- If `pip` isn’t found, try: `python -m pip install -r requirements.txt`

6) Run the app
```bash
python app.py
```
- A small window will open. Paste a Limitless TCG cards URL (for example: https://limitlesstcg.com/cards/WHT), then click Generate.
- When it finishes, click “Open Output Folder” to see your files.

Where are the outputs?
- The app saves everything under `output/<set>/` inside this folder.
- You’ll find all generated PNG inserts in `output/<set>/inserts/` and the final PDF named `<set>_inserts.pdf`.

Troubleshooting
- If the window does not appear, your Python install may not include Tkinter. Install a standard Python from python.org (macOS/Windows) and try again.
- If activation fails on Windows PowerShell due to policy, either use Command Prompt or run PowerShell as Administrator and temporarily enable script execution.

---

## How to run (developer)

1) Create/activate a virtual environment and install deps:

```bash
python -m venv .venv
source .venv/bin/activate  # Windows: .venv\Scripts\activate
pip install -r requirements.txt
```

2) Start the app:

```bash
python app.py
```

Paste the Limitless TCG set URL (e.g., https://limitlesstcg.com/cards/WHT), click Generate, and wait. When done, use the "Open Output Folder" button.

Outputs will be under `output/<set>/`, including `inserts/` PNGs and `<set>_inserts.pdf`.

## Packaging into a standalone app (optional)

You can create a single-file executable using PyInstaller.

```bash
pip install pyinstaller
pyinstaller --onefile --windowed \
  --add-data "pokemon_solid.ttf:pokemon_solid.ttf" \
  app.py
```

- macOS binary will be at `dist/app`
- Windows binary will be at `dist\app.exe`

Note: If fonts or images are missing in the bundled app, ensure `--add-data` paths are correct. On Windows, use `;` as the data path separator instead of `:`.

## Notes
- The scraper relies on the current Structure of Limitless TCG. If it changes, selectors may need adjustment.
- The app sets a desktop-like User-Agent and uses timeouts.
- PDF layout targets US Letter. For A4, adjust page size.
