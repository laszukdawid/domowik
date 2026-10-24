#!/usr/bin/env python3
"""Environment capture, report and comparison for `task benchmark`.

    benchmark_report.py environment          JSON about this machine, stdout
    benchmark_report.py report RUN_DIR       Markdown report for one run
    benchmark_report.py compare RUN_A RUN_B  comparison written owner-only to
                                             private/benchmarks/compare-*.md
    benchmark_report.py public-summary [RUN] health checks safe to post
                                             publicly (latest run by default)

Reports describe one machine in detail and are CONFIDENTIAL: they stay in the
gitignored private/ tree and are moved between machines by hand. Standard
library only. Percentiles are nearest-rank (docs/BENCHMARK-INTERACTION.md).
"""

import datetime
import json
import os
import platform
import re
import shutil
import subprocess
import sys
from pathlib import Path

FRAME_60HZ_MS = 1000 / 60
FRAME_30HZ_MS = 1000 / 30
CORE_BUDGET_MS = 8.0  # docs/BENCHMARK-INTERACTION.md: p99 gesture latency.
EDIT_TO_3D_REFERENCE_MS = 100.0  # Provisional reference line, not a budget.

# Latency metrics: (key, label, reference ms, what the reference is).
LATENCIES = [
    ("pickMs", "3D pick", CORE_BUDGET_MS, "core budget"),
    ("selectTo3dMs", "Selection → 3D tint", EDIT_TO_3D_REFERENCE_MS, "reference"),
    ("dragInputTo3dMs", "Drag input → 3D preview", 2 * FRAME_60HZ_MS, "two 60 Hz frames"),
    ("previewLoadMs", "Drag preview scene update (load only)", FRAME_60HZ_MS, "one 60 Hz frame"),
    ("commitAndRefreshMs", "Edit commit + 2D refresh", CORE_BUDGET_MS, "core budget"),
    ("editTo3dMs", "Edit → 3D rebuilt", EDIT_TO_3D_REFERENCE_MS, "reference"),
    ("historyStepMs", "Undo/redo + 2D refresh", CORE_BUDGET_MS, "core budget"),
    ("historyTo3dMs", "Undo/redo → 3D rebuilt", EDIT_TO_3D_REFERENCE_MS, "reference"),
    ("activateMs", "Level switch + 2D refresh", CORE_BUDGET_MS, "core budget"),
    ("activateTo3dMs", "Level switch → 3D", EDIT_TO_3D_REFERENCE_MS, "reference"),
]
SINGLE_LATENCIES = [
    ("openToRefreshMs", "Open + 2D refresh"),
    ("openTo3dMs", "Open → first 3D scene"),
    ("releaseTo3dMs", "Drag release → 3D committed"),
]
# Scenarios that must never upload geometry: they only move or tint.
TRANSFORM_ONLY = {"orbit", "pick", "selection"}


# ---------------------------------------------------------------- environment

def _run(*cmd, timeout=20):
    if not shutil.which(cmd[0]):
        return None
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return out.stdout.strip() if out.returncode == 0 else None


def _sysctl(name):
    return _run("sysctl", "-n", name)


def _macos():
    info = {
        "model": _sysctl("hw.model"),
        "cpu": _sysctl("machdep.cpu.brand_string"),
        "logicalCpus": _sysctl("hw.ncpu"),
        "performanceCores": _sysctl("hw.perflevel0.physicalcpu"),
        "efficiencyCores": _sysctl("hw.perflevel1.physicalcpu"),
        "memoryBytes": int(_sysctl("hw.memsize") or 0) or None,
        "os": f"macOS {_run('sw_vers', '-productVersion')} ({_run('sw_vers', '-buildVersion')})",
        "kernel": platform.release(),
    }
    displays = _run("system_profiler", "SPDisplaysDataType", "-json", timeout=60)
    gpus, screens = [], []
    if displays:
        for gpu in json.loads(displays).get("SPDisplaysDataType", []):
            gpus.append({
                "name": gpu.get("sppci_model") or gpu.get("_name"),
                "cores": gpu.get("sppci_cores"),
                "metal": gpu.get("spdisplays_mtlgpufamilysupport"),
                "vram": gpu.get("spdisplays_vram") or gpu.get("spdisplays_vram_shared"),
            })
            for screen in gpu.get("spdisplays_ndrvs", []):
                screens.append({
                    "name": screen.get("_name"),
                    "resolution": screen.get("_spdisplays_resolution")
                    or screen.get("spdisplays_resolution"),
                    "pixels": screen.get("_spdisplays_pixels"),
                    "main": screen.get("spdisplays_main") == "spdisplays_yes",
                })
    info["gpus"] = gpus
    info["displays"] = screens
    battery = _run("pmset", "-g", "batt") or ""
    info["power"] = {
        "source": "AC" if "AC Power" in battery else "battery" if "Battery Power" in battery else None,
        "battery": (re.search(r"(\d+)%", battery) or [None, None])[1],
        "lowPowerMode": bool(re.search(r"lowpowermode\s+1", _run("pmset", "-g") or "")),
        "thermal": _run("pmset", "-g", "therm"),
    }
    return info


