"""Draws pinball/examples/trace.rs's output over the table's art: what the ball bounces off (in
cyan) and each plunge's path, coloured from weak (blue) to strong (red), with where it ended up.

    python3 tools/pinball-art/trace.py trace.json [out.png]
"""
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ART = Path(__file__).resolve().parents[2] / "assets/pinball/saloon/background.png"
SCALE = 2  # The art is exported at twice the 1920 by 1080 screen.

trace = json.load(open(sys.argv[1]))
out = sys.argv[2] if len(sys.argv) > 2 else "trace.png"
art = Image.open(ART).convert("RGBA")
art = Image.blend(art, Image.new("RGBA", art.size, (0, 0, 0, 255)), 0.45)
draw = ImageDraw.Draw(art)
for o in trace["outlines"]:
    a, b, r = [v * SCALE for v in o["a"]], [v * SCALE for v in o["b"]], o["r"] * SCALE
    if a == b:
        draw.ellipse([a[0] - r, a[1] - r, a[0] + r, a[1] + r], outline=(0, 255, 255), width=2)
    else:
        draw.line([*a, *b], fill=(0, 255, 255), width=max(2, int(r * 2)))
paths = trace["paths"]
for i, path in enumerate(paths):
    t = i / max(1, len(paths) - 1)
    colour = (int(255 * t), 80, int(255 * (1 - t)))
    points = [(x * SCALE, y * SCALE) for x, y in path["points"]]
    if len(points) > 1:
        draw.line(points, fill=colour, width=4)
    if points:
        x, y = points[-1]
        draw.ellipse([x - 10, y - 10, x + 10, y + 10], fill=colour)
        draw.text((x + 12, y - 8), f"{path['pull']:.1f}", fill=colour)
    print(f"pull {path['pull']:.2f}: ends {path['end']}  {' | '.join(path['events'][:12])}")
art.crop((0, 0, 1100 * SCALE, 1080 * SCALE)).resize((1100, 1080)).save(out)
print("wrote", out)
