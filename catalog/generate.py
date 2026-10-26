#!/usr/bin/env python3
"""Generate original Apache-2.0 furniture, separately from the application bundle.

Python stdlib only. GLB writing reuses the reviewed first-party example builder;
thumbnail decoding/rasterization uses the actual native Rust core. Coordinates
are glTF metres, +Y up; catalog dimensions are [width, depth, height] in mm.
No external furniture, photographs, textures or catalog data is used.
"""

import argparse
import ctypes as C
import hashlib
import importlib.util
import json
import pathlib
import shutil
import struct
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "original_boxes", ROOT / "examples/welcome-home/generate.py"
)
original = importlib.util.module_from_spec(spec)
spec.loader.exec_module(original)
box = original.box
WOOD, FABRIC, LIGHT, DARK = (
    [0.64, 0.44, 0.27],
    [0.35, 0.51, 0.52],
    [0.90, 0.88, 0.81],
    [0.21, 0.24, 0.26],
)

# Independently invented generic designs and dimensions, not branded products.
# kind, id, display name, category, width/depth/height mm, optional variation.
RECIPES = [
    ("chair", "dining-chair", "Dining chair", "Seating", 450, 500, 850),
    (
        "chair",
        "slat-back-chair",
        "Slat-back dining chair",
        "Seating",
        460,
        520,
        900,
        "slats",
    ),
    (
        "chair",
        "upholstered-chair",
        "Upholstered dining chair",
        "Seating",
        500,
        550,
        880,
        "padded",
    ),
    ("chair", "dining-armchair", "Dining armchair", "Seating", 580, 560, 900, "arms"),
    ("chair", "lounge-chair", "Lounge chair", "Seating", 780, 820, 850, "arms"),
    ("chair", "low-lounge-chair", "Low lounge chair", "Seating", 720, 800, 680, "arms"),
    ("stool", "kitchen-stool", "Kitchen stool", "Seating", 380, 380, 450),
    ("stool", "counter-stool", "Counter stool", "Seating", 400, 400, 650),
    ("chair", "bar-chair", "Bar chair", "Seating", 440, 480, 1050),
    ("bench", "two-seat-bench", "Two-seat bench", "Seating", 1100, 400, 450),
    ("bench", "three-seat-bench", "Three-seat bench", "Seating", 1600, 420, 450),
    (
        "bench",
        "entry-bench",
        "Upholstered entry bench",
        "Seating",
        1000,
        420,
        480,
        "padded",
    ),
    ("ottoman", "square-ottoman", "Square ottoman", "Seating", 600, 600, 420),
    ("ottoman", "wide-ottoman", "Wide ottoman", "Seating", 1000, 600, 420),
    ("ottoman", "floor-pouf", "Floor pouf", "Seating", 550, 550, 320),
    ("stool", "footstool", "Footstool", "Seating", 450, 350, 300, "padded"),
    ("chair", "desk-chair", "Desk chair", "Seating", 520, 550, 900, "padded"),
    ("chair", "task-armchair", "Task armchair", "Seating", 620, 600, 1000, "arms"),
    ("chair", "visitor-chair", "Visitor chair", "Seating", 520, 570, 800, "slats"),
    ("chair", "child-chair", "Child chair", "Seating", 320, 350, 600),
    ("sofa", "compact-loveseat", "Compact loveseat", "Sofas", 1400, 800, 800),
    ("sofa", "two-seat-sofa", "Two-seat sofa", "Sofas", 1700, 900, 850),
    ("sofa", "wide-two-seat-sofa", "Wide two-seat sofa", "Sofas", 1900, 950, 850),
    ("sofa", "three-seat-sofa", "Three-seat sofa", "Sofas", 2200, 900, 850),
    ("sofa", "four-seat-sofa", "Four-seat sofa", "Sofas", 2800, 950, 850),
    ("sofa", "high-back-sofa", "High-back loveseat", "Sofas", 1600, 850, 1050),
    ("sofa", "low-sofa", "Low sofa", "Sofas", 2100, 1000, 650),
    (
        "sofa",
        "left-sectional",
        "Left-chaise sectional",
        "Sofas",
        2600,
        1600,
        850,
        "left",
    ),
    (
        "sofa",
        "right-sectional",
        "Right-chaise sectional",
        "Sofas",
        2600,
        1600,
        850,
        "right",
    ),
    ("sofa", "chaise-lounge", "Chaise lounge", "Sofas", 800, 1650, 800),
    ("table", "small-dining-table", "Two-person dining table", "Tables", 800, 700, 750),
    (
        "table",
        "four-person-table",
        "Four-person dining table",
        "Tables",
        1200,
        800,
        750,
    ),
    ("table", "six-person-table", "Six-person dining table", "Tables", 1800, 900, 750),
    (
        "table",
        "eight-person-table",
        "Eight-person dining table",
        "Tables",
        2400,
        1000,
        750,
    ),
    ("table", "counter-table", "Counter-height table", "Tables", 1400, 700, 950),
    ("table", "coffee-table", "Coffee table", "Tables", 1100, 600, 420),
    ("table", "square-coffee-table", "Square coffee table", "Tables", 800, 800, 400),
    ("table", "low-coffee-table", "Low coffee table", "Tables", 1200, 650, 300),
    ("table", "side-table", "Side table", "Tables", 450, 450, 520),
    ("table", "tall-side-table", "Tall side table", "Tables", 400, 400, 650),
    ("table", "console-table", "Console table", "Tables", 1200, 350, 800),
    ("table", "narrow-console", "Narrow console table", "Tables", 900, 280, 750),
    ("table", "child-table", "Child table", "Tables", 700, 600, 500),
    ("table", "utility-table", "Utility table", "Tables", 1500, 600, 740),
    ("table", "large-work-table", "Large work table", "Tables", 2000, 900, 760),
    ("bed", "child-bed", "Child bed", "Beds", 800, 1700, 750),
    ("bed", "single-bed", "Single bed", "Beds", 950, 2100, 900),
    ("bed", "long-single-bed", "Long single bed", "Beds", 1000, 2250, 900),
    ("bed", "double-bed", "Double bed", "Beds", 1450, 2100, 950),
    ("bed", "queen-bed", "Queen bed", "Beds", 1650, 2150, 1000),
    ("bed", "king-bed", "King bed", "Beds", 1850, 2150, 1000),
    ("bed", "super-king-bed", "Super-king bed", "Beds", 2050, 2150, 1100),
    ("bed", "daybed", "Daybed", "Beds", 950, 2100, 800, "daybed"),
    ("crib", "crib", "Baby crib", "Beds", 750, 1450, 1000),
    ("bed", "bunk-bed", "Bunk bed", "Beds", 1000, 2150, 1750, "bunk"),
    ("cabinet", "single-wardrobe", "Single wardrobe", "Storage", 600, 600, 2000),
    ("cabinet", "double-wardrobe", "Double wardrobe", "Storage", 1200, 600, 2100),
    ("cabinet", "triple-wardrobe", "Triple wardrobe", "Storage", 1800, 600, 2100),
    ("cabinet", "low-wardrobe", "Low wardrobe", "Storage", 1000, 550, 1500),
    (
        "cabinet",
        "three-drawer-dresser",
        "Three-drawer dresser",
        "Storage",
        900,
        450,
        800,
        "drawers",
    ),
    ("cabinet", "wide-dresser", "Wide dresser", "Storage", 1400, 450, 850, "drawers"),
    (
        "cabinet",
        "tall-drawer-chest",
        "Tall drawer chest",
        "Storage",
        650,
        450,
        1200,
        "drawers",
    ),
    (
        "cabinet",
        "small-nightstand",
        "Small nightstand",
        "Storage",
        400,
        350,
        500,
        "drawers",
    ),
    (
        "cabinet",
        "wide-nightstand",
        "Wide nightstand",
        "Storage",
        550,
        400,
        550,
        "drawers",
    ),
    ("shelf", "two-shelf-bookcase", "Two-shelf bookcase", "Storage", 700, 300, 800),
    (
        "shelf",
        "three-shelf-bookcase",
        "Three-shelf bookcase",
        "Storage",
        800,
        300,
        1200,
    ),
    ("shelf", "tall-bookcase", "Tall bookcase", "Storage", 800, 320, 2000),
    ("shelf", "low-bookcase", "Low wide bookcase", "Storage", 1400, 300, 750),
    ("shelf", "four-cube-shelf", "Four-cube shelf", "Storage", 760, 360, 760, "cubes"),
    ("shelf", "six-cube-shelf", "Six-cube shelf", "Storage", 1100, 360, 760, "cubes"),
    ("cabinet", "compact-sideboard", "Compact sideboard", "Storage", 1200, 420, 800),
    ("cabinet", "wide-sideboard", "Wide sideboard", "Storage", 1800, 450, 850),
    ("cabinet", "shoe-cabinet", "Shoe cabinet", "Storage", 800, 250, 1000),
    ("cabinet", "compact-tv-stand", "Compact TV stand", "Storage", 1200, 400, 450),
    ("cabinet", "wide-tv-stand", "Wide TV stand", "Storage", 1800, 450, 500),
    ("desk", "compact-desk", "Compact desk", "Office", 900, 500, 740),
    ("desk", "standard-desk", "Standard desk", "Office", 1200, 600, 740),
    ("desk", "wide-desk", "Wide desk", "Office", 1600, 750, 750),
    ("desk", "left-corner-desk", "Left-return desk", "Office", 1600, 1200, 750, "left"),
    (
        "desk",
        "right-corner-desk",
        "Right-return desk",
        "Office",
        1600,
        1200,
        750,
        "right",
    ),
    ("desk", "standing-desk", "Standing desk", "Office", 1400, 700, 1100),
    (
        "cabinet",
        "filing-cabinet",
        "Filing cabinet",
        "Office",
        450,
        600,
        1050,
        "drawers",
    ),
    (
        "cabinet",
        "rolling-pedestal",
        "Under-desk pedestal",
        "Office",
        420,
        500,
        600,
        "drawers",
    ),
    ("shelf", "printer-stand", "Printer stand", "Office", 600, 500, 700),
    ("table", "meeting-table", "Meeting table", "Office", 2200, 1000, 740),
    (
        "cabinet",
        "kitchen-base-cabinet",
        "Kitchen base cabinet",
        "Kitchen and bathroom",
        600,
        600,
        900,
    ),
    (
        "cabinet",
        "kitchen-wall-cabinet",
        "Kitchen wall cabinet",
        "Kitchen and bathroom",
        800,
        350,
        700,
    ),
    (
        "cabinet",
        "pantry-cabinet",
        "Tall pantry cabinet",
        "Kitchen and bathroom",
        600,
        600,
        2100,
    ),
    (
        "sink",
        "sink-cabinet",
        "Kitchen sink cabinet",
        "Kitchen and bathroom",
        900,
        600,
        900,
    ),
    (
        "cabinet",
        "kitchen-island",
        "Kitchen island",
        "Kitchen and bathroom",
        1600,
        800,
        900,
    ),
    (
        "appliance",
        "refrigerator",
        "Refrigerator",
        "Kitchen and bathroom",
        650,
        700,
        1850,
    ),
    (
        "appliance",
        "oven",
        "Freestanding oven",
        "Kitchen and bathroom",
        600,
        600,
        900,
        "oven",
    ),
    (
        "appliance",
        "washing-machine",
        "Washing machine",
        "Kitchen and bathroom",
        600,
        600,
        850,
        "washer",
    ),
    (
        "sink",
        "bathroom-vanity",
        "Bathroom vanity",
        "Kitchen and bathroom",
        800,
        500,
        850,
    ),
    (
        "cabinet",
        "bathroom-tower",
        "Bathroom storage tower",
        "Kitchen and bathroom",
        350,
        350,
        1700,
    ),
    ("bench", "garden-bench", "Garden bench", "Outdoor", 1500, 500, 800, "back"),
    ("chair", "patio-chair", "Patio armchair", "Outdoor", 620, 650, 900, "arms"),
    ("table", "patio-table", "Patio table", "Outdoor", 1400, 800, 750),
    ("sofa", "outdoor-loveseat", "Outdoor loveseat", "Outdoor", 1500, 800, 800),
    (
        "cabinet",
        "outdoor-storage-box",
        "Outdoor storage box",
        "Outdoor",
        1200,
        550,
        600,
    ),
]