def _read(path):
    try:
        return Path(path).read_text().strip()
    except OSError:
        return None


def _linux():
    cpuinfo = _read("/proc/cpuinfo") or ""
    meminfo = _read("/proc/meminfo") or ""
    release = _read("/etc/os-release") or ""
    model = re.search(r"model name\s*:\s*(.+)", cpuinfo)
    mem = re.search(r"MemTotal:\s*(\d+) kB", meminfo)
    pretty = re.search(r'PRETTY_NAME="?([^"\n]+)', release)
    gpus = []
    for line in (_run("lspci") or "").splitlines():
        if re.search(r"VGA|3D controller|Display controller", line):
            gpus.append({"name": line.split(": ", 1)[-1]})
    nvidia = _run("nvidia-smi", "--query-gpu=name,driver_version,memory.total",
                  "--format=csv,noheader")
    glx = _run("glxinfo", "-B")
    renderer = re.search(r"OpenGL renderer string:\s*(.+)", glx or "")
    screens = [line.strip() for line in (_run("xrandr", "--current") or "").splitlines()
               if "*" in line]
    supplies = Path("/sys/class/power_supply")
    source = None
    battery = None
    if supplies.is_dir():
        for supply in supplies.iterdir():
            kind = _read(supply / "type")
            if kind == "Mains" and _read(supply / "online") is not None:
                source = "AC" if _read(supply / "online") == "1" else "battery"
            if kind == "Battery":
                battery = _read(supply / "capacity")
    return {
        "model": _read("/sys/devices/virtual/dmi/id/product_name"),
        "cpu": model[1] if model else platform.processor(),
        "logicalCpus": os.cpu_count(),
        "memoryBytes": int(mem[1]) * 1024 if mem else None,
        "os": pretty[1] if pretty else platform.platform(),
        "kernel": platform.release(),
        "gpus": gpus,
        "nvidia": nvidia,
        "openglRenderer": renderer[1] if renderer else None,
        "displays": screens,
        "sessionType": os.environ.get("XDG_SESSION_TYPE"),
        "cpuGovernor": _read("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor"),
        "power": {"source": source, "battery": battery},
    }


def environment():
    system = platform.system()
    machine = _macos() if system == "Darwin" else _linux() if system == "Linux" else {}
    flutter = _run("flutter", "--version", "--machine", timeout=120)
    try:
        flutter = json.loads(flutter) if flutter else None
    except json.JSONDecodeError:
        flutter = None
    root = Path(__file__).resolve().parent.parent
    git = lambda *a: _run("git", "-C", str(root), *a)
    dirty = git("status", "--porcelain")
    return {
        "hostname": platform.node(),
        "system": system,
        "architecture": platform.machine(),
        "machine": machine,
        "loadAverage": list(os.getloadavg()) if hasattr(os, "getloadavg") else None,
        "toolchain": {
            "flutter": flutter and flutter.get("frameworkVersion"),
            "flutterRevision": flutter and flutter.get("frameworkRevision"),
            "dart": flutter and flutter.get("dartSdkVersion"),
            "rustc": _run("rustc", "--version"),
            "python": platform.python_version(),
        },
        "source": {
            "commit": git("rev-parse", "HEAD"),
            "describe": git("log", "-1", "--format=%h %s"),
            "uncommittedFiles": len(dirty.splitlines()) if dirty else 0,
        },
    }


# --------------------------------------------------------------------- report

def _ms(v, digits=2):
    if v is None:
        return "–"
    return f"{v:.{digits}f}"


def _bytes(v):
    if not v:
        return "–"
    for unit in ("B", "KiB", "MiB", "GiB"):
        if v < 1024 or unit == "GiB":
            return f"{v:.1f} {unit}" if unit != "B" else f"{v} B"
        v /= 1024


def _verdict(value, reference):
    if value is None or reference is None:
        return "–"
    if value <= reference:
        return "within"
    if value <= reference * 2:
        return "over (≤2×)"
    return "**over (>2×)**"


def _frame_verdict(frames):
    total = frames and frames.get("totalMs")
    if not total:
        return "–"
    p95 = total["p95"]
    if p95 <= FRAME_60HZ_MS:
        return "≤ 16.7"
    if p95 <= FRAME_30HZ_MS:
        return "**> 16.7**"
    return "**> 33.3**"


