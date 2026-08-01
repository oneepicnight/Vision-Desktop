from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont


ROOT = Path(__file__).resolve().parents[1]
ICON_DIR = ROOT / "src-tauri" / "icons"
SUPERSAMPLE = 4


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    candidates = (
        [Path("C:/Windows/Fonts/seguisb.ttf"), Path("C:/Windows/Fonts/arialbd.ttf")]
        if bold
        else [Path("C:/Windows/Fonts/segoeui.ttf"), Path("C:/Windows/Fonts/arial.ttf")]
    )
    for candidate in candidates:
        if candidate.exists():
            return ImageFont.truetype(str(candidate), size)
    return ImageFont.load_default()


def vertical_gradient(size: tuple[int, int], top: tuple[int, int, int], bottom: tuple[int, int, int]) -> Image.Image:
    width, height = size
    image = Image.new("RGB", size)
    pixels = image.load()
    for y in range(height):
        amount = y / max(height - 1, 1)
        color = tuple(round(start + (end - start) * amount) for start, end in zip(top, bottom))
        for x in range(width):
            pixels[x, y] = color
    return image


def diagonal_gradient(size: tuple[int, int], start: tuple[int, int, int], end: tuple[int, int, int]) -> Image.Image:
    width, height = size
    image = Image.new("RGBA", size)
    pixels = image.load()
    denominator = max(width + height - 2, 1)
    for y in range(height):
        for x in range(width):
            amount = (x + y) / denominator
            pixels[x, y] = tuple(round(a + (b - a) * amount) for a, b in zip(start, end)) + (255,)
    return image


