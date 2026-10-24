# Application benchmark

`task benchmark` measures the release desktop editor as a person uses it:
frame times, interaction latency, 3D scene work and memory while plans are
opened, navigated and edited. It complements the core
[interaction benchmark](BENCHMARK-INTERACTION.md), which times document
gestures without the editor, and the CPU harnesses in `frontend/benchmark/`.

Its reports describe one machine in detail and are **confidential**: they
are written to the gitignored `private/benchmarks/` tree with owner-only
permissions and are never committed or published. This document describes
the method; it records no results.

## Running it

```sh
task benchmark                     # build the release app, run, report
task benchmark -- --no-build       # reuse the last release build
task benchmark:compare -- private/benchmarks/RUN_A private/benchmarks/RUN_B
task benchmark:summary             # health checks safe to post publicly
```

Each run creates `private/benchmarks/<UTC stamp>-<host>/` with `REPORT.md`,
the raw `results.json`, `environment.json` (hardware, displays, power and
toolchain) and the application log. To evaluate on another machine, check out
the same commit there, run `task benchmark`, and carry the run directory back
by hand; `benchmark:compare` writes a side-by-side table with B/A ratios.

`task benchmark:summary` (or `scripts/benchmark_report.py public-summary
[RUN]`, the latest run by default) prints whether a run works without saying
how fast it was or what ran it: OS family, architecture, Flutter version,
commit and 3D graphics API, then pass/fail checks and per-scenario counts.
It checks that every scenario recorded frames and every action produced its
3D result, that the 3D presenter drew while the view moved, that orbit, pick
and selection uploaded no meshes, that drags ran in the expected mode,
previewed and committed, and that the run recorded no problems. It is built from an allow-list, contains no
timings, host name, hardware, displays or power state, and scrubs host names
and paths from problem text. Use it for issues and pull requests.

The run takes about a minute plus the build, opens the editor window and
drives it; leave it visible and the machine otherwise idle. Plug laptops in:
the report notes battery power and macOS Low Power Mode because both
throttle. On macOS the sandboxed app reads its inputs from, and writes its
results to, its own container; the script stages them there and moves the
results out.

Supported hosts are macOS and Linux desktops. On Linux with `nvidia-smi`, the
script uses the same PRIME offload settings as `task run:gui:linux` unless
they are already set. The browser build is not covered.

## Workloads

| Workload | Content |
|---|---|
| `four-level-house` | The repository example, unchanged. |
| `campus-xl` | The deterministic stress corpus from the interaction benchmark (`cargo run -p domowik-bench -- --export campus-xl PATH`), with 200 repository example models added to the measured level. |

Each workload is measured on its level with the most walls.

## Scenarios

| Scenario | What it does | Main measurements |
|---|---|---|
| `open` | Opens the plan in split view. | Open + 2D refresh; open → first complete 3D scene. |
| `orbit` | One camera orbit step per frame for 240 frames. | Frame times; presenter frames; mesh uploads (should be zero). |
| `pick` | 200 3D picks on a grid across the viewport. | Pick latency. |
| `selection` | Selects 40 walls and models in turn. | Selection → 3D tint (transform-only scene update). |
| `drag-furniture` | A synthetic mouse drag of one model for 120 frames, then release. | Drag input → 3D preview; preview load duration; release → committed 3D scene. |
| `drag-group` | The same with up to 20 selected models. | As above. |
| `wall-edit` | 20 wall edits: height changes and 100 mm moves. | Commit + 2D refresh; edit → rebuilt 3D scene. |
| `undo-redo` | Undoes then redoes 10 steps. | History step + 2D refresh; step → rebuilt 3D scene. |
| `switch-level` | Activates each level in turn, three rounds. | Activation + 2D refresh; activation → 3D. |

A scenario cannot pass by measuring the wrong thing. A drag must run in the
expected mode, produce at least ten 3D previews (a slow host coalesces moves
into fewer previews, which shows in the latencies, not as a failure), and on
release move every dragged model and advance the document; otherwise it is reported as a
problem with the editor's drag mode and status message. An interaction whose
3D scene never installs within its timeout is counted as *missing*: the
metric is marked incomplete instead of losing its slowest samples.

## Method

- The release editor runs with `DOMOWIK_BENCHMARK=<results.json>`
  (`frontend/lib/app_benchmark.dart`). Interactions go through the editor's
  own handlers and the real GPU presenter; drags are mouse events injected
  through Flutter's gesture binding, one 2 px move per frame.
- Flutter frame times are `FrameTiming` reports (build, raster, and the total
  span from vsync to raster finished), attributed to a scenario by their
  vsync time on the engine clock. They are not presentation times: the
  native 3D presenter renders on its own cadence, reported separately as its
  frames per second.
- Scene loads are timed inside the 3D view from request to installed scene
  (`ThreeDViewState.loadSamples`, recorded only during a benchmark); failed
  loads do not count. A "→ 3D" latency runs from the action to the
  installed scene it caused. *Drag input → 3D preview* runs from each pointer
  move to the first installed scene whose pose revision includes the preview
  that carried it; a load that started later but still showed an older
  preview is not credited. A preview that reaches the core after a load
  started is credited to a later scene, so the figure errs toward slower.
- The benchmark neither reads nor writes the user's remembered active level,
  and records the split orientation and viewport sizes it ran with.
- Percentiles are nearest-rank over every sample, as in the interaction
  benchmark. No warm-up is discarded and no outlier removed.
- Reference lines in the report: the core's 8 ms p99 gesture budget (applied
  here to core plus 2D refresh, so stricter), one 60 Hz frame (16.7 ms), and
  a provisional 100 ms for edit → 3D until issue 0033 sets the render budget.
  None of them is a gate, and `scripts/verify.sh` does not run this benchmark.

## Limits

Numbers are specific to the host, its display refresh rate, power state and
background load; compare runs from one machine, or treat cross-machine ratios
as indicative. The Linux path is implemented but has not yet been
run; expect to fix details the first time. Synthetic input skips the
operating system's event delivery,
so input-to-photon latency is not measured. Presenter counters differ between
the Linux OpenGL and the `wgpu` presenters; the report shows what each
provides.
