import requests
from bs4 import BeautifulSoup
import pandas as pd

# URL of the set
url = "https://limitlesstcg.com/cards/WHT"

# Fetch the page
response = requests.get(url)
soup = BeautifulSoup(response.text, "lxml")

# Find the specific container that holds all the cards
grid = soup.find("div", class_="card-search-grid")

# Find all <a> tags (each one wraps an image)
cards = grid.find_all("a", href=True)

data = []
for idx, card in enumerate(cards, start=1):
    img_tag = card.find("img")
    if img_tag:
        # Use the index as the number (since they're in order in the grid)
        number = f"{idx:03d}"

        img_url = img_tag["src"]
        data.append({"Number": number, "ImageURL": img_url})

# Convert to DataFrame and save
df = pd.DataFrame(data)
df.to_csv("destined_rivals_cards.csv", index=False, encoding="utf-8")
print("✅ Data saved to destined_rivals_cards.csv")