def vision_mark(size: int) -> Image.Image:
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    tile_size = round(size * 0.69)
    padding = round(size * 0.14)
    tile_canvas_size = tile_size + padding * 2
    radius = round(tile_size * 0.27)
    tile_box = (padding, padding, padding + tile_size, padding + tile_size)

    glow = Image.new("RGBA", (tile_canvas_size, tile_canvas_size), (0, 0, 0, 0))
    glow_draw = ImageDraw.Draw(glow)
    glow_draw.rounded_rectangle(tile_box, radius=radius, fill=(65, 200, 255, 155))
    glow = glow.filter(ImageFilter.GaussianBlur(round(size * 0.055)))

    tile = Image.new("RGBA", (tile_canvas_size, tile_canvas_size), (0, 0, 0, 0))
    mask = Image.new("L", tile.size, 0)
    mask_draw = ImageDraw.Draw(mask)
    mask_draw.rounded_rectangle(tile_box, radius=radius, fill=255)
    gradient = diagonal_gradient(tile.size, (46, 205, 255), (105, 67, 231))
    tile.alpha_composite(Image.composite(gradient, Image.new("RGBA", tile.size), mask))

    tile_draw = ImageDraw.Draw(tile)
    border_width = max(2, round(size * 0.008))
    tile_draw.rounded_rectangle(
        tile_box,
        radius=radius,
        outline=(177, 236, 255, 220),
        width=border_width,
    )
    inset = round(tile_size * 0.08)
    tile_draw.rounded_rectangle(
        (tile_box[0] + inset, tile_box[1] + inset, tile_box[2] - inset, tile_box[3] - inset),
        radius=max(1, radius - inset),
        outline=(255, 255, 255, 38),
        width=max(1, border_width // 2),
    )

    x0, y0, x1, y1 = tile_box
    v_points = [
        (x0 + tile_size * 0.22, y0 + tile_size * 0.24),
        (x0 + tile_size * 0.39, y0 + tile_size * 0.24),
        (x0 + tile_size * 0.50, y0 + tile_size * 0.59),
        (x0 + tile_size * 0.61, y0 + tile_size * 0.24),
        (x0 + tile_size * 0.78, y0 + tile_size * 0.24),
        (x0 + tile_size * 0.60, y0 + tile_size * 0.75),
        (x0 + tile_size * 0.40, y0 + tile_size * 0.75),
    ]
    tile_draw.polygon(v_points, fill=(255, 255, 255, 255))

    highlight = [
        (x0 + tile_size * 0.25, y0 + tile_size * 0.27),
        (x0 + tile_size * 0.36, y0 + tile_size * 0.27),
        (x0 + tile_size * 0.50, y0 + tile_size * 0.67),
        (x0 + tile_size * 0.50, y0 + tile_size * 0.59),
        (x0 + tile_size * 0.39, y0 + tile_size * 0.24),
        (x0 + tile_size * 0.22, y0 + tile_size * 0.24),
    ]
    tile_draw.polygon(highlight, fill=(228, 250, 255, 255))

    combined = Image.alpha_composite(glow, tile)
    rotated = combined.rotate(-5, resample=Image.Resampling.BICUBIC, expand=True)
    canvas.alpha_composite(rotated, ((size - rotated.width) // 2, (size - rotated.height) // 2))
    return canvas


def draw_orbits(draw: ImageDraw.ImageDraw, width: int, height: int, accent: tuple[int, int, int]) -> None:
    center_x = round(width * 0.73)
    center_y = round(height * 0.72)
    for radius, alpha in ((34, 62), (56, 44), (80, 30), (108, 20)):
        draw.ellipse(
            (center_x - radius, center_y - radius, center_x + radius, center_y + radius),
            outline=accent + (alpha,),
            width=1,
        )
    for x, y, radius, color in (
        (round(width * 0.26), round(height * 0.66), 2, (84, 230, 212, 220)),
        (round(width * 0.82), round(height * 0.50), 2, (65, 200, 255, 220)),
        (round(width * 0.66), round(height * 0.86), 2, (151, 105, 255, 220)),
    ):
        draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=color)


def render_sidebar() -> Image.Image:
    width, height = 164 * SUPERSAMPLE, 314 * SUPERSAMPLE
    image = vertical_gradient((width, height), (4, 10, 26), (9, 19, 47)).convert("RGBA")
    draw = ImageDraw.Draw(image, "RGBA")
    draw_orbits(draw, width, height, (91, 126, 255))

    mark = vision_mark(92 * SUPERSAMPLE)
    image.alpha_composite(mark, ((width - mark.width) // 2, 22 * SUPERSAMPLE))

    title_font = font(22 * SUPERSAMPLE, bold=True)
    label_font = font(8 * SUPERSAMPLE, bold=True)
    title = "VISION"
    title_box = draw.textbbox((0, 0), title, font=title_font)
    draw.text(((width - (title_box[2] - title_box[0])) // 2, 123 * SUPERSAMPLE), title, font=title_font, fill=(244, 248, 255, 255))

    label = "NETWORK DESKTOP"
    label_box = draw.textbbox((0, 0), label, font=label_font)
    draw.text(((width - (label_box[2] - label_box[0])) // 2, 151 * SUPERSAMPLE), label, font=label_font, fill=(130, 162, 219, 255))

    line_y = 176 * SUPERSAMPLE
    draw.line((34 * SUPERSAMPLE, line_y, 130 * SUPERSAMPLE, line_y), fill=(88, 164, 255, 70), width=SUPERSAMPLE)
    draw.text((30 * SUPERSAMPLE, 279 * SUPERSAMPLE), "SECURE NODE OPERATIONS", font=font(7 * SUPERSAMPLE, bold=True), fill=(96, 222, 213, 205))
    return image.convert("RGB").resize((164, 314), Image.Resampling.LANCZOS)


def render_header(uninstall: bool = False) -> Image.Image:
    width, height = 150 * SUPERSAMPLE, 57 * SUPERSAMPLE
    bottom = (27, 13, 51) if uninstall else (14, 27, 68)
    image = vertical_gradient((width, height), (4, 10, 25), bottom).convert("RGBA")
    draw = ImageDraw.Draw(image, "RGBA")
    accent = (255, 117, 143) if uninstall else (84, 230, 212)

    for offset in (0, 15, 30):
        y = (13 + offset) * SUPERSAMPLE
        draw.arc(
            (66 * SUPERSAMPLE, y - 35 * SUPERSAMPLE, 164 * SUPERSAMPLE, y + 35 * SUPERSAMPLE),
            start=185,
            end=344,
            fill=accent + (58,),
            width=SUPERSAMPLE,
        )

    mark = vision_mark(45 * SUPERSAMPLE)
    image.alpha_composite(mark, (8 * SUPERSAMPLE, 6 * SUPERSAMPLE))
    draw.text((56 * SUPERSAMPLE, 16 * SUPERSAMPLE), "VISION", font=font(14 * SUPERSAMPLE, bold=True), fill=(244, 248, 255, 255))
    draw.text((57 * SUPERSAMPLE, 33 * SUPERSAMPLE), "DESKTOP", font=font(6 * SUPERSAMPLE, bold=True), fill=accent + (235,))
    return image.convert("RGB").resize((150, 57), Image.Resampling.LANCZOS)


def main() -> None:
    ICON_DIR.mkdir(parents=True, exist_ok=True)

    icon = vision_mark(1024)
    icon.save(ICON_DIR / "icon.png", optimize=True)
    icon.save(
        ICON_DIR / "icon.ico",
        format="ICO",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )

    render_sidebar().save(ICON_DIR / "nsis-sidebar.bmp", format="BMP")
    render_header().save(ICON_DIR / "nsis-header.bmp", format="BMP")
    render_header(uninstall=True).save(ICON_DIR / "nsis-uninstaller-header.bmp", format="BMP")


if __name__ == "__main__":
    main()
