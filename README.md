# Pokemon Card Inserts Generator

This project automates the process of scraping Pokémon TCG card images, generating custom insert images, and laying them out for print as a PDF. It's CURRENTLY designed for the "Destined Rivals" set.

## Features

- **Scrape Card Data:** Downloads card numbers and image URLs from LimitlessTCG.
- **Generate Inserts:** Creates stylized insert images with blurred backgrounds and large numbers.
- **PDF Layout:** Arranges inserts in a print-ready PDF with minimal gaps for easy cutting.

## Project Structure

- [`scrape_cards.py`](scrape_cards.py): Scrapes card data and saves it to `destined_rivals_cards.csv`.
- [`create_inserts.py`](create_inserts.py): Generates insert images in the `inserts/` folder.
- [`create_pdf.py`](create_pdf.py): Lays out inserts into `destined_rivals_inserts.pdf`.
- [`destined_rivals_cards.csv`](destined_rivals_cards.csv): Card numbers and image URLs (auto-generated).
- [`pokemon_solid.ttf`](pokemon_solid.ttf): Font for insert numbers (add this file yourself).
- [`inserts/`](inserts/): Folder for generated insert images.
- `.gitignore`: Prevents large/generated files from being committed.

## Requirements

- Python 3.x
- [Pillow](https://python-pillow.org/)
- [pandas](https://pandas.pydata.org/)
- [requests](https://docs.python-requests.org/)
- [beautifulsoup4](https://www.crummy.com/software/BeautifulSoup/)
- [reportlab](https://www.reportlab.com/)

Install dependencies with:

```sh
pip install pillow pandas requests beautifulsoup4 reportlab
```

## Usage

1. **Scrape Card Data**

   ```sh
   python scrape_cards.py
   ```

2. **Generate Insert Images**

   ```sh
   python create_inserts.py
   ```

   > Make sure `pokemon_solid.ttf` is present in the project directory for best results.

3. **Create PDF Layout**

   ```sh
   python create_pdf.py
   ```

   The final PDF will be saved as `destined_rivals_inserts.pdf`.

## Notes

- The `inserts/` folder and `destined_rivals_cards.csv` are auto-generated and ignored by git.
- The PDF is formatted for US Letter paper with 9 cards per page and small gaps for cutting.

## License

This project is for personal use and not affiliated with or endorsed by Pokémon or LimitlessTCG.