def _histogram(raw, width=40):
    """Frame totals in ms buckets as a text histogram."""
    edges = [4, 8, 12, 16.7, 25, 33.3, 50, 100, float("inf")]
    labels = ["<4", "4–8", "8–12", "12–16.7", "16.7–25", "25–33.3", "33.3–50", "50–100", "≥100"]
    counts = [0] * len(edges)
    for frame in raw:
        total = frame[3] / 1000
        for i, edge in enumerate(edges):
            if total < edge:
                counts[i] += 1
                break
    peak = max(counts) or 1
    lines = []
    for label, count in zip(labels, counts):
        bar = "█" * max(1 if count else 0, round(count / peak * width))
        lines.append(f"{label:>9} ms │{bar} {count}")
    return "\n".join(lines)


def _summary_row(name, s, reference=None, kind=None):
    missing = (s or {}).get("missing", 0)
    missing_text = f"**{missing}**" if missing else "0"
    if not s or not s.get("count"):
        return f"| {name} | 0 | {missing_text} | – | – | – | – | – | – | – |"
    ref = f"{reference:.1f} ms {kind}" if reference else "–"
    verdict = _verdict(s["p99"], reference) if not missing else "**incomplete**"
    return (f"| {name} | {s['count']} | {missing_text} | {_ms(s['p50'])} | {_ms(s['p90'])} "
            f"| {_ms(s['p95'])} | {_ms(s['p99'])} | {_ms(s['max'])} | {ref} | {verdict} |")


def _env_rows(env):
    m = env.get("machine", {})
    rows = [
        ("Host name", env.get("hostname")),
        ("Machine", m.get("model")),
        ("CPU", m.get("cpu")),
        ("Cores", " · ".join(x for x in [
            f"{m.get('logicalCpus')} logical" if m.get("logicalCpus") else None,
            f"{m.get('performanceCores')} performance" if m.get("performanceCores") else None,
            f"{m.get('efficiencyCores')} efficiency" if m.get("efficiencyCores") else None,
        ] if x)),
        ("Memory", _bytes(m.get("memoryBytes"))),
        ("GPU", "; ".join(
            " ".join(str(x) for x in [g.get("name"),
                                      f"({g['cores']} cores)" if g.get("cores") else None,
                                      g.get("metal")] if x)
            for g in m.get("gpus", [])) or None),
        ("NVIDIA", m.get("nvidia")),
        ("OpenGL renderer", m.get("openglRenderer")),
        ("Displays", "; ".join(
            (d if isinstance(d, str) else
             " ".join(str(x) for x in [d.get("name"), d.get("resolution"),
                                       "(main)" if d.get("main") else None] if x))
            for d in m.get("displays", [])) or None),
        ("Operating system", f"{m.get('os')} · kernel {m.get('kernel')} · {env.get('architecture')}"),
        ("Session", m.get("sessionType")),
        ("CPU governor", m.get("cpuGovernor")),
        ("Power", " · ".join(str(x) for x in [
            (m.get("power") or {}).get("source"),
            f"battery {(m.get('power') or {}).get('battery')}%" if (m.get("power") or {}).get("battery") else None,
            "LOW POWER MODE" if (m.get("power") or {}).get("lowPowerMode") else None,
        ] if x) or None),
        ("Load average at start", " / ".join(f"{v:.2f}" for v in env.get("loadAverage") or []) or None),
    ]
    return [(k, v) for k, v in rows if v]


