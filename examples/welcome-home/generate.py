#!/usr/bin/env python3
"""Generate original demo models and author the plan through Domowik APIs.

Requires a built release CLI and frontend bridge; Python standard library only.
No external geometry, images or catalog content is used. Run from any directory.
"""

import ctypes as C
import json
from pathlib import Path
import struct
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
CLI = ROOT / "target/release/domowik"
PLAN = HERE / "welcome-home.domo"


def model(
    name, boxes, *, directory=None, generator="Domowik original Welcome Home example"
):
    """Axis-aligned boxes in glTF metres (+Y up), with flat face normals."""
    blob = bytearray()
    views, accessors, primitives, materials = [], [], [], []
    for center, size, color in boxes:
        vertices, normals = [], []
        for axis in range(3):
            a, b = (axis + 1) % 3, (axis + 2) % 3
            for sign in [-1, 1]:
                face = []
                for u, v in [(-1, -1), (1, -1), (1, 1), (-1, 1)]:
                    p = list(center)
                    p[axis] += sign * size[axis] / 2
                    p[a] += u * size[a] / 2
                    p[b] += v * size[b] / 2
                    face.append(p)
                order = [0, 1, 2, 0, 2, 3] if sign > 0 else [0, 2, 1, 0, 3, 2]
                for i in order:
                    vertices.extend(face[i])
                    n = [0, 0, 0]
                    n[axis] = sign
                    normals.extend(n)
        attrs = {}
        for semantic, data in [("POSITION", vertices), ("NORMAL", normals)]:
            start = len(blob)
            blob.extend(struct.pack("<" + "f" * len(data), *data))
            views.append(
                {"buffer": 0, "byteOffset": start, "byteLength": len(blob) - start}
            )
            acc = {
                "bufferView": len(views) - 1,
                "componentType": 5126,
                "count": len(data) // 3,
                "type": "VEC3",
            }
            if semantic == "POSITION":
                acc["min"] = [min(data[i::3]) for i in range(3)]
                acc["max"] = [max(data[i::3]) for i in range(3)]
            accessors.append(acc)
            attrs[semantic] = len(accessors) - 1
        materials.append(
            {
                "pbrMetallicRoughness": {
                    "baseColorFactor": [*color, 1],
                    "metallicFactor": 0,
                    "roughnessFactor": 0.85,
                }
            }
        )
        primitives.append({"attributes": attrs, "material": len(materials) - 1})
    doc = {
        "asset": {"version": "2.0", "generator": generator},
        "buffers": [{"byteLength": len(blob)}],
        "bufferViews": views,
        "accessors": accessors,
        "materials": materials,
        "meshes": [{"primitives": primitives}],
        "nodes": [{"mesh": 0}],
        "scenes": [{"nodes": [0]}],
        "scene": 0,
    }
    text = json.dumps(doc, separators=(",", ":")).encode()
    text += b" " * (-len(text) % 4)
    blob.extend(b"\0" * (-len(blob) % 4))
    out = b"glTF" + struct.pack("<II", 2, 28 + len(text) + len(blob))
    out += (
        struct.pack("<II", len(text), 0x4E4F534A)
        + text
        + struct.pack("<II", len(blob), 0x004E4942)
        + blob
    )
    path = (HERE / "models" if directory is None else directory) / f"{name}.glb"
    path.write_bytes(out)
    return path


def box(x, y, z, w, h, d, color):
    return ([x, y, z], [w, h, d], color)


WOOD = [0.68, 0.43, 0.23]
CREAM = [0.93, 0.88, 0.76]
SAGE = [0.29, 0.49, 0.40]
CORAL = [0.72, 0.35, 0.27]
WHITE = [0.96, 0.95, 0.90]
DARK = [0.16, 0.22, 0.22]


