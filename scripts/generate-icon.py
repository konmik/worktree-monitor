from pathlib import Path

from PIL import Image, ImageDraw


destination = Path(__file__).resolve().parents[1] / "crates" / "wtm" / "assets"
destination.mkdir(parents=True, exist_ok=True)

background = "#0b1012"
border = "#28343a"
teal = "#54d5c5"
lime = "#c2f970"
branch = [(80, 164), (128, 164)]
for step in range(1, 17):
    t = step / 16
    branch.append(((1 - t) ** 2 * 128 + 2 * (1 - t) * t * 176 + t ** 2 * 176,
                   (1 - t) ** 2 * 164 + 2 * (1 - t) * t * 164 + t ** 2 * 116))
branch.append((176, 60))
paths = [[(80, 196), (80, 60)], branch]
nodes = [(80, 60, teal), (80, 196, teal), (176, 60, lime)]

svg = [
    '<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">',
    f'<rect x="8" y="8" width="240" height="240" rx="48" fill="{border}"/>',
    f'<rect x="16" y="16" width="224" height="224" rx="40" fill="{background}"/>',
]
for path in paths:
    points = " ".join(f"{x:.2f},{y:.2f}" for x, y in path)
    svg.append(f'<polyline points="{points}" fill="none" stroke="{teal}" stroke-width="20" stroke-linecap="round" stroke-linejoin="round"/>')
for x, y, color in nodes:
    svg.append(f'<circle cx="{x}" cy="{y}" r="18" fill="{color}"/>')
svg.append('</svg>')
(destination / "wtm.svg").write_text("\n".join(svg) + "\n", encoding="utf-8")

scale = 4
image = Image.new("RGBA", (256 * scale, 256 * scale))
draw = ImageDraw.Draw(image)
draw.rounded_rectangle((8 * scale, 8 * scale, 248 * scale, 248 * scale), 48 * scale, fill=border)
draw.rounded_rectangle((16 * scale, 16 * scale, 240 * scale, 240 * scale), 40 * scale, fill=background)
for path in paths:
    draw.line([(round(x * scale), round(y * scale)) for x, y in path], fill=teal, width=20 * scale, joint="curve")
    for x, y in (path[0], path[-1]):
        draw.ellipse(((x - 10) * scale, (y - 10) * scale, (x + 10) * scale, (y + 10) * scale), fill=teal)
for x, y, color in nodes:
    draw.ellipse(((x - 18) * scale, (y - 18) * scale, (x + 18) * scale, (y + 18) * scale), fill=color)
image.resize((256, 256), Image.Resampling.LANCZOS).save(destination / "wtm.png")
sizes = [(size, size) for size in (16, 20, 24, 32, 40, 48, 64, 128, 256)]
image.save(destination / "wtm.ico", format="ICO", sizes=sizes)