def report(run_dir):
    run = Path(run_dir)
    results = json.loads((run / "results.json").read_text())
    env_path = run / "environment.json"
    env = json.loads(env_path.read_text()) if env_path.exists() else {}
    scenarios = results["scenarios"]
    workloads = results["workloads"]
    tool = env.get("toolchain", {})
    src = env.get("source", {})
    out = []
    w = out.append

    w("# Domowik application benchmark — CONFIDENTIAL\n")
    w("> **CONFIDENTIAL.** This report identifies a specific machine (host name, "
      "hardware, displays, power state) and its performance. Keep it out of the "
      "repository, issue trackers and public channels; `private/` is gitignored. "
      "Move it between machines by hand.\n")
    w(f"Run `{run.name}` · {results.get('durationSeconds', 0):.0f} s of scenarios · "
      f"commit `{(src.get('commit') or '?')[:12]}`"
      + (f" with **{src['uncommittedFiles']} uncommitted file(s)**" if src.get("uncommittedFiles") else "")
      + "\n")

    # Headline.
    w("## Summary\n")
    w("*Flutter frame* times (ms) are each frame's total span from vsync to raster "
      "finished, inside each scenario; they are not presentation or input-to-photon "
      "times. *Flutter/s* and *3D/s* are frames produced per second by Flutter and by "
      "the native 3D presenter, capped by the display. "
      "*Action* is the synchronous cost of the interaction (core + 2D refresh, or a pick); "
      "*→ 3D* runs from the interaction to the 3D scene it caused. Both are p95 in ms. "
      "Reference lines are explained under *Method*; none is a pass/fail gate.\n")
    w("| Workload | Scenario | Frames | Flutter frame p50 | p95 | p99 | > 16.7 ms | p95 verdict | Flutter/s | 3D/s | Action p95 | → 3D p95 |")
    w("|---|---|---|---|---|---|---|---|---|---|---|---|")
    to_3d = {"selectTo3dMs", "dragInputTo3dMs", "editTo3dMs", "historyTo3dMs", "activateTo3dMs"}
    not_action = to_3d | {"previewLoadMs"}

    def first(keys):
        for key, label, ref, _ in LATENCIES:
            metric = s.get(key)
            if key in keys and isinstance(metric, dict) and (metric.get("count") or metric.get("missing")):
                return metric.get("p95"), ref, metric.get("missing", 0), metric.get("count", 0)
        return None

    for s in scenarios:
        f = s.get("frames") or {}
        total = f.get("totalMs") or {}
        action = first({k for k, *_ in LATENCIES} - not_action)
        three = first(to_3d)
        if three is None and isinstance(s.get("openTo3dMs"), (int, float)):
            three = (s["openTo3dMs"], None, 0, 1)

        def cell(v):
            if v is None:
                return "–"
            value, ref, missing, count = v
            if missing:
                return f"**incomplete {count}/{count + missing}**"
            return _ms(value) + ("" if ref is None or value <= ref else f" (>{ref:.0f})")
        over = f"{f.get('over16', 0)} ({f.get('over16', 0) / f['count'] * 100:.0f}%)" if f.get("count") else "–"
        w(f"| {s['workload']} | {s['name']} | {f.get('count', 0)} | {_ms(total.get('p50'))} "
          f"| {_ms(total.get('p95'))} | {_ms(total.get('p99'))} | {over} | {_frame_verdict(f)} "
          f"| {_ms(f.get('fps'), 0)} | {_ms(s.get('presenterFps'), 0)} | {cell(action)} | {cell(three)} |")
    w("")

    # Findings.
    w("## Findings\n")
    findings = []
    ranked = sorted(
        ((s["workload"], s["name"], label, s[key]["p95"], ref)
         for s in scenarios for key, label, ref, _ in LATENCIES
         if isinstance(s.get(key), dict) and s[key].get("count")),
        key=lambda x: x[3] / x[4], reverse=True)
    for workload, name, label, p95, ref in ranked[:5]:
        if p95 > ref:
            findings.append(f"**{label}** on `{workload}` ({name}): p95 {p95:.1f} ms, "
                            f"{p95 / ref:.1f}× its {ref:.1f} ms reference line.")
    slow_frames = sorted(
        ((s["workload"], s["name"], s["frames"]["totalMs"]["p95"])
         for s in scenarios if (s.get("frames") or {}).get("totalMs")),
        key=lambda x: x[2], reverse=True)
    for workload, name, p95 in slow_frames[:3]:
        if p95 > FRAME_60HZ_MS:
            findings.append(f"Flutter frames during **{name}** on `{workload}` exceed 16.7 ms at p95 ({p95:.1f} ms).")
    for s in scenarios:
        presenter = s.get("presenter") or {}
        if s["name"] in TRANSFORM_ONLY and presenter.get("meshUploads"):
            findings.append(f"`{s['workload']}` {s['name']} uploaded {presenter['meshUploads']} mesh(es); "
                            "it should only move or tint instances.")
        loads = s.get("sceneLoads") or {}
        if s["name"] in TRANSFORM_ONLY | {"drag-furniture", "drag-group"} and (loads.get("full") or {}).get("count") and s["name"] != "open":
            if s["name"] in TRANSFORM_ONLY:
                findings.append(f"`{s['workload']}` {s['name']} ran {loads['full']['count']} full scene build(s).")
        if (s.get("frames") or {}).get("count", 0) == 0:
            findings.append(f"`{s['workload']}` {s['name']} recorded no frames; its frame columns are empty.")
    lagging = []
    for s in scenarios:
        flutter_fps = (s.get("frames") or {}).get("fps")
        presenter_fps = s.get("presenterFps")
        if s["name"] in ("orbit", "drag-furniture", "drag-group") and flutter_fps \
                and presenter_fps is not None and presenter_fps < 0.9 * flutter_fps:
            lagging.append(f"{s['name']} on `{s['workload']}` {presenter_fps:.0f}/{flutter_fps:.0f}")
    if lagging:
        findings.append(
            "The **3D presenter lags Flutter** while the view moves, so fast Flutter frames do not mean "
            "smooth 3D (3D/Flutter frames per second): " + "; ".join(lagging) + ".")
    if (env.get("machine", {}).get("power") or {}).get("source") == "battery":
        findings.append("The machine ran on **battery**; results may be throttled.")
    if (env.get("machine", {}).get("power") or {}).get("lowPowerMode"):
        findings.append("**Low Power Mode** was on; results are throttled.")
    for problem in results.get("problems", []):
        findings.append(f"Problem: {problem}")
    if not findings:
        findings.append("Every measured latency is within its reference line and every scenario's Flutter frames stay within 16.7 ms at p95.")
    for f in findings:
        w(f"- {f}")
    w("")

    # Environment.
    w("## Environment\n")
    w("| Property | Value |")
    w("|---|---|")
    for k, v in _env_rows(env):
        w(f"| {k} | {v} |")
    drivers = sorted({wl.get("driver") for wl in workloads if wl.get("driver")})
    w(f"| 3D presenter | {', '.join(drivers) or '–'} |")
    w(f"| Flutter / Dart | {tool.get('flutter') or '?'} ({(tool.get('flutterRevision') or '')[:10]}) / {tool.get('dart') or '?'} |")
    w(f"| Rust | {tool.get('rustc') or '?'} |")
    w(f"| Source | {src.get('describe') or '?'} |")
    host = results.get("host", {})
    w(f"| App process | {host.get('os')} {host.get('osVersion')} · {host.get('logicalCpus')} CPUs · release build |")
    w(f"| Peak memory (whole run) | {_bytes((results.get('memory') or {}).get('maxRssBytes'))} |")
    w("")

    # Workloads.
    w("## Workloads\n")
    w("| Workload | Levels | Measured level | Walls | Openings | Models | 3D triangles | Draw calls | Models added by the benchmark |")
    w("|---|---|---|---|---|---|---|---|---|")
    for wl in workloads:
        placed = wl.get("furniturePlaced")
        placed_text = f"{placed} in {wl['furniturePlacementSeconds']:.1f} s" if placed else "none"
        w(f"| `{wl['name']}` | {wl.get('levels', '–')} | {wl.get('activeLevel', '–')} | {wl.get('activeLevelWalls', '–')} | {wl.get('activeLevelOpenings', '–')} "
          f"| {wl.get('activeLevelFurniture', '–')} | {wl.get('triangles', '–')} | {wl.get('drawCalls', '–')} | {placed_text} |")
    w("")
    w("| Workload | Split | Plan viewport | 3D viewport | Device pixel ratio |")
    w("|---|---|---|---|---|")
    for wl in workloads:
        layout = wl.get("layout") or {}
        px = lambda v: f"{v[0]:.0f} × {v[1]:.0f}" if v else "–"
        w(f"| `{wl['name']}` | {layout.get('split', '–')} | {px(layout.get('planLogicalPx'))} "
          f"| {px(layout.get('threeDLogicalPx'))} | {layout.get('devicePixelRatio', '–')} |")
    w("\nEach workload is measured on its level with the most walls; walls, openings "
      "and models count that level, triangles and draw calls the whole visible 3D scene. "
      "`four-level-house` is the repository example as it is. `campus-xl` is the "
      "deterministic interaction-benchmark corpus (`domowik-bench --export campus-xl`) "
      "with 200 repository example models added, so drags and picks have content.\n")

    # Method.
    w("## Method\n")
    w("- The **release** desktop editor runs `DOMOWIK_BENCHMARK` (`frontend/lib/app_benchmark.dart`): "
      "scripted interactions through the editor's real handlers and the real GPU presenter, in split view.")
    w("- **Drags** are synthetic mouse events injected through Flutter's gesture binding, one 2 px move per frame.")
    w("- **Flutter frames** come from `FrameTiming` reports: build, raster and the total span from vsync "
      "to raster finished, attributed to a scenario by vsync time on the engine clock. They are not "
      "presentation times; the native 3D presenter renders on its own cadence, reported as *3D/s*. "
      "Idle periods produce no frames.")
    w("- **Scene loads** are timed in the 3D view from request to installed scene; only installed "
      "scenes count. *→ 3D* latencies run from the action to that installation; the presenter draws "
      "on its next frame. An action whose scene never installs within its timeout is counted as "
      "*missing*, makes the metric incomplete and is listed under Findings.")
    w("- **Drags** report *input → 3D preview* per pointer move, to the first installed scene whose pose "
      "revision includes the preview that carried the move (never to a load showing an older preview; "
      "errs toward slower), separately from the preview load's own duration. A drag must run in the "
      "expected mode and its release must move every dragged model and advance the document.")
    w("- **Isolation.** The benchmark neither reads nor writes the user's remembered active level; "
      "each workload's split orientation and viewport sizes are recorded under Workloads.")
    w("- **Percentiles** are nearest-rank over every sample, with no warm-up discarded and no outliers removed.")
    w(f"- **Reference lines.** {CORE_BUDGET_MS:.0f} ms p99 is the project's core gesture budget "
      "(`docs/BENCHMARK-INTERACTION.md`); here it also includes the Dart 2D refresh, so it is stricter than "
      f"the core-only measurement. {FRAME_60HZ_MS:.1f} ms is one 60 Hz frame. "
      f"{EDIT_TO_3D_REFERENCE_MS:.0f} ms for edit→3D is a provisional reference line until issue 0033 "
      "sets the render budget. None of these is a gate.\n")

    # Per scenario.
    w("## Scenarios\n")
    for s in scenarios:
        w(f"### `{s['workload']}` · {s['name']}\n")
        w(f"{s['description']}\n")
        if s.get("error"):
            w(f"> **Error:** {s['error']}\n")
        f = s.get("frames") or {}
        w(f"Wall time {s.get('wallSeconds', 0):.2f} s · {f.get('count', 0)} frames · "
          f"{_ms(f.get('fps'), 1)} frames/s · {f.get('over16', 0)} over 16.7 ms · {f.get('over33', 0)} over 33.3 ms\n")
        if f.get("count"):
            w("| Flutter frame phase | Count | Missing | p50 | p90 | p95 | p99 | Max | Reference | p99 vs reference |")
            w("|---|---|---|---|---|---|---|---|---|---|")
            w(_summary_row("Build (UI thread)", f.get("buildMs"), FRAME_60HZ_MS, "frame"))
            w(_summary_row("Raster (GPU thread)", f.get("rasterMs"), FRAME_60HZ_MS, "frame"))
            w(_summary_row("Total span (vsync → raster finished)", f.get("totalMs"), FRAME_60HZ_MS, "frame"))
            w("")
            w("```text")
            w(_histogram(f.get("raw", [])))
            w("```\n")
        rows = [_summary_row(label, s[key], ref, kind) for key, label, ref, kind in LATENCIES
                if isinstance(s.get(key), dict) and s[key].get("count")]
        if rows:
            w("| Latency (ms) | Count | Missing | p50 | p90 | p95 | p99 | Max | Reference | p99 vs reference |")
            w("|---|---|---|---|---|---|---|---|---|---|")
            out.extend(rows)
            w("")
        singles = [(label, s.get(key)) for key, label in SINGLE_LATENCIES if key in s]
        extras = [(k, s[k]) for k in ("hits", "groupSize", "previewLoads", "dragMode",
                                      "committed", "statusAfterRelease") if k in s]
        if singles or extras:
            w("| Measure | Value |")
            w("|---|---|")
            for label, value in singles:
                w(f"| {label} | {_ms(value)} ms |")
            for k, v in extras:
                w(f"| {k} | {v} |")
            w("")
        loads = s.get("sceneLoads") or {}
        presenter = s.get("presenter") or {}
        w("| 3D activity | Value |")
        w("|---|---|")
        full, fast = loads.get("full") or {}, loads.get("transformOnly") or {}
        w(f"| Full scene loads | {full.get('count', 0)} (p50 {_ms(full.get('p50'))} ms, max {_ms(full.get('max'))} ms) |")
        w(f"| Transform-only loads | {fast.get('count', 0)} (p50 {_ms(fast.get('p50'))} ms, max {_ms(fast.get('max'))} ms) |")
        w(f"| Failed loads | {loads.get('failed', 0)} |")
        w(f"| Presenter frames | {presenter.get('frames', '–')} ({_ms(s.get('presenterFps'), 0)}/s) |")
        w(f"| Camera updates | {presenter.get('cameraUpdates', '–')} |")
        w(f"| Mesh uploads | {presenter.get('meshUploads', '–')} |")
        w(f"| Resident memory after | {_bytes((s.get('memory') or {}).get('rssBytes'))} |")
        w("")

    w("## Reproducing and comparing\n")
    w("```sh\ntask benchmark                                  # build, run, report\n"
      "task benchmark -- --no-build                    # reuse the last release build\n"
      "task benchmark:compare -- RUN_A RUN_B           # side-by-side of two runs\n```\n")
    w(f"Raw measurements: `{run / 'results.json'}`; environment: `{env_path}`; application log: `{run / 'app.log'}`.\n")
    return "\n".join(out)


