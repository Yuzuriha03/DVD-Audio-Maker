from pathlib import Path
from PIL import Image

source = Path("assets/dvd-audio-maker.png")
root = Path("apps/android/android/app/src/main/res")
sizes = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}
with Image.open(source) as image:
    image = image.convert("RGBA")
    for density, size in sizes.items():
        target = root / f"mipmap-{density}" / "ic_launcher.png"
        target.parent.mkdir(parents=True, exist_ok=True)
        image.resize((size, size), Image.Resampling.LANCZOS).save(target)
        print(f"{target} {size}x{size}")
