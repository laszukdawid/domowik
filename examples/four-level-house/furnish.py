"""Original box-built decoration and illustrative access models, via the C ABI.

Reuses the project's Apache-2.0 Welcome Home models and GLB box builder.
No third-party designs, catalog data, artwork or Python packages are used.
"""

from collections import Counter
import ctypes as C
import json
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
WELCOME = ROOT / "examples/welcome-home"
WOOD = [0.68, 0.43, 0.23]
CREAM = [0.93, 0.88, 0.76]
SAGE = [0.29, 0.49, 0.40]
CORAL = [0.72, 0.35, 0.27]
DARK = [0.16, 0.22, 0.22]
REUSED_MODELS = (
    "sage-sofa",
    "coffee-table",
    "dining-set",
    "double-bed",
    "kitchen-counter",
    "wardrobe",
    "bathtub",
    "vanity",
    "planter",
)

# Positions are footprint centres in plan mm; Z is relative to the level.
# Optional dimensions/height intentionally resize rugs, not the stair rise.
PLACEMENTS = {
    "Ground floor": [
        ("woven-rug", 1500, 1850, 0, 0, (2300, 2400), 20),
        ("sage-sofa", 1500, 650, 0, 0),
        ("coffee-table", 1500, 1900, 0, 0),
        ("planter", 500, 3300, 0, 0),
        ("dining-set", 1500, 6100, 0, 0),
        ("planter", 450, 7450, 0, 0),
        ("low-storage", 2600, 7100, 0, 90),
        ("kitchen-counter", 7900, 7450, 0, 0),
        ("bathtub", 7900, 650, 0, 0),
        ("vanity", 9500, 2200, 0, 90),
        ("planter", 6400, 700, 0, 0),
        ("planter", 5300, 900, 0, 0),
        ("return-stair", 4400, 6375, 0, 0),
        ("crawlspace-access-hatch", 9200, 6200, 0, 0),
    ],
    "Upper floor": [
        ("woven-rug", 1150, 1450, 0, 0, (2000, 2500), 20),
        ("double-bed", 1150, 1400, 0, 0),
        ("wardrobe", 2500, 3200, 0, 90),
        ("study-desk", 1200, 3300, 0, 0),
        ("woven-rug", 1150, 5500, 0, 0, (2000, 2500), 20),
        ("double-bed", 1150, 5450, 0, 0),
        ("wardrobe", 2500, 7100, 0, 90),
        ("study-desk", 1200, 7300, 0, 0),
        ("woven-rug", 7900, 4400, 0, 0, (2300, 2600), 20),
        ("double-bed", 7900, 4300, 0, 0),
        ("wardrobe", 9500, 6900, 0, 90),
        ("study-desk", 7200, 7300, 0, 0),
        ("planter", 6300, 7300, 0, 0),
        ("bathtub", 7900, 650, 0, 0),
        ("vanity", 9500, 2200, 0, 90),
        ("stairwell-marker", 4400, 6375, 0, 0),
        ("attic-access-hatch", 4400, 2900, 2500, 0),
        ("attic-access-ladder", 4400, 2900, 0, 0),
    ],
    "Attic": [
        ("low-storage", 1700, 650, 0, 0),
        ("low-storage", 3200, 650, 0, 0),
        ("low-storage", 9200, 1800, 0, 90),
        ("attic-access-hatch", 4400, 2900, 0, 0),
    ],
    "Crawlspace": [
        ("crawlspace-access-hatch", 9200, 6200, 865, 0),
    ],
}