# -------------------------------------------------------------------- compare

def _key_metrics(results):
    """Comparable p95 values: `{name: (value, note)}`. A note marks a value
    that must not be compared as is: an incomplete distribution, or a
    scenario that failed or reported a problem."""
    flagged = {p.split(":", 1)[0] for p in results.get("problems", [])}
    metrics = {}
    for s in results["scenarios"]:
        prefix = f"{s['workload']} · {s['name']}"
        scenario_note = "error" if s.get("error") else (
            "problem" if f"{s['workload']}/{s['name']}" in flagged else None)
        total = (s.get("frames") or {}).get("totalMs")
        if total:
            metrics[f"{prefix} · Flutter frame p95"] = (total["p95"], scenario_note)
        for key, label, _, _ in LATENCIES:
            metric = s.get(key)
            if isinstance(metric, dict) and (metric.get("count") or metric.get("missing")):
                missing = metric.get("missing", 0)
                note = f"incomplete {metric.get('count', 0)}/{metric.get('count', 0) + missing}" \
                    if missing else scenario_note
                metrics[f"{prefix} · {label} p95"] = (metric.get("p95"), note)
        for key, label in SINGLE_LATENCIES:
            if isinstance(s.get(key), (int, float)):
                metrics[f"{prefix} · {label}"] = (s[key], scenario_note)
    return metrics


