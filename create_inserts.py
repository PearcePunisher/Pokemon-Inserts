import pandas as pd
import requests
from io import BytesIO
from PIL import Image, ImageDraw, ImageFont, ImageFilter
import os

# Read the CSV
df = pd.read_csv("destined_rivals_cards.csv")

# Output folder
output_folder = "inserts"
os.makedirs(output_folder, exist_ok=True)

# Insert size
card_width, card_height = 750, 1050  # 2.5" x 3.5" at 300 DPI

# Load the Pokémon-style font
try:
    font = ImageFont.truetype("pokemon_solid.ttf", 144)
except:
    print("⚠️ Could not find pokemon_solid.ttf. Using default font instead.")
    font = ImageFont.load_default()

for index, row in df.iterrows():
    number = row["Number"]
    img_url = row["ImageURL"]

    # Download the image
    response = requests.get(img_url)
    img_data = BytesIO(response.content)
    original_img = Image.open(img_data).convert("RGB").resize((card_width, card_height))

    # Blur the image
    blurred_bg = original_img.filter(ImageFilter.GaussianBlur(20))

    # Create the insert image
    insert = Image.new("RGB", (card_width, card_height), "white")
    insert.paste(blurred_bg, (0, 0))

    # Draw the large number in the center with a black border
    draw = ImageDraw.Draw(insert)
    text = f"{number}"

    bbox = draw.textbbox((0, 0), text, font=font)
    text_width = bbox[2] - bbox[0]
    text_height = bbox[3] - bbox[1]
    text_x = (card_width - text_width) / 2
    text_y = (card_height - text_height) / 2

    draw.text(
        (text_x, text_y),
        text,
        font=font,
        fill="white",
        stroke_width=2,
        stroke_fill="black"
    )


    # Using textbbox for centering
    bbox = draw.textbbox((0, 0), text, font=font)
    text_width = bbox[2] - bbox[0]
    text_height = bbox[3] - bbox[1]
    text_x = (card_width - text_width) / 2
    text_y = (card_height - text_height) / 2

    draw.text((text_x, text_y), text, font=font, fill="white")

    # Add rounded corners
    corner_radius = 40  # ~3-4mm at 300 DPI
    mask = Image.new("L", (card_width, card_height), 0)
    mask_draw = ImageDraw.Draw(mask)
    mask_draw.rounded_rectangle([0, 0, card_width, card_height], radius=corner_radius, fill=255)

    # Apply the mask to get rounded corners
    insert.putalpha(mask)

    # Save as PNG to keep transparency
    filename = f"{number}.png"
    insert.save(os.path.join(output_folder, filename), dpi=(300, 300), quality=95)

    print(f"✅ Created insert for card #{number}")

print("🎉 All inserts generated successfully!")