def models(directory=None):
    """Create deterministic original models; return these plus reused originals."""
    directory = HERE / "models" if directory is None else directory
    directory.mkdir(parents=True, exist_ok=True)
    builder = runpy.run_path(str(WELCOME / "generate.py"))
    box = builder["box"]

    def model(name, boxes):
        return builder["model"](
            name,
            boxes,
            directory=directory,
            generator="Domowik original Four-level house example",
        )

    files = {name: WELCOME / "models" / f"{name}.glb" for name in REUSED_MODELS}
    files["woven-rug"] = model(
        "woven-rug",
        [
            box(0, 0.01, 0, 2, 0.02, 2.5, CREAM),
            *[box(x, 0.021, 0, 0.07, 0.002, 2.35, SAGE) for x in [-0.85, 0.85]],
            *[box(0, 0.021, z, 1.7, 0.002, 0.07, CORAL) for z in [-1.1, 1.1]],
        ],
    )
    files["low-storage"] = model(
        "low-storage",
        [
            box(0, 0.45, 0, 1.2, 0.9, 0.5, CREAM),
            box(0, 0.94, 0, 1.25, 0.08, 0.55, WOOD),
            box(0, 0.47, -0.255, 0.025, 0.8, 0.02, WOOD),
            *[box(x, 0.55, -0.28, 0.10, 0.025, 0.025, DARK) for x in [-0.15, 0.15]],
        ],
    )
    files["study-desk"] = model(
        "study-desk",
        [
            box(0, 0.74, 0.2, 1.1, 0.08, 0.55, WOOD),
            *[
                box(x, 0.35, z, 0.06, 0.7, 0.06, DARK)
                for x in [-0.47, 0.47]
                for z in [0, 0.4]
            ],
            box(0, 0.44, -0.35, 0.42, 0.08, 0.42, SAGE),
            box(0, 0.65, -0.54, 0.42, 0.48, 0.06, SAGE),
            *[
                box(x, 0.2, z, 0.045, 0.4, 0.045, WOOD)
                for x in [-0.16, 0.16]
                for z in [-0.5, -0.2]
            ],
        ],
    )

    # A 2800 mm rise: 16 illustrative 175 mm risers in two returning flights.
    # glTF +Z is plan south. The shared north landing is at 1400 mm.
    stair = [box(0, 1.35, -0.9, 2.4, 0.1, 0.95, WOOD)]
    marker = [box(0, 0.01, -0.9, 2.4, 0.02, 0.95, WOOD)]
    for step in range(8):
        z = 1.25 - step * 0.25
        lower = (step + 1) * 0.175
        upper = 2.8 - step * 0.175
        stair += [
            box(-0.7, lower / 2, z, 1, lower, 0.25, CREAM),
            box(-0.7, lower - 0.015, z, 1, 0.03, 0.25, WOOD),
            box(0.7, upper / 2, z, 1, upper, 0.25, CREAM),
            box(0.7, upper - 0.015, z, 1, 0.03, 0.25, WOOD),
        ]
        marker += [box(x, 0.018, z, 1, 0.012, 0.025, SAGE) for x in [-0.7, 0.7]]
    # Guard posts stay inside the stair cuboid; these are not certified rails.
    stair += [
        box(0, 0.65 + i * 0.175, 1.25 - i * 0.25, 0.06, 1.0, 0.06, DARK)
        for i in range(7)
    ]
    files["return-stair"] = model("return-stair", stair)
    files["stairwell-marker"] = model("stairwell-marker", marker)

    for name, color in [
        ("crawlspace-access-hatch", SAGE),
        ("attic-access-hatch", CORAL),
    ]:
        files[name] = model(
            name,
            [
                box(0, 0.0125, 0, 0.8, 0.025, 1, WOOD),
                box(0, 0.0275, 0, 0.68, 0.005, 0.88, color),
                box(0, 0.0325, -0.28, 0.20, 0.005, 0.035, DARK),
                *[box(x, 0.03, 0.40, 0.08, 0.01, 0.10, DARK) for x in [-0.25, 0.25]],
            ],
        )
    files["attic-access-ladder"] = model(
        "attic-access-ladder",
        [
            *[box(x, 1.4, 0, 0.08, 2.8, 0.08, WOOD) for x in [-0.35, 0.35]],
            *[box(0, 0.2 + i * 0.22, 0, 0.62, 0.04, 0.08, CREAM) for i in range(12)],
            box(0, 0.015, 0, 0.85, 0.03, 0.65, DARK),
        ],
    )
    return files


class Buffer(C.Structure):
    _fields_ = [("data", C.c_void_p), ("len", C.c_uint64), ("token", C.c_uint64)]


