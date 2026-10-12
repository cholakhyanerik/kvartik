"""Значки программы: вкладка браузера, установка, экран «Домой» (задача 8).

Пишет PNG в crates/app/public/icons/. Цвета не придуманы, а взяты из
crates/app/style/tokens.css, светлая тема: фон — --accent, знак —
--on-accent (их контраст там записан и проверен). Тест
crates/app/tests/pwa.rs сверяет палитру каждого значка с tokens.css:
поменяли цвет и не перезапустили скрипт — тест красный.

Знак — круговая диаграмма с отодвинутой четвертью: квартал, ради которого
программа и нужна, — и от рабочего имени (задача 16) он не зависит.
Рисуется геометрией, без шрифтов и без сторонних библиотек: только
стандартная библиотека Python.

PNG — с палитрой (тип 3), а не RGBA: палитра лежит в файле несжатой,
и тест читает цвета без распаковки. Края сглажены выборкой 4×4 на точку:
цветов выходит три десятка, а не миллионы, и файлы меньше.

Запуск из корня репозитория:

    python -I -X utf8 tools/icons.py
"""

import math
import re
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOKENS = ROOT / "crates" / "app" / "style" / "tokens.css"
OUT = ROOT / "crates" / "app" / "public" / "icons"

# Сглаживание: выборок на сторону точки.
SAMPLES = 4

# Значки: имя, сторона в точках, с подложкой во весь квадрат или со
# скруглёнными углами, радиус круга и сдвиг четверти — доли стороны.
#
# Во весь квадрат — «maskable» и для iPhone: форму значка там задаёт
# система, а прозрачные углы iPhone закрасил бы чёрным. Знак maskable
# обязан уместиться в круг радиусом 0,4 стороны от центра (W3C Web App
# Manifest, «icon masks»): у нас 0,30 + 0,05/√2 ≈ 0,335.
#
# Значок вкладки — 32 точки: на 16–32 точках щель между четвертью и кругом
# в 0,05 стороны слипается, поэтому у него круг и щель крупнее.
ICONS = [
    ("icon-32.png", 32, "rounded", 0.34, 0.08),
    ("icon-192.png", 192, "rounded", 0.30, 0.05),
    ("icon-512.png", 512, "rounded", 0.30, 0.05),
    ("icon-maskable-512.png", 512, "full", 0.30, 0.05),
    ("apple-touch-icon.png", 180, "full", 0.30, 0.05),
]

# Радиус скругления подложки — доля стороны.
CORNER = 0.22


def light_color(name):
    """Цвет переменной светлой темы tokens.css — (r, g, b)."""
    text = TOKENS.read_text(encoding="utf-8")
    light = text.split("@media", 1)[0]
    match = re.search(r"^\s*" + re.escape(name) + r":\s*#([0-9a-fA-F]{6});", light, re.M)
    if not match:
        sys.exit(f"{name} не найдена в светлой теме {TOKENS}")
    value = match.group(1)
    return tuple(int(value[i : i + 2], 16) for i in (0, 2, 4))


def in_rounded(x, y):
    """Точка (доли стороны) внутри квадрата со скруглёнными углами."""
    cx = min(max(x, CORNER), 1 - CORNER)
    cy = min(max(y, CORNER), 1 - CORNER)
    return (x - cx) ** 2 + (y - cy) ** 2 <= CORNER**2


def in_glyph(x, y, r, d):
    """Точка внутри знака: круг без правой верхней четверти и сама четверть,
    отодвинутая вправо-вверх на d. Ось y — вниз, как на экране. Центр круга
    сдвинут так, чтобы знак целиком стоял посередине."""
    cx, cy = 0.5 - d / 2, 0.5 + d / 2
    if (x - cx) ** 2 + (y - cy) ** 2 <= r * r and not (x > cx and y < cy):
        return True
    qx, qy = cx + d, cy - d
    return x >= qx and y <= qy and (x - qx) ** 2 + (y - qy) ** 2 <= r * r


def render(size, backing, r, d):
    """Значок: строки пикселей, пиксель — (доля знака, доля подложки) в
    шестнадцатых долях выборки."""
    n = SAMPLES
    rows = []
    for py in range(size):
        row = []
        for px in range(size):
            glyph = backed = 0
            for sy in range(n):
                y = (py + (sy + 0.5) / n) / size
                for sx in range(n):
                    x = (px + (sx + 0.5) / n) / size
                    if backing == "full" or in_rounded(x, y):
                        backed += 1
                        if in_glyph(x, y, r, d):
                            glyph += 1
            row.append((glyph, backed))
        rows.append(row)
    return rows


def chunk(kind, data):
    body = kind + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)


def write_png(path, rows, background, foreground):
    """PNG с палитрой: цвет пикселя — смесь подложки и знака по доле знака,
    прозрачность — по доле подложки."""
    full = SAMPLES * SAMPLES
    palette = {}
    indexed = []
    for row in rows:
        line = bytearray([0])  # фильтр строки: нет
        for glyph, backed in row:
            if backed == 0:
                key = (0, 0, 0, 0)
            else:
                t = glyph / backed
                rgb = tuple(round(b + (f - b) * t) for b, f in zip(background, foreground))
                key = (*rgb, round(255 * backed / full))
            if key not in palette:
                palette[key] = len(palette)
            line.append(palette[key])
        indexed.append(bytes(line))
    if len(palette) > 256:
        sys.exit(f"{path.name}: цветов {len(palette)}, в палитру PNG влезает 256")
    entries = sorted(palette, key=palette.get)
    plte = b"".join(bytes(entry[:3]) for entry in entries)
    trns = bytes(entry[3] for entry in entries)
    size = len(rows)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 3, 0, 0, 0))
    png += chunk(b"PLTE", plte)
    png += chunk(b"tRNS", trns)
    png += chunk(b"IDAT", zlib.compress(b"".join(indexed), 9))
    png += chunk(b"IEND", b"")
    path.write_bytes(png)
    return len(png), len(palette)


def main():
    background = light_color("--accent")
    foreground = light_color("--on-accent")
    OUT.mkdir(parents=True, exist_ok=True)
    for name, size, backing, r, d in ICONS:
        bytes_, colors = write_png(OUT / name, render(size, backing, r, d), background, foreground)
        print(f"{name}: {size}×{size}, {colors} цветов, {bytes_} Б")


if __name__ == "__main__":
    main()