def models():
    result = {}
    result["Sage sofa"] = model(
        "sage-sofa",
        [
            box(0, 0.18, 0, 2.2, 0.25, 0.9, WOOD),
            box(0, 0.43, 0, 1.85, 0.25, 0.8, SAGE),
            box(0, 0.77, 0.36, 2.2, 0.6, 0.18, SAGE),
            box(-1, 0.60, 0, 0.2, 0.55, 0.9, SAGE),
            box(1, 0.60, 0, 0.2, 0.55, 0.9, SAGE),
            box(-0.52, 0.63, 0.15, 0.6, 0.15, 0.45, CREAM),
            box(0.5, 0.63, 0.15, 0.6, 0.15, 0.45, CORAL),
        ],
    )
    result["Coffee table"] = model(
        "coffee-table",
        [box(0, 0.40, 0, 1.1, 0.09, 0.65, WOOD)]
        + [
            box(x, 0.2, z, 0.06, 0.4, 0.06, DARK)
            for x in [-0.42, 0.42]
            for z in [-0.22, 0.22]
        ],
    )
    dining = [box(0, 0.74, 0, 1.6, 0.09, 0.85, WOOD)] + [
        box(x, 0.37, z, 0.07, 0.7, 0.07, WOOD)
        for x in [-0.65, 0.65]
        for z in [-0.3, 0.3]
    ]
    for x in [-0.48, 0.48]:
        for z in [-0.85, 0.85]:
            dining += [
                box(x, 0.46, z, 0.43, 0.09, 0.43, SAGE),
                box(x, 0.73, z + (0.18 if z > 0 else -0.18), 0.43, 0.5, 0.07, SAGE),
            ]
            dining += [
                box(x + dx, 0.23, z + dz, 0.045, 0.43, 0.045, WOOD)
                for dx in [-0.16, 0.16]
                for dz in [-0.16, 0.16]
            ]
    result["Dining set"] = model("dining-set", dining)
    result["Double bed"] = model(
        "double-bed",
        [
            box(0, 0.2, 0, 1.65, 0.32, 2.1, WOOD),
            box(0, 0.44, 0, 1.6, 0.22, 2.0, CREAM),
            box(0, 0.57, -0.35, 1.62, 0.10, 1.2, CORAL),
            box(-0.42, 0.59, 0.65, 0.62, 0.16, 0.43, WHITE),
            box(0.42, 0.59, 0.65, 0.62, 0.16, 0.43, WHITE),
            box(0, 0.65, 1.02, 1.7, 1.0, 0.10, WOOD),
        ],
    )
    result["Kitchen counter"] = model(
        "kitchen-counter",
        [
            box(0, 0.45, 0, 3.5, 0.9, 0.65, CREAM),
            box(0, 0.94, 0, 3.6, 0.08, 0.7, WOOD),
            box(-0.95, 0.99, 0, 0.6, 0.025, 0.45, DARK),
            box(0.95, 1.0, 0, 0.6, 0.04, 0.42, WHITE),
        ]
        + [
            box(x, 0.48, -0.33, 0.025, 0.65, 0.015, WOOD)
            for x in [-1.1, -0.4, 0.4, 1.1]
        ],
    )
    result["Wardrobe"] = model(
        "wardrobe",
        [
            box(0, 1.0, 0, 1.4, 2, 0.6, CREAM),
            box(0, 1.0, -0.31, 0.025, 1.9, 0.025, WOOD),
        ],
    )
    result["Bathtub"] = model(
        "bathtub",
        [
            box(0, 0.29, 0, 1.7, 0.55, 0.78, WHITE),
            box(0, 0.58, 0, 1.35, 0.025, 0.5, [0.45, 0.68, 0.69]),
        ],
    )
    result["Vanity"] = model(
        "vanity",
        [
            box(0, 0.42, 0, 1, 0.8, 0.5, WOOD),
            box(0, 0.86, 0, 1.05, 0.08, 0.55, WHITE),
            box(0, 0.92, 0, 0.52, 0.08, 0.34, [0.64, 0.76, 0.73]),
        ],
    )
    result["Planter"] = model(
        "planter",
        [
            box(0, 0.2, 0, 0.35, 0.4, 0.35, CORAL),
            box(0, 0.6, 0, 0.07, 0.7, 0.07, WOOD),
            box(0, 0.95, 0, 0.55, 0.45, 0.5, SAGE),
            box(0.12, 1.2, 0, 0.35, 0.3, 0.35, [0.38, 0.58, 0.36]),
        ],
    )
    return result


def cli(*args):
    subprocess.run([str(CLI), *map(str, args)], check=True, stdout=subprocess.DEVNULL)


class Buf(C.Structure):
    _fields_ = [("data", C.c_void_p), ("len", C.c_uint64), ("token", C.c_uint64)]