def geometry(recipe):
    kind, _, _, _, wm, dm, hm, *option = recipe
    w, d, h = wm / 1000, dm / 1000, hm / 1000
    style = option[0] if option else ""
    parts = []

    def add(x, y, z, width, height, depth, color=WOOD):
        parts.append(box(x, y, z, width, height, depth, color))

    def legs(top, thick=0.055):
        for x in [-w / 2 + thick, w / 2 - thick]:
            for z in [-d / 2 + thick, d / 2 - thick]:
                add(x, top / 2, z, thick, top, thick)

    if kind in ["table", "desk", "stool", "bench"]:
        seat = h * 0.55 if style == "back" else h
        add(0, seat - 0.025, 0, w, 0.05, d, FABRIC if style == "padded" else WOOD)
        legs(seat - 0.05)
        if style == "back":
            add(0, (seat + h) / 2, d / 2 - 0.025, w, h - seat, 0.05)
        if kind == "desk":
            if style in ["left", "right"]:
                # Cut-out worktop: two rectangles rather than a solid footprint.
                parts[0] = box(0, h - 0.025, d / 2 - 0.25, w, 0.05, 0.5, WOOD)
                add(
                    (-1 if style == "left" else 1) * (w / 2 - 0.25),
                    h - 0.025,
                    -0.25,
                    0.5,
                    0.05,
                    d - 0.5,
                )
            else:
                add(w * 0.3, h * 0.43, 0, w * 0.23, h * 0.65, d * 0.8, LIGHT)
    elif kind == "chair":
        seat = h * 0.51 if h < 1 else h * 0.70
        add(0, seat - 0.045, 0, w, 0.09, d, FABRIC)
        legs(seat - 0.09)
        if style == "slats":
            for x in [-w * 0.4, 0, w * 0.4]:
                add(x, (seat + h) / 2, d / 2 - 0.025, 0.055, h - seat, 0.05)
            add(0, h - 0.035, d / 2 - 0.025, w, 0.07, 0.05)
        else:
            add(
                0,
                (seat + h) / 2,
                d / 2 - 0.035,
                w,
                h - seat,
                0.07,
                FABRIC if style == "padded" else WOOD,
            )
        if style == "arms":
            for x in [-w / 2 + 0.035, w / 2 - 0.035]:
                add(x, seat + (h - seat) * 0.42, 0, 0.07, 0.07, d)
    elif kind in ["sofa", "ottoman"]:
        seat = h if kind == "ottoman" else min(0.45, h * 0.6)
        add(0, 0.09, 0, w * 0.9, 0.18, d * 0.9, DARK)
        if style in ["left", "right"]:
            side = -1 if style == "left" else 1
            add(0, seat / 2 + 0.06, d / 2 - 0.45, w, seat - 0.12, 0.9, FABRIC)
            add(
                side * (w / 2 - 0.4),
                seat / 2 + 0.06,
                -0.45,
                0.8,
                seat - 0.12,
                d - 0.9,
                FABRIC,
            )
        else:
            add(0, (seat + 0.16) / 2, 0, w, seat - 0.16, d, FABRIC)
        if kind == "sofa":
            add(0, (seat + h) / 2, d / 2 - 0.07, w, h - seat, 0.14, FABRIC)
            for x in [-w / 2 + 0.06, w / 2 - 0.06]:
                add(
                    x,
                    seat,
                    d / 2 - min(d, 0.9) / 2,
                    0.12,
                    h * 0.38,
                    min(d, 0.9),
                    FABRIC,
                )
    elif kind in ["bed", "crib"]:
        if kind == "crib":
            add(0, 0.28, 0, w, 0.12, d, LIGHT)
            for x in [-w / 2 + 0.025, w / 2 - 0.025]:
                for i in range(9):
                    add(x, 0.55, -d / 2 + 0.03 + (d - 0.06) * i / 8, 0.035, 0.9, 0.035)
                add(x, h - 0.025, 0, 0.05, 0.05, d)
            for z in [-d / 2 + 0.03, d / 2 - 0.03]:
                add(0, h / 2, z, w, h, 0.06)
        else:
            for y in [0.35, 1.35] if style == "bunk" else [0.35]:
                add(0, y - 0.15, 0, w, 0.15, d)
                add(0, y, 0, w - 0.08, 0.15, d - 0.12, LIGHT)
                add(0, y + 0.09, -d * 0.15, w - 0.09, 0.03, d * 0.55, FABRIC)
                add(0, y + 0.12, d * 0.32, w * 0.72, 0.09, d * 0.18, LIGHT)
            add(0, h / 2, d / 2 - 0.035, w, h, 0.07)
            if style in ["bunk", "daybed"]:
                add(-w / 2 + 0.025, h / 2, 0, 0.05, h, d)
                add(w / 2 - 0.025, h / 2, 0, 0.05, h, d)
    else:
        # Open shelves, cabinets and appliance silhouettes. Front faces point -Z.
        t = min(0.035, w * 0.08, d * 0.08)
        add(0, h / 2, d / 2 - t / 2, w, h, t, LIGHT)
        for x in [-w / 2 + t / 2, w / 2 - t / 2]:
            add(x, h / 2, 0, t, h, d)
        for y in [t / 2, h - t / 2]:
            add(0, y, 0, w, t, d)
        if kind == "shelf":
            n = max(1, round(h / 0.4))
            for i in range(1, n):
                add(0, h * i / n, 0, w, t, d)
            if style == "cubes":
                for i in range(1, round(w / 0.38)):
                    add(-w / 2 + w * i / round(w / 0.38), h / 2, 0, t, h, d)
        else:
            n = max(1, round(h / 0.28)) if style == "drawers" else 2
            for i in range(n):
                y = (i + 0.5) * h / n
                add(0, y, -d / 2 + t / 2, w - 0.012, h / n - 0.012, t, LIGHT)
                add(w * 0.23, y, -d / 2, w * 0.13, 0.025, 0.025, DARK)
            if kind == "sink":
                add(0, h - 0.045, 0, w * 0.65, 0.02, d * 0.60, DARK)
                add(0, h - 0.055, 0, w * 0.5, 0.02, d * 0.42, LIGHT)
            if kind == "appliance" and style in ["oven", "washer"]:
                add(0, h * 0.43, -d / 2 - 0.001, w * 0.72, h * 0.53, 0.012, DARK)
    return parts


