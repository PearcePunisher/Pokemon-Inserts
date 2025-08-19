import os
import sys
import threading
import queue
import re
import webbrowser
from urllib.parse import urlparse

import requests
from bs4 import BeautifulSoup
from io import BytesIO
from PIL import Image, ImageDraw, ImageFont, ImageFilter
from reportlab.pdfgen import canvas
from reportlab.lib.pagesizes import letter
from reportlab.lib.units import inch
from typing import Optional

# Try to import Tkinter; fall back to CLI mode if unavailable
try:
    import tkinter as tk
    from tkinter import ttk, messagebox, filedialog
    HAS_TK = True
except Exception:
    HAS_TK = False


# ---------- Utility helpers ----------

def resource_path(relative_path: str) -> str:
    """Get absolute path to resource, works for dev and for PyInstaller bundle."""
    base_path = getattr(sys, '_MEIPASS', None) or os.path.abspath(os.path.dirname(__file__))
    return os.path.join(base_path, relative_path)


def open_in_file_manager(path: str):
    path = os.path.abspath(path)
    if sys.platform.startswith('darwin'):
        os.system(f'open "{path}"')
    elif os.name == 'nt':
        os.startfile(path)  # type: ignore[attr-defined]
    else:
        webbrowser.open(f'file://{path}')


# ---------- Core pipeline functions ----------

USER_AGENT = (
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
    "AppleWebKit/537.36 (KHTML, like Gecko) "
    "Chrome/125.0.0.0 Safari/537.36"
)


def log_safe(q: Optional[queue.Queue], msg: str):
    if q is None:
        # CLI mode: print to stdout
        print(msg)
        return
    try:
        q.put_nowait(msg)
    except Exception:
        # If queue is full or closed, ignore
        pass


def derive_set_name_from_url(url: str) -> str:
    # Try to derive a human-ish name from the last URL segment
    parsed = urlparse(url)
    parts = [p for p in parsed.path.split('/') if p]
    if parts:
        return parts[-1]
    return 'set'


def scrape_cards(url: str, q: queue.Queue) -> list[dict]:
    log_safe(q, f"Fetching cards from: {url}")
    headers = {"User-Agent": USER_AGENT}
    resp = requests.get(url, headers=headers, timeout=30)
    resp.raise_for_status()

    soup = BeautifulSoup(resp.text, 'lxml')

    grid = soup.find('div', class_='card-search-grid')
    if not grid:
        # Fallback: try alternative known containers if site structure changes
        grid = soup.find('div', class_='search-grid') or soup

    cards = grid.find_all('a', href=True)

    data = []
    for idx, card in enumerate(cards, start=1):
        img_tag = card.find('img')
        if not img_tag:
            continue
        img_url = (
            img_tag.get('src')
            or img_tag.get('data-src')
            or img_tag.get('data-original')
        )
        if not img_url:
            continue
        number = f"{idx:03d}"
        data.append({"Number": number, "ImageURL": img_url})

    if not data:
        raise RuntimeError("No cards/images found on the provided page.")

    log_safe(q, f"Found {len(data)} cards.")
    return data