def compare(run_a, run_b):
    runs = []
    for run in (Path(run_a), Path(run_b)):
        results = json.loads((run / "results.json").read_text())
        env_path = run / "environment.json"
        env = json.loads(env_path.read_text()) if env_path.exists() else {}
        runs.append((run, results, env))
    (a, ra, ea), (b, rb, eb) = runs
    ma, mb = _key_metrics(ra), _key_metrics(rb)
    out = ["# Domowik benchmark comparison — CONFIDENTIAL\n",
           "> **CONFIDENTIAL.** Identifies the machines compared. Keep it in `private/`.\n",
           "| | A | B |", "|---|---|---|"]
    for label, fn in [
        ("Run", lambda r, e, p: f"`{p.name}`"),
        ("Machine", lambda r, e, p: f"{e.get('machine', {}).get('model') or '?'} · {e.get('machine', {}).get('cpu') or '?'}"),
        ("GPU", lambda r, e, p: "; ".join(g.get("name") or "?" for g in e.get("machine", {}).get("gpus", [])) or "?"),
        ("Commit", lambda r, e, p: f"`{(e.get('source', {}).get('commit') or '?')[:12]}`"),
        ("Flutter", lambda r, e, p: e.get("toolchain", {}).get("flutter") or "?"),
        ("Power", lambda r, e, p: (e.get("machine", {}).get("power") or {}).get("source") or "?"),
    ]:
        out.append(f"| {label} | {fn(ra, ea, a)} | {fn(rb, eb, b)} |")
    out += ["", "Values in ms; *B / A* above 1 means B is slower. No ratio is given for a "
            "value that is incomplete or comes from a scenario with an error or problem.\n",
            "| Metric | A | B | B / A |", "|---|---|---|---|"]

    def cell(v):
        if v is None:
            return "–"
        value, note = v
        return f"{_ms(value)} (**{note}**)" if note else _ms(value)

    for key in sorted(set(ma) | set(mb)):
        va, vb = ma.get(key), mb.get(key)
        usable = va and vb and not va[1] and not vb[1] and va[0] and vb[0]
        ratio = f"{vb[0] / va[0]:.2f}×" if usable else "–"
        if usable and vb[0] / va[0] >= 1.5:
            ratio = f"**{ratio}**"
        out.append(f"| {key} | {cell(va)} | {cell(vb)} | {ratio} |")
    for label, results in (("A", ra), ("B", rb)):
        for problem in results.get("problems", []):
            out.append(f"\n- {label} problem: {problem}")
    return "\n".join(out) + "\n"