class Buf(C.Structure):
    _fields_ = [("data", C.c_void_p), ("len", C.c_uint64), ("token", C.c_uint64)]


def generate(output, bridge):
    output.mkdir(parents=True, exist_ok=False)
    models, thumbs = output / "models", output / "thumbnails"
    models.mkdir()
    thumbs.mkdir()
    lib = C.CDLL(str(bridge))
    lib.domowik_plan_create_default.argtypes = [C.POINTER(C.c_uint64)]
    lib.domowik_plan_assets.argtypes = [
        C.c_uint64,
        C.c_char_p,
        C.c_uint64,
        C.POINTER(Buf),
    ]
    lib.domowik_plan_close.argtypes = [C.c_uint64]
    lib.domowik_buffer_free.argtypes = [C.c_uint64]
    handle = C.c_uint64()
    assert lib.domowik_plan_create_default(C.byref(handle)) == 0
    entries = []
    try:
        for recipe in RECIPES:
            _, slug, name, category, *_ = recipe
            parts = geometry(recipe)
            path = original.model(
                slug,
                parts,
                directory=models,
                generator="Domowik original furniture catalog v1; Apache-2.0",
            )
            raw = path.read_bytes()
            length = struct.unpack_from("<I", raw, 12)[0]
            doc = json.loads(raw[20 : 20 + length])
            doc["asset"]["copyright"] = "Copyright Domowik contributors; Apache-2.0"
            doc["asset"]["extras"] = {
                "license": "Apache-2.0",
                "licenseText": (ROOT / "LICENSE").read_text(),
                "notice": "Domowik original furniture catalog v1. Copyright Domowik contributors. Original generic designs; no external catalog content.",
                "catalogId": slug,
                "source": "Domowik original procedural catalog v1",
            }
            text = json.dumps(doc, separators=(",", ":")).encode()
            text += b" " * (-len(text) % 4)
            binary = raw[20 + length :]
            raw = (
                b"glTF"
                + struct.pack("<II", 2, 20 + len(text) + len(binary))
                + struct.pack("<II", len(text), 0x4E4F534A)
                + text
                + binary
            )
            path.write_bytes(raw)
            digest = hashlib.sha256(raw).hexdigest()
            final = models / f"{slug}.{digest}.glb"
            path.rename(final)
            request = json.dumps(
                {
                    "kind": "thumbnail",
                    "file": str(final.resolve()),
                    "sourceDirectory": str(models.resolve()),
                }
            ).encode()
            out = Buf()
            code = lib.domowik_plan_assets(handle, request, len(request), C.byref(out))
            if code:
                raise RuntimeError(f"Rust decoder/thumbnail refused {slug}: {code}")
            try:
                preview = json.loads(C.string_at(out.data, out.len))
            finally:
                lib.domowik_buffer_free(out.token)
            png = bytes(preview["png"])
            png_hash = hashlib.sha256(png).hexdigest()
            png_path = thumbs / f"{slug}.{png_hash}.png"
            png_path.write_bytes(png)
            # Dimensions are measured from the actual recipe, not assumed from
            # its nominal size (handles/rails can extend the silhouette).
            dimensions = [
                round(
                    1000
                    * (
                        max(c[i] + s[i] / 2 for c, s, _ in parts)
                        - min(c[i] - s[i] / 2 for c, s, _ in parts)
                    )
                )
                for i in [0, 2, 1]
            ]
            entries.append(
                {
                    "id": slug,
                    "name": name,
                    "category": category,
                    "dimensionsMm": dimensions,
                    "license": "Apache-2.0",
                    "model": {
                        "path": str(final.relative_to(output)),
                        "sha256": digest,
                        "bytes": len(raw),
                    },
                    "thumbnail": {
                        "path": str(png_path.relative_to(output)),
                        "sha256": png_hash,
                        "bytes": len(png),
                    },
                }
            )
    finally:
        lib.domowik_plan_close(handle)
    assert len(entries) == 100 and len({e["id"] for e in entries}) == 100
    index = {
        "catalogVersion": 1,
        "id": "domowik-common-furniture-v1",
        "models": entries,
    }
    (output / "index.json").write_text(json.dumps(index, separators=(",", ":")) + "\n")
    shutil.copyfile(ROOT / "LICENSE", output / "LICENSE.txt")
    (output / "NOTICE.txt").write_text(
        "Domowik original furniture catalog v1\nCopyright Domowik contributors. Apache-2.0; see LICENSE.txt.\nOriginal invented generic designs/dimensions. No external models, textures, images or catalogs.\nGenerated by catalog/generate.py and the first-party Welcome Home GLB box builder.\nThumbnails derived by the Domowik Rust model decoder/rasterizer.\nEach model retains its license/provenance in glTF asset metadata.\n"
    )
    print(
        f"Generated {len(entries)} original models; index {len((output / 'index.json').read_bytes())} B; "
        f"models {sum(e['model']['bytes'] for e in entries)} B; thumbnails {sum(e['thumbnail']['bytes'] for e in entries)} B"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument(
        "--bridge",
        type=pathlib.Path,
        default=ROOT
        / (
            "frontend/target/release/libdomowik_bridge."
            + ("dylib" if sys.platform == "darwin" else "so")
        ),
    )
    args = parser.parse_args()
    generate(args.output, args.bridge)
