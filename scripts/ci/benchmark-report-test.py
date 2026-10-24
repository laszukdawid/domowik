#!/usr/bin/env python3
"""Regression tests for scripts/benchmark_report.py on synthetic results."""

import json
import pathlib
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import benchmark_report as br  # noqa: E402


def latency(values, missing=0):
    ordered = sorted(values)
    pick = lambda p: ordered[max(0, -(-p * len(ordered) // 100) - 1)] if ordered else None
    return {"count": len(values), "missing": missing, "p50": pick(50), "p90": pick(90),
            "p95": pick(95), "p99": pick(99), "max": ordered[-1] if ordered else None,
            "raw": values}


def scenario(name, workload="plan", **fields):
    frames = {"count": 60, "fps": 60, "over16": 0, "over33": 0,
              "totalMs": latency([5.0] * 60), "raw": [[0, 1, 1, 5000]] * 60}
    return {"workload": workload, "name": name, "description": name, "wallSeconds": 1.0,
            "frames": frames, "presenter": {}, "sceneLoads": {}, "memory": {}, **fields}


def results(scenarios, problems=()):
    return {"scenarios": scenarios, "workloads": [{"name": "plan"}], "problems": list(problems),
            "host": {}, "memory": {}, "durationSeconds": 1}


class ReportTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="domowik-bench-report-")
        self.addCleanup(self.scratch.cleanup)

    def run_dir(self, name, data):
        path = pathlib.Path(self.scratch.name) / name
        path.mkdir()
        (path / "results.json").write_text(json.dumps(data))
        return path

    def test_an_incomplete_metric_is_never_shown_as_a_plain_number(self):
        run = self.run_dir("a", results([scenario("wall-edit", editTo3dMs=latency([1.0], missing=19),
                                                  commitAndRefreshMs=latency([2.0] * 20))]))
        text = br.report(run)
        self.assertIn("incomplete 1/20", text)
        self.assertIn("**incomplete**", text)

    def test_comparisons_drop_ratios_for_incomplete_or_flagged_values(self):
        a = self.run_dir("a", results([
            scenario("wall-edit", editTo3dMs=latency([10.0] * 20)),
            scenario("pick", pickMs=latency([1.0] * 10)),
            scenario("selection", selectTo3dMs=latency([4.0] * 10)),
        ]))
        b = self.run_dir("b", results([
            scenario("wall-edit", editTo3dMs=latency([1.0], missing=19)),
            scenario("pick", pickMs=latency([2.0] * 10)),
            scenario("selection", selectTo3dMs=latency([4.0] * 10)),
        ], problems=["plan/selection: release did not commit"]))
        rows = {line.split(" | ")[0].lstrip("| "): line
                for line in br.compare(a, b).splitlines() if line.startswith("| plan")}
        self.assertTrue(rows["plan · wall-edit · Edit → 3D rebuilt p95"].endswith("| – |"))
        self.assertIn("incomplete 1/20", rows["plan · wall-edit · Edit → 3D rebuilt p95"])
        self.assertTrue(rows["plan · selection · Selection → 3D tint p95"].endswith("| – |"))
        self.assertIn("**2.00×**", rows["plan · pick · 3D pick p95"])

    def test_a_stalled_presenter_is_a_finding_not_an_all_clear(self):
        run = self.run_dir("a", results([scenario("orbit", presenterFps=0.0)]))
        text = br.report(run)
        self.assertIn("3D presenter lags Flutter", text)
        self.assertIn("orbit on `plan` 0/60", text)
        self.assertNotIn("Every measured latency is within", text)

    def test_reports_render_without_an_environment_file(self):
        run = self.run_dir("a", results([scenario("pick", pickMs=latency([0.1] * 5))]))
        self.assertIn("CONFIDENTIAL", br.report(run))


class PublicSummaryTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="domowik-bench-public-")
        self.addCleanup(self.scratch.cleanup)

    def run_dir(self, data, env):
        path = pathlib.Path(self.scratch.name) / "run"
        path.mkdir()
        (path / "results.json").write_text(json.dumps(data))
        (path / "environment.json").write_text(json.dumps(env))
        return path

    def env(self):
        return {"hostname": "secret-host.local", "system": "Linux", "architecture": "x86_64",
                "machine": {"cpu": "Secret CPU 9000", "gpus": [{"name": "Secret GPU"}],
                            "power": {"source": "battery"}},
                "toolchain": {"flutter": "3.47.7"}, "source": {"commit": "abcdef1234567890"}}

    def test_it_leaks_no_host_hardware_paths_or_timings(self):
        data = results(
            [scenario("orbit", presenter={"frames": 50, "meshUploads": 0},
                      error="crashed reading /home/someone/plan.domo on secret-host",
                      pickMs=latency([123.456]))],
            problems=["plan/orbit: failed at /Users/someone/x on secret-host.local"])
        data["workloads"] = [{"name": "plan", "driver": "Secret GPU (OpenGL)"}]
        text = br.public_summary(self.run_dir(data, self.env()))
        for secret in ("secret-host", "someone", "Secret CPU", "Secret GPU", "battery", "123.456"):
            self.assertNotIn(secret, text)
        self.assertIn("<path>", text)
        self.assertIn("<host>", text)
        self.assertIn("`Linux x86_64`", text)
        self.assertIn("3D: OpenGL", text)

    def test_broken_runs_fail_the_matching_checks(self):
        data = results([
            scenario("drag-furniture", dragMode="body", committed=False, previewLoads=1,
                     presenter={"frames": 0}, dragInputTo3dMs=latency([5.0], missing=3)),
            scenario("selection", presenter={"meshUploads": 7}),
        ])
        text = br.public_summary(self.run_dir(data, self.env()))
        self.assertIn("FAIL  plan drag-furniture: asset drag previewed and committed", text)
        self.assertIn("FAIL  plan drag-furniture: 3D presenter produced frames", text)
        self.assertIn("FAIL  plan drag-furniture: 3 action(s) never produced their 3D result", text)
        self.assertIn("FAIL  plan selection: no mesh uploads", text)
        self.assertIn("4 check(s) failed", text)

    def test_a_clean_run_passes_and_unreported_counters_are_not_failures(self):
        data = results([scenario("orbit", presenter={}),
                        scenario("pick", presenter={"meshUploads": 0}, hits=3)])
        text = br.public_summary(self.run_dir(data, self.env()))
        self.assertIn("All checks pass", text)
        self.assertIn("| plan | orbit | 60 | n/a | n/a | 0 |", text)


if __name__ == "__main__":
    unittest.main()