# ------------------------------------------------------------- public summary

# Scenarios that must only move or tint instances, and the drag modes expected.
_DRAG_MODES = {"drag-furniture": "asset", "drag-group": "group"}


def _scrubber(env):
    """Removes the host name and filesystem paths from free text."""
    host = (env.get("hostname") or "").strip()
    short = host.split(".")[0]

    def scrub(text):
        text = re.sub(r"(?:/Users|/home|/private|/tmp|/var|/Volumes)/\S+", "<path>", str(text))
        text = re.sub(r"[A-Za-z]:\\\S+", "<path>", text)
        for name in {host, short} - {""}:
            text = text.replace(name, "<host>")
        return text
    return scrub


def public_summary(run_dir):
    """Pass/fail health of a run, safe to post publicly.

    Built from an allow-list only: OS family, architecture, Flutter version,
    commit, 3D graphics API, and per-scenario counts and checks. No timings,
    host name, hardware model, displays, power state or paths; problem and
    error text is scrubbed of the host name and paths."""
    run = Path(run_dir)
    results = json.loads((run / "results.json").read_text())
    env_path = run / "environment.json"
    env = json.loads(env_path.read_text()) if env_path.exists() else {}
    scrub = _scrubber(env)
    apis = sorted({m[1] for w in results["workloads"]
                   if (m := re.search(r"\(([^)]+)\)\s*$", str(w.get("driver") or "")))})
    commit = ((env.get("source") or {}).get("commit") or "?")[:12]
    uncommitted = (env.get("source") or {}).get("uncommittedFiles")
    checks = []

    def check(ok, text):
        checks.append(("ok  " if ok else "FAIL") + "  " + text)

    rows = []
    for s in results["scenarios"]:
        where = f"{s['workload']} {s['name']}"
        frames = (s.get("frames") or {}).get("count")
        presenter = s.get("presenter") or {}
        frames_3d, uploads = presenter.get("frames"), presenter.get("meshUploads")
        missing = sum(v.get("missing", 0) for v in s.values() if isinstance(v, dict))
        if s.get("error"):
            check(False, f"{where}: error {scrub(s['error'])}")
        check(bool(frames), f"{where}: Flutter frames recorded")
        if missing:
            check(False, f"{where}: {missing} action(s) never produced their 3D result")
        if s["name"] in ("orbit", "drag-furniture", "drag-group") and frames_3d is not None:
            check(frames_3d > 0, f"{where}: 3D presenter produced frames")
        if s["name"] in TRANSFORM_ONLY and uploads is not None:
            check(uploads == 0, f"{where}: no mesh uploads")
        if s["name"] in _DRAG_MODES:
            check(s.get("dragMode") == _DRAG_MODES[s["name"]]
                  and s.get("committed") is True
                  and (s.get("previewLoads") or 0) >= 60,
                  f"{where}: {_DRAG_MODES[s['name']]} drag previewed and committed")
        extra = ", ".join(f"{k} {s[k]}" for k in ("dragMode", "committed", "previewLoads", "hits")
                          if k in s)
        show = lambda v: "n/a" if v is None else v
        rows.append(f"| {s['workload']} | {s['name']} | {show(frames)} | {show(frames_3d)} "
                    f"| {show(uploads)} | {missing} | {extra} |")
    failed = sum(1 for c in checks if c.startswith("FAIL"))
    out = [
        "### Domowik benchmark health (public summary)\n",
        f"`{env.get('system') or results['host'].get('os')} {env.get('architecture') or ''}`".rstrip()
        + f" · Flutter {(env.get('toolchain') or {}).get('flutter') or '?'}"
        + f" · commit `{commit}`" + (f" + {uncommitted} uncommitted file(s)" if uncommitted else "")
        + f" · 3D: {', '.join(apis) or 'n/a'}\n",
        f"**{'All checks pass' if not failed else f'{failed} check(s) failed'}** "
        f"({len(checks)} checks; timings are deliberately omitted).\n",
        "```text", *[c for c in checks if c.startswith("FAIL")],
        *[c for c in checks if not c.startswith("FAIL")], "```\n",
        "| Workload | Scenario | Flutter frames | 3D frames | Mesh uploads | Missing results | Drag / pick |",
        "|---|---|---|---|---|---|---|", *rows, "",
        "n/a: not reported by this platform's 3D presenter.",
    ]
    problems = [scrub(p) for p in results.get("problems", [])]
    out.append("Problems: " + ("; ".join(problems) if problems else "none") + "\n")
    return "\n".join(out)


def _latest_run():
    runs = sorted(p for p in (Path(__file__).resolve().parent.parent / "private" / "benchmarks").glob("2*")
                  if (p / "results.json").exists())
    if not runs:
        raise SystemExit("no benchmark run with results.json under private/benchmarks/")
    return runs[-1]


def main(argv):
    if len(argv) >= 1 and argv[0] == "environment":
        print(json.dumps(environment(), indent=1))
    elif argv and argv[0] in ("public-summary", "--public-summary") and len(argv) <= 2:
        print(public_summary(argv[1] if len(argv) == 2 else _latest_run()))
    elif len(argv) == 2 and argv[0] == "report":
        print(report(argv[1]))
    elif len(argv) == 3 and argv[0] == "compare":
        text = compare(argv[1], argv[2])
        folder = Path(__file__).resolve().parent.parent / "private" / "benchmarks"
        folder.mkdir(parents=True, exist_ok=True, mode=0o700)
        stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        out = folder / f"compare-{stamp}.md"
        fd = os.open(out, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(fd, "w") as f:
            f.write(text)
        print(f"CONFIDENTIAL comparison at {out}")
    else:
        print(__doc__, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
