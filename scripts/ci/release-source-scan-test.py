#!/usr/bin/env python3
"""Exercise release scanners against actual index/worktree differences."""

import importlib.util
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
SCRIPTS = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import releaselib as rl  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "release_validate", SCRIPTS / "release-validate.py"
)
validator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validator)


class WorktreeScanTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="domowik-source-scan-")
        self.addCleanup(self.scratch.cleanup)
        self.root = pathlib.Path(self.scratch.name)
        self.git("init", "-q")

    def git(self, *args):
        subprocess.run(["git", *args], cwd=self.root, check=True, capture_output=True)

    def write(self, relative, text, tracked=True):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        if tracked:
            self.git("add", "--", relative)
        return path

    def test_live_sources_are_scanned_but_untracked_files_are_not(self):
        self.write("fixture/src/lib.rs", 'include_bytes!("../assets/model.bin");\n')
        self.write("fixture/untracked.rs", 'include_bytes!("secret.bin");\n', False)
        self.write("fixture/pubspec.yaml", "fonts:\n  - asset: assets/font.ttf\n")
        self.write(
            "fixture/stray/pubspec.yaml", "fonts:\n  - asset: stray.ttf\n", False
        )
        self.assertEqual(
            [row["path"] for row in rl.scan_embeds(str(self.root), "fixture")],
            ["fixture/assets/model.bin"],
        )
        self.assertEqual(
            [row["path"] for row in rl.scan_pubspec_fonts(str(self.root), "fixture")],
            ["fixture/assets/font.ttf"],
        )

    def test_unstaged_rust_deletion_is_not_a_source_input(self):
        self.write("fixture/src/removed.rs", 'include_bytes!("old.bin");\n').unlink()
        self.assertEqual(rl.scan_embeds(str(self.root), "fixture"), [])

    def test_read_errors_for_live_sources_are_not_silenced(self):
        self.write("fixture/src/lib.rs", 'include_bytes!("model.bin");\n')
        with mock.patch("builtins.open", side_effect=PermissionError("unreadable")):
            with self.assertRaises(PermissionError):
                rl.scan_embeds(str(self.root), "fixture")

    def test_unstaged_pubspec_deletion_is_not_a_font_input(self):
        self.write(
            "fixture/removed app/pubspec.yaml", "fonts:\n  - asset: old.ttf\n"
        ).unlink()
        self.assertEqual(rl.scan_pubspec_fonts(str(self.root), "fixture"), [])

    def test_index_removal_excludes_the_remaining_untracked_source(self):
        self.write("fixture/src/removed.rs", 'include_bytes!("old.bin");\n')
        self.git("rm", "--cached", "--", "fixture/src/removed.rs")
        self.assertEqual(rl.scan_embeds(str(self.root), "fixture"), [])

    def test_removing_an_embedded_asset_still_refuses_validation(self):
        self.write("fixture/src/lib.rs", 'include_bytes!("../assets/model.bin");\n')
        self.write("fixture/assets/model.bin", "synthetic asset").unlink()
        report = validator.Report()
        validator.check_embeds(
            str(self.root),
            {
                "embedScanSubtrees": ["fixture"],
                "artifacts": [],
                "inputs": {"assetsRecord": "assets.json"},
            },
            {"assets": [{"path": "fixture/assets/model.bin", "status": "distributed"}]},
            report,
        )
        self.assertEqual([tag for tag, _ in report.failures], ["stale-asset-row"])

    def test_removing_the_embedding_source_still_refuses_a_stale_record(self):
        self.write(
            "fixture/src/lib.rs", 'include_bytes!("../assets/model.bin");\n'
        ).unlink()
        self.write("fixture/assets/model.bin", "synthetic asset")
        report = validator.Report()
        validator.check_embeds(
            str(self.root),
            {
                "embedScanSubtrees": ["fixture"],
                "artifacts": [],
                "inputs": {"assetsRecord": "assets.json"},
            },
            {
                "assets": [
                    {"path": "fixture/assets/model.bin", "status": "distributed"}
                ],
            },
            report,
        )
        self.assertEqual([tag for tag, _ in report.failures], ["stale-asset-row"])


if __name__ == "__main__":
    unittest.main()