def furnish(files, level):
    suffix = "dylib" if sys.platform == "darwin" else "so"
    lib = C.CDLL(str(ROOT / f"frontend/target/release/libdomowik_bridge.{suffix}"))
    lib.domowik_plan_open.argtypes = [C.c_char_p, C.POINTER(C.c_uint64)]
    lib.domowik_plan_import_model.argtypes = [
        C.c_uint64,
        C.c_char_p,
        C.c_uint64,
        C.c_char_p,
        C.c_uint64,
        C.POINTER(Buf),
    ]
    lib.domowik_plan_assets.argtypes = [
        C.c_uint64,
        C.c_char_p,
        C.c_uint64,
        C.POINTER(Buf),
    ]
    lib.domowik_buffer_free.argtypes = [C.c_uint64]
    lib.domowik_plan_save.argtypes = [C.c_uint64, C.c_char_p]
    lib.domowik_plan_close.argtypes = [C.c_uint64]
    h = C.c_uint64()
    assert lib.domowik_plan_open(str(PLAN).encode(), C.byref(h)) == 0

    def reply(fn, *args):
        out = Buf()
        status = fn(h, *args, C.byref(out))
        assert status == 0, f"Bridge returned {status}"
        try:
            return json.loads(C.string_at(out.data, out.len))
        finally:
            lib.domowik_buffer_free(out.token)

    def assets(**req):
        raw = json.dumps(req).encode()
        return reply(lib.domowik_plan_assets, raw, len(raw))

    # Placement origins are footprint centres; the model's ground rests on the level.
    placements = [
        ("Sage sofa", 1800, 1900, 0),
        ("Coffee table", 1800, 3000, 0),
        ("Dining set", 3100, 5700, 0),
        ("Kitchen counter", 3050, 7400, 0),
        ("Double bed", 8000, 6800, 90),
        ("Wardrobe", 9500, 4500, 90),
        ("Bathtub", 8650, 750, 0),
        ("Vanity", 9450, 2450, 90),
        ("Planter", 650, 7100, 0),
        ("Planter", 5350, 1200, 0),
        ("Planter", 6800, 7200, 0),
    ]
    try:
        digests = {}
        for name, path in files.items():
            data = path.read_bytes()
            source = f"examples/welcome-home/models/{path.name}".encode()
            digests[name] = reply(
                lib.domowik_plan_import_model, source, len(source), data, len(data)
            )["digest"]
        for name, x, y, angle in placements:
            r = assets(
                kind="place", digest=digests[name], levelId=level, positionMm=[x, y]
            )
            if angle:
                assets(
                    kind="pose",
                    instanceId=r["instanceId"],
                    positionMm=[x, y, 0],
                    rotationDeg=[0, 0, angle],
                )
        assert lib.domowik_plan_save(h, str(PLAN).encode()) == 0
    finally:
        lib.domowik_plan_close(h)


def main():
    if PLAN.exists():
        raise SystemExit(
            "Move the existing welcome-home.domo aside before regenerating; it may contain your edits."
        )
    files = models()
    cli("new", PLAN)
    for a, b in [
        ((0, 0), (10000, 0)),
        ((10000, 0), (10000, 8000)),
        ((10000, 8000), (0, 8000)),
        ((0, 8000), (0, 0)),
    ]:
        cli("add-wall", PLAN, *a, *b, "--thickness", 200)
    cli("add-wall", PLAN, 6000, 0, 6000, 8000, "--thickness", 120)
    cli("add-wall", PLAN, 6000, 3500, 10000, 3500, "--thickness", 120)
    for point, kind, width in [
        ("4200,0", "door", 1000),
        ("6000,1800", "door", 850),
        ("6000,4550", "door", 900),
        ("0,3000", "window", 2200),
        ("2500,8000", "window", 2200),
        ("8000,8000", "window", 1800),
        ("10000,1800", "window", 1000),
    ]:
        cli("add-opening", PLAN, point, kind, "--width", width)
    for x, y, name in [
        (3000, 4000, "Living / kitchen"),
        (8000, 5400, "Bedroom"),
        (8000, 1800, "Bathroom"),
    ]:
        cli("room", "add", PLAN, x, y, "--name", name)
    info = json.loads(subprocess.check_output([str(CLI), "info", str(PLAN), "--json"]))
    furnish(files, info["levels"][0]["id"])
    cli("validate", PLAN)
    cli("check", PLAN, "--strict")
    cli("asset", "verify", PLAN)
    cli("export-png", PLAN, ROOT / "docs/images/welcome-home-plan.png", "--scale", 0.12)
    cli("export-svg", PLAN, HERE / "welcome-home.svg")
    print(f"Created {PLAN.relative_to(ROOT)} and {len(files)} original models.")


if __name__ == "__main__":
    main()