class PlanAssets:
    """Own one bridge handle and free every returned buffer before closing it."""

    def __init__(self, path):
        suffix = "dylib" if sys.platform == "darwin" else "so"
        self.lib = C.CDLL(
            str(ROOT / f"frontend/target/release/libdomowik_bridge.{suffix}")
        )
        if self.lib.domowik_abi_version() != 17:
            raise RuntimeError("This generator requires native bridge ABI 17")
        self.lib.domowik_plan_open.argtypes = [C.c_char_p, C.POINTER(C.c_uint64)]
        self.lib.domowik_plan_import_model.argtypes = [
            C.c_uint64,
            C.c_char_p,
            C.c_uint64,
            C.c_char_p,
            C.c_uint64,
            C.POINTER(Buffer),
        ]
        self.lib.domowik_plan_assets.argtypes = [
            C.c_uint64,
            C.c_char_p,
            C.c_uint64,
            C.POINTER(Buffer),
        ]
        self.lib.domowik_plan_save.argtypes = [C.c_uint64, C.c_char_p]
        self.lib.domowik_plan_close.argtypes = [C.c_uint64]
        self.lib.domowik_buffer_free.argtypes = [C.c_uint64]
        self.handle = C.c_uint64()
        self.path = path
        self.status(
            self.lib.domowik_plan_open(str(path).encode(), C.byref(self.handle))
        )

    @staticmethod
    def status(code):
        if code != 0:
            raise RuntimeError(f"Native bridge returned status {code}")

    def reply(self, function, *args):
        out = Buffer()
        self.status(function(self.handle, *args, C.byref(out)))
        try:
            return json.loads(C.string_at(out.data, out.len))
        finally:
            self.status(self.lib.domowik_buffer_free(out.token))

    def request(self, **request):
        raw = json.dumps(request).encode()
        return self.reply(self.lib.domowik_plan_assets, raw, len(raw))

    def close(self):
        self.status(self.lib.domowik_plan_close(self.handle))


def check_assets(state, files, levels):
    expected = Counter()
    for level, placements in PLACEMENTS.items():
        for name, x, y, z, angle, *size in placements:
            expected[(levels[level], name, x, y, z, angle)] += 1
    actual = Counter(
        (a["levelId"], a["name"], *a["positionMm"], a["pose3d"][0], a["rotationDeg"])
        for a in state["instances"]
    )
    if actual != expected:
        raise RuntimeError("Unexpected furniture/access placement or level")
    if len(state["models"]) != len(files) or state["directories"]:
        raise RuntimeError("Expected only the self-contained original model library")
    for instance in state["instances"]:
        name = instance["name"]
        if name in ("return-stair", "attic-access-ladder"):
            if abs(instance["heightMm"] - 2800) > 1:
                raise RuntimeError(f"Incorrect living-floor rise for {name}")
        if instance["levelId"] == levels["Attic"] and name == "low-storage":
            if instance["heightMm"] > 1500:
                raise RuntimeError("Storage is taller than the attic")


def furnish(path, levels, files):
    assets = PlanAssets(path)
    try:
        if assets.request(kind="list")["instances"]:
            raise RuntimeError(
                "Refusing to replace existing furniture; use a fresh example"
            )
        digests = {}
        for name, file in files.items():
            data = file.read_bytes()
            origin = (
                "welcome-home" if file.parent.parent == WELCOME else "four-level-house"
            )
            source = f"examples/{origin}/models/{file.name}".encode()
            digests[name] = assets.reply(
                assets.lib.domowik_plan_import_model,
                source,
                len(source),
                data,
                len(data),
            )["digest"]
        for level, placements in PLACEMENTS.items():
            for name, x, y, z, angle, *size in placements:
                placed = assets.request(
                    kind="place",
                    digest=digests[name],
                    levelId=levels[level],
                    positionMm=[x, y],
                )
                instance = placed["instanceId"]
                if size:
                    dimensions, height = size
                    assets.request(
                        kind="set",
                        instanceId=instance,
                        positionMm=[x, y],
                        dimensionsMm=dimensions,
                        rotationDeg=angle,
                        heightMm=height,
                    )
                if z or angle:
                    assets.request(
                        kind="pose",
                        instanceId=instance,
                        positionMm=[x, y, z],
                        rotationDeg=[0, 0, angle],
                    )
        check_assets(assets.request(kind="list"), files, levels)
        assets.status(assets.lib.domowik_plan_save(assets.handle, str(path).encode()))
    finally:
        assets.close()
    reopened = PlanAssets(path)
    try:
        check_assets(reopened.request(kind="list"), files, levels)
    finally:
        reopened.close()
