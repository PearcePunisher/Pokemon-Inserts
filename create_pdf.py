from reportlab.pdfgen import canvas
from reportlab.lib.pagesizes import letter
from reportlab.lib.units import inch
import os

# Card size in points
card_width = 2.5 * inch
card_height = 3.5 * inch

# Page setup
page_width, page_height = letter
margin_x = 0.25 * inch
margin_y = 0.25 * inch

# Output PDF
output_pdf = "destined_rivals_inserts.pdf"
c = canvas.Canvas(output_pdf, pagesize=letter)

# Load insert images
insert_folder = "inserts"
insert_files = [f for f in os.listdir(insert_folder) if f.endswith(".png")]

# Sort by numeric value of filename (e.g., 1.png, 2.png, 10.png)
insert_files.sort(key=lambda x: int(os.path.splitext(x)[0]))

# Grid layout: 3 columns x 3 rows
cols, rows = 3, 3
gap = 0.05 * inch  # ~1.27mm gap for Cricut cutting
x_spacing = card_width + gap
y_spacing = card_height + gap

card_count = 0
for insert_file in insert_files:
    col = card_count % cols
    row = (card_count // cols) % rows

    x = margin_x + col * x_spacing
    y = page_height - margin_y - (row + 1) * y_spacing + gap

    # Draw the image
    c.drawImage(os.path.join(insert_folder, insert_file), x, y, width=card_width, height=card_height, mask='auto')

    card_count += 1

    # Start a new page after 9 cards
    if card_count % (cols * rows) == 0:
        c.showPage()

# Save the final PDF
c.save()
print("🎉 PDF layout with minimal gaps created successfully:", output_pdf)
