#!/usr/bin/env python3
"""Author an original, furnished four-level example through Domowik APIs.

Python standard library only; no external plans, models or images are used.
All dimensions are invented millimetres, not survey measurements.
"""

import json
from pathlib import Path
import subprocess

from furnish import furnish, models

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
CLI = ROOT / "target/release/domowik"
PLAN = HERE / "four-level-house.domo"
LEVELS = {
    "Ground floor": (0, 2500),
    "Upper floor": (2800, 2500),
    "Crawlspace": (-1200, 900),
    "Attic": (5600, 1500),
}
ROOMS = {
    "Ground floor": [
        (1500, 2000, "Common room"),
        (1500, 6000, "Dining space"),
        (7900, 5500, "Kitchen"),
        (7900, 1500, "Bathroom"),
        (4400, 4000, "Hall"),
    ],
    "Upper floor": [
        (1500, 2000, "Bedroom 1"),
        (1500, 6000, "Bedroom 2"),
        (7900, 5500, "Bedroom 3"),
        (7900, 1500, "Bathroom"),
        (4400, 4000, "Landing"),
    ],
    "Crawlspace": [(8000, 6000, "Crawlspace")],
    "Attic": [(5000, 4000, "Attic")],
}


def cli(*args):
    return subprocess.check_output([str(CLI), *map(str, args)], text=True)


def info():
    return json.loads(cli("info", PLAN, "--json"))


def wall(level, start, end, thickness=120):
    cli("add-wall", PLAN, *start, *end, "--thickness", thickness, "--level", level)


def rectangle(level, west, south, east, north):
    corners = [(west, south), (east, south), (east, north), (west, north)]
    for start, end in zip(corners, corners[1:] + corners[:1]):
        wall(level, start, end, thickness=200)


def opening(level_info, start, end, kind, width, *options):
    # Match the CLI's reported endpoints, not mutable wall numbers or ambiguous
    # points shared by several levels. Rust has already performed every split.
    matches = [
        w
        for w in level_info["walls"]
        if {tuple(w["startMm"]), tuple(w["endMm"])} == {start, end}
    ]
    if len(matches) != 1:
        raise RuntimeError(f"Expected one wall on {level_info['name']}: {start}, {end}")
    cli("add-opening", PLAN, matches[0]["id"], kind, "--width", width, *options)


def check_plan():
    """Check semantic expectations as well as format and drafting diagnostics."""
    cli("validate", PLAN)
    cli("check", PLAN, "--strict")
    document = info()
    levels = {level["name"]: level for level in document["levels"]}
    if len(document["levels"]) != 4 or set(levels) != set(LEVELS):
        raise RuntimeError("Expected exactly the four named levels")
    expected_openings = {
        "Ground floor": (6, 4),
        "Upper floor": (4, 4),
        "Crawlspace": (0, 0),
        "Attic": (0, 2),
    }
    for name, (elevation, height) in LEVELS.items():
        level = levels[name]
        expected = sorted(room[2] for room in ROOMS[name])
        actual = sorted(room["label"] for room in level["rooms"])
        if actual != expected or len(level["roomRecords"]) != len(expected):
            raise RuntimeError(f"Unexpected rooms on {name}: {actual}")
        if any(record["status"] != "associated" for record in level["roomRecords"]):
            raise RuntimeError(f"Unplaced room name on {name}")
        if (
            level["elevationMm"],
            level["defaultWallHeightMm"],
            level["defaultCeilingHeightMm"],
            level["floorThicknessMm"],
        ) != (elevation, height, height, 300):
            raise RuntimeError(f"Unexpected level dimensions on {name}")
        if any(w["heightMm"] != height for w in level["walls"]):
            raise RuntimeError(f"Unexpected wall height on {name}")
        openings = [o for w in level["walls"] for o in w["openings"]]
        counts = tuple(
            sum(o["kind"] == kind for o in openings) for kind in ("door", "window")
        )
        if counts != expected_openings[name]:
            raise RuntimeError(f"Unexpected door/window counts on {name}: {counts}")
        cli("check", PLAN, "--strict", "--level", name)
    cli("asset", "verify", PLAN)


def main():
    if PLAN.exists():
        raise SystemExit(
            "Move the existing four-level-house.domo aside before "
            "regenerating; it may contain your edits."
        )
    cli("new", PLAN)
    cli("set-level", PLAN, "Level 1", "--rename", "Ground floor")
    for name, (elevation, height) in LEVELS.items():
        if name != "Ground floor":
            cli("add-level", PLAN, name, "--elevation", elevation)
        # Set defaults before drawing, including genuinely low crawlspace walls.
        cli(
            "set-level",
            PLAN,
            name,
            "--wall-height",
            height,
            "--ceiling-height",
            height,
            "--floor-thickness",
            300,
        )

    for name in ("Ground floor", "Upper floor"):
        rectangle(name, 0, 0, 10000, 8000)
        wall(name, (3000, 0), (3000, 8000))
        wall(name, (5800, 0), (5800, 8000))
        wall(name, (0, 4000), (3000, 4000))
        wall(name, (5800, 3000), (10000, 3000))
    rectangle("Crawlspace", 6000, 4500, 10000, 7500)
    rectangle("Attic", 0, 0, 10000, 8000)

    # Openings do not split walls, so these reported UUIDs remain valid while
    # adding doors/windows. No wall references are carried across wall edits.
    levels = {level["name"]: level for level in info()["levels"]}
    for name in ("Ground floor", "Upper floor"):
        level = levels[name]
        for start, end, side in [
            ((3000, 0), (3000, 4000), "left"),
            ((3000, 4000), (3000, 8000), "left"),
            ((5800, 0), (5800, 3000), "right"),
            ((5800, 3000), (5800, 8000), "right"),
        ]:
            options = []
            if start == (3000, 4000):
                options = ["--offset", 500]
            elif start == (5800, 3000):
                options = ["--offset", 1000]
            opening(level, start, end, "door", 850, "--opens-toward", side, *options)
        for start, end, width in [
            ((0, 0), (0, 4000), 1800),
            ((0, 8000), (3000, 8000), 1800),
            ((10000, 3000), (10000, 8000), 1800),
        ]:
            opening(level, start, end, "window", width)
        opening(
            level,
            (10000, 0),
            (10000, 3000),
            "window",
            900,
            "--height",
            800,
            "--sill",
            1200,
        )
    ground = levels["Ground floor"]
    opening(ground, (3000, 0), (5800, 0), "door", 1000)
    opening(ground, (0, 4000), (3000, 4000), "door", 1600, "--mechanism", "sliding")
    for start, end in [((0, 0), (0, 8000)), ((10000, 0), (10000, 8000))]:
        opening(
            levels["Attic"], start, end, "window", 1000, "--height", 600, "--sill", 600
        )

    for name, rooms in ROOMS.items():
        for x, y, label in rooms:
            cli("room", "add", PLAN, x, y, "--name", label, "--level", name)
    cli("order-levels", PLAN, "Attic", "Upper floor", "Ground floor", "Crawlspace")
    furnish(PLAN, {level["name"]: level["id"] for level in info()["levels"]}, models())
    check_plan()
    for name in LEVELS:
        slug = name.lower().replace(" ", "-")
        cli(
            "export-png",
            PLAN,
            ROOT / f"docs/images/four-level-house-{slug}.png",
            "--scale",
            0.12,
            "--level",
            name,
        )
        cli("export-svg", PLAN, HERE / f"{slug}.svg", "--level", name)
    print(f"Created {PLAN.relative_to(ROOT)}: four levels, twelve named rooms.")


if __name__ == "__main__":
    main()