def load_font(size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    try:
        font_path = resource_path('pokemon_solid.ttf')
        if os.path.exists(font_path):
            return ImageFont.truetype(font_path, size)
    except Exception:
        pass
    return ImageFont.load_default()


def create_inserts(cards: list[dict], inserts_dir: str, q: queue.Queue):
    os.makedirs(inserts_dir, exist_ok=True)
    card_width, card_height = 750, 1050  # 2.5" x 3.5" at 300 DPI
    font = load_font(144)

    session = requests.Session()
    session.headers.update({"User-Agent": USER_AGENT})

    for i, row in enumerate(cards, start=1):
        number = str(row.get('Number', f"{i:03d}"))
        img_url = row.get('ImageURL')
        if not img_url:
            log_safe(q, f"Skipping {number}: missing image URL")
            continue

        log_safe(q, f"Downloading image {i}/{len(cards)}: {img_url}")
        r = session.get(img_url, timeout=30)
        r.raise_for_status()

        img_data = BytesIO(r.content)
        try:
            original_img = Image.open(img_data).convert('RGB').resize((card_width, card_height))
        except Exception as e:
            log_safe(q, f"Skipping {number}: cannot open image ({e})")
            continue

        blurred_bg = original_img.filter(ImageFilter.GaussianBlur(20))

        insert = Image.new('RGB', (card_width, card_height), 'white')
        insert.paste(blurred_bg, (0, 0))

        draw = ImageDraw.Draw(insert)
        text = f"{number}"

        bbox = draw.textbbox((0, 0), text, font=font)
        text_width = bbox[2] - bbox[0]
        text_height = bbox[3] - bbox[1]
        text_x = (card_width - text_width) / 2
        text_y = (card_height - text_height) / 2

        # White text with black stroke for contrast
        draw.text(
            (text_x, text_y),
            text,
            font=font,
            fill='white',
            stroke_width=2,
            stroke_fill='black',
        )

        # Rounded corners mask
        corner_radius = 40
        mask = Image.new('L', (card_width, card_height), 0)
        mask_draw = ImageDraw.Draw(mask)
        mask_draw.rounded_rectangle([0, 0, card_width, card_height], radius=corner_radius, fill=255)

        insert = insert.convert('RGBA')
        insert.putalpha(mask)

        out_path = os.path.join(inserts_dir, f"{number}.png")
        insert.save(out_path, dpi=(300, 300), quality=95)

    log_safe(q, f"Inserts saved to: {inserts_dir}")


def create_pdf_from_inserts(inserts_dir: str, output_pdf: str, q: queue.Queue):
    files = [f for f in os.listdir(inserts_dir) if f.lower().endswith('.png')]
    if not files:
        raise RuntimeError("No insert images found to place into the PDF.")

    # Sort numerically if filenames are numbers like 001.png
    try:
        files.sort(key=lambda x: int(os.path.splitext(x)[0]))
    except ValueError:
        files.sort()  # fallback to lexicographic

    # Page and card sizes
    page_width, page_height = letter
    card_width = 2.5 * inch
    card_height = 3.5 * inch
    margin_x = 0.25 * inch
    margin_y = 0.25 * inch
    cols, rows = 3, 3
    gap = 0.05 * inch
    x_spacing = card_width + gap
    y_spacing = card_height + gap

    c = canvas.Canvas(output_pdf, pagesize=letter)

    card_count = 0
    for insert_file in files:
        col = card_count % cols
        row = (card_count // cols) % rows

        x = margin_x + col * x_spacing
        y = page_height - margin_y - (row + 1) * y_spacing + gap

        c.drawImage(os.path.join(inserts_dir, insert_file), x, y, width=card_width, height=card_height, mask='auto')
        card_count += 1

        if card_count % (cols * rows) == 0:
            c.showPage()

    c.save()
    log_safe(q, f"PDF created: {output_pdf}")


# ---------- Tkinter UI ----------

if HAS_TK:
    class App(tk.Tk):
        def __init__(self):
            super().__init__()
            self.title("Pokemon Inserts Generator")
            self.geometry("720x480")
            self.minsize(680, 420)

            self.queue: queue.Queue[str] = queue.Queue()
            self.worker: threading.Thread | None = None
            self.result_pdf: str | None = None
            self.output_dir: str | None = None

            self.create_widgets()
            self.after(100, self.process_queue)

        def create_widgets(self):
            frm = ttk.Frame(self, padding=16)
            frm.pack(fill=tk.BOTH, expand=True)

            url_label = ttk.Label(frm, text="Limitless TCG cards URL:")
            url_label.grid(row=0, column=0, sticky='w')

            self.url_var = tk.StringVar()
            self.url_entry = ttk.Entry(frm, textvariable=self.url_var)
            self.url_entry.grid(row=1, column=0, columnspan=3, sticky='ew', pady=(4, 12))
            self.url_entry.focus_set()

            self.start_btn = ttk.Button(frm, text="Generate", command=self.on_start)
            self.start_btn.grid(row=2, column=0, sticky='w')

            self.open_btn = ttk.Button(frm, text="Open Output Folder", command=self.on_open_folder, state=tk.DISABLED)
            self.open_btn.grid(row=2, column=1, sticky='w', padx=(8, 0))

            self.progress = ttk.Progressbar(frm, mode='indeterminate')
            self.progress.grid(row=2, column=2, sticky='e')

            self.log = tk.Text(frm, height=18, wrap='word')
            self.log.grid(row=3, column=0, columnspan=3, sticky='nsew', pady=(12, 0))

            frm.columnconfigure(0, weight=1)
            frm.columnconfigure(1, weight=0)
            frm.columnconfigure(2, weight=0)
            frm.rowconfigure(3, weight=1)

            self.bind('<Return>', lambda e: self.on_start())

        def append_log(self, text: str):
            self.log.insert(tk.END, text + "\n")
            self.log.see(tk.END)

        def process_queue(self):
            try:
                while True:
                    msg = self.queue.get_nowait()
                    self.append_log(msg)
            except queue.Empty:
                pass
            self.after(100, self.process_queue)

        def on_start(self):
            if self.worker and self.worker.is_alive():
                return
            url = self.url_var.get().strip()
            if not url:
                messagebox.showerror("Missing URL", "Please paste a Limitless TCG cards URL.")
                return
            if not url.lower().startswith("http"):
                messagebox.showerror("Invalid URL", "Please enter a valid http(s) URL.")
                return

            self.result_pdf = None
            self.output_dir = None
            self.open_btn.config(state=tk.DISABLED)
            self.start_btn.config(state=tk.DISABLED)
            self.progress.start(10)
            self.log.delete('1.0', tk.END)

            self.worker = threading.Thread(target=self.run_pipeline, args=(url,), daemon=True)
            self.worker.start()

        def run_pipeline(self, url: str):
            try:
                cards = scrape_cards(url, self.queue)

                set_name = derive_set_name_from_url(url)
                base_output = os.path.join(os.path.abspath(os.getcwd()), 'output', set_name)
                inserts_dir = os.path.join(base_output, 'inserts')
                os.makedirs(inserts_dir, exist_ok=True)

                create_inserts(cards, inserts_dir, self.queue)

                output_pdf = os.path.join(base_output, f"{set_name}_inserts.pdf")
                create_pdf_from_inserts(inserts_dir, output_pdf, self.queue)

                self.result_pdf = output_pdf
                self.output_dir = base_output

                log_safe(self.queue, "All done!")
            except Exception as e:
                log_safe(self.queue, f"Error: {e}")
                self.result_pdf = None
            finally:
                # Switch back to UI thread to update controls
                self.after(0, self.on_pipeline_done)

        def on_pipeline_done(self):
            self.progress.stop()
            self.start_btn.config(state=tk.NORMAL)
            if self.result_pdf and os.path.exists(self.result_pdf):
                self.open_btn.config(state=tk.NORMAL)
                self.append_log(f"PDF saved at: {self.result_pdf}")
            else:
                self.open_btn.config(state=tk.DISABLED)

        def on_open_folder(self):
            if self.output_dir and os.path.isdir(self.output_dir):
                open_in_file_manager(self.output_dir)


def run_cli():
    print("Tkinter not available. Running in CLI mode.\n")
    url = input("Enter Limitless TCG cards URL: ").strip()
    if not url:
        print("No URL provided. Exiting.")
        return
    try:
        cards = scrape_cards(url, None)
        set_name = derive_set_name_from_url(url)
        base_output = os.path.join(os.path.abspath(os.getcwd()), 'output', set_name)
        inserts_dir = os.path.join(base_output, 'inserts')
        os.makedirs(inserts_dir, exist_ok=True)

        create_inserts(cards, inserts_dir, None)
        output_pdf = os.path.join(base_output, f"{set_name}_inserts.pdf")
        create_pdf_from_inserts(inserts_dir, output_pdf, None)

        print(f"\nAll done. PDF created at: {output_pdf}")
        try:
            open_in_file_manager(base_output)
        except Exception:
            pass
    except Exception as e:
        print(f"Error: {e}")


if __name__ == '__main__':
    if HAS_TK:
        App().mainloop()
    else:
        run_cli()
