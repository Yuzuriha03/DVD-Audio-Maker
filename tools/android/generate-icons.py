"""Generate the Android launcher and notification icons from the master PNG.

The repository keeps one icon source of truth: assets/dvd-audio-maker.png.
Both the adaptive-free launcher mipmaps and the notification small icon are
derived from it here, so no second hand-drawn icon set can drift.
"""

from pathlib import Path

from PIL import Image

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "assets" / "dvd-audio-maker.png"
RES = REPO / "apps" / "android" / "android" / "app" / "src" / "main" / "res"

LAUNCHER_SIZES = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}
# 24 dp notification icon per density bucket.
NOTIFICATION_SIZES = {"mdpi": 24, "hdpi": 36, "xhdpi": 48, "xxhdpi": 72, "xxxhdpi": 96}
# The master also carries the "DVD-Audio MAKER" wordmarks below the disc. They
# are unreadable at 24 dp, so the glyph is cropped to the disc plus waveform.
NOTIFICATION_TOP_FRACTION = 0.68
NOTIFICATION_BRIGHT_THRESHOLD = 150
NOTIFICATION_ALPHA_THRESHOLD = 128
NOTIFICATION_MARGIN_FRACTION = 0.10


def notification_glyph(image: Image.Image) -> Image.Image:
    """Return a square white-on-transparent notification glyph."""
    luminance = image.convert("L")
    alpha = image.getchannel("A")
    bright = luminance.point(
        lambda value: 255 if value >= NOTIFICATION_BRIGHT_THRESHOLD else 0
    )
    opaque = alpha.point(
        lambda value: 255 if value > NOTIFICATION_ALPHA_THRESHOLD else 0
    )
    mask = Image.composite(bright, Image.new("L", image.size, 0), opaque)
    width, height = image.size
    top = mask.crop((0, 0, width, int(height * NOTIFICATION_TOP_FRACTION)))
    bounds = top.getbbox()
    if bounds is None:
        raise SystemExit(f"No bright pixels found in {SOURCE}")
    glyph = top.crop(bounds)
    side = max(glyph.size)
    padding = int(side * NOTIFICATION_MARGIN_FRACTION)
    canvas = Image.new("L", (side + 2 * padding, side + 2 * padding), 0)
    canvas.paste(glyph, (padding, padding))
    white = Image.new("RGBA", canvas.size, (255, 255, 255, 0))
    white.putalpha(canvas)
    return white


def main() -> None:
    if not SOURCE.is_file():
        raise SystemExit(f"Master icon is missing: {SOURCE}")
    with Image.open(SOURCE) as source:
        image = source.convert("RGBA")
        for density, size in LAUNCHER_SIZES.items():
            target = RES / f"mipmap-{density}" / "ic_launcher.png"
            target.parent.mkdir(parents=True, exist_ok=True)
            image.resize((size, size), Image.Resampling.LANCZOS).save(target)
            print(f"{target} {size}x{size}")
        glyph = notification_glyph(image)
        for density, size in NOTIFICATION_SIZES.items():
            target = RES / f"drawable-{density}" / "ic_stat_dvda.png"
            target.parent.mkdir(parents=True, exist_ok=True)
            glyph.resize((size, size), Image.Resampling.LANCZOS).save(target)
            print(f"{target} {size}x{size}")


if __name__ == "__main__":
    main()
