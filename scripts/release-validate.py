#!/usr/bin/env python3
"""Fail-closed release validation for the Rust artifacts. Issue 0036.

    release-validate.py [--artifact ID] [--target TRIPLE] [--require-built]
                        [--skip-generated]

Every check below REFUSES rather than warns, because the value of the Go
pipeline this replaces was never the documents it emitted -- it was that a
release could not proceed past a component nobody had reviewed. A validator
that prints a warning and exits 0 is a report, not a gate.

WHAT IT REFUSES:

  empty-artifact-set    the manifest declares nothing to ship
  undeclared-embed      a file is compiled in by include_bytes!/include_str!,
                        or declared as a font in a tracked pubspec.yaml, with
                        no row in the assets record (an artifact's
                        copiedAssets count as found, and their copying script
                        must name them)
  stale-asset-row       the assets record declares a distributed file that no
                        source embeds, or whose digest no longer matches
  unreviewed-licence    a linked crate's declared licence is not on the
                        reviewed allow-list
  undeclared-licence    a linked crate declares no licence at all
  toolchain-drift       the installed rustc, rust-toolchain.toml and the
                        pinned toolchain record disagree
  target-gap            a declared target has no rust-std row in the record
  notice-stale          NOTICE does not match what its generator produces now
  inventory-stale       docs/RUST-DEPENDENCIES.md likewise
  missing-artifact      --require-built was asked for and the binary is absent
  incomplete-coverage   --require-built on an artifact whose inventory is
                        staged rather than complete (the Flutter bundle)

--skip-generated drops the notice-stale and inventory-stale checks, and exists
for ONE caller: scripts/verify.sh, which regenerates and diffs both files two
stages earlier. Repeating it there costs two `cargo metadata` runs and a full
pass over every runtime crate's licence files, to reprove what the same run has
just proved. It is wrong anywhere else. Standalone, the default is the point: a
release validator that trusted files it had not regenerated would pass a tree
whose notice went stale after the last full run, which is the failure ED-0094
built the diff to prevent. The flag announces what it gave up in the script's
own output, so a log cannot hide which checks actually ran.

WHAT IT DOES NOT DO, said plainly: it is not a licence audit. It reads what
crates DECLARE, exactly as docs/RUST-DEPENDENCIES.md does. It cannot detect a
crate whose declared licence is wrong, and it resolves no licence for Dart
packages or for frameworks inside a built bundle -- those are named gaps with
an owner, not silent passes.

First-party, Apache-2.0, stdlib-only, not distributed with Domowik.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys

# No .pyc beside the sources. `releaselib` is the first module this repo
# imports rather than runs, and verify.sh's final stage asserts a clean
# tree -- a __pycache__ directory would fail a run for a cache nobody
# asked for.
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import releaselib as rl  # noqa: E402

MANIFEST = "release/rust-manifest.json"
VALIDATOR_VERSION = "0.1.0"


class Report:
    def __init__(self) -> None:
        self.failures: list[tuple[str, str]] = []
        self.checks: list[str] = []

    def refuse(self, tag: str, detail: str) -> None:
        self.failures.append((tag, detail))

    def passed(self, what: str) -> None:
        self.checks.append(what)


def check_embeds(root: str, manifest: dict, assets: dict, report: Report) -> None:
    declared = {a["path"]: a for a in assets.get("assets", [])
                if a.get("status") == "distributed"}
    found: dict[str, dict] = {}
    for subtree in manifest["embedScanSubtrees"]:
        for embed in rl.scan_embeds(root, subtree) + rl.scan_pubspec_fonts(root, subtree):
            found.setdefault(embed["path"], embed)
    # A file a build script copies into an artifact (the browser editor's
    # Roboto, ADR 0012 section 6) is declared on that artifact, and the named
    # script must still name the file, relative to the artifact's workspace.
    for artifact in manifest["artifacts"]:
        for copied in artifact.get("copiedAssets", []):
            script = copied["copiedBy"]
            rel = os.path.relpath(copied["path"], artifact["workspaceRoot"])
            full = os.path.join(root, script)
            text = open(full, encoding="utf-8").read() if os.path.isfile(full) else ""
            if rel not in text:
                report.refuse(
                    "stale-asset-row",
                    "%s declares %s as copied by %s, which does not name %s"
                    % (MANIFEST, copied["path"], script, rel))
                continue
            found.setdefault(copied["path"], {
                "path": copied["path"], "source": script, "line": 0,
                "macro": "copy"})

    # An embed of a file that lives in the same crate's own source tree is
    # first-party and needs no third-party row; only files under an assets
    # directory are third-party candidates. Judged by path rather than by
    # licence, because guessing at provenance is what the record exists to stop.
    for path, embed in sorted(found.items()):
        if "/assets/" not in path:
            continue
        if path not in declared:
            report.refuse(
                "undeclared-embed",
                "%s is compiled in by %s at %s:%d but has no row in %s"
                % (path, embed["macro"], embed["source"], embed["line"],
                   manifest["inputs"]["assetsRecord"]))
    for path, asset in sorted(declared.items()):
        full = os.path.join(root, path)
        if not os.path.isfile(full):
            report.refuse("stale-asset-row",
                          "%s declares %s, which is not in the tree"
                          % (manifest["inputs"]["assetsRecord"], path))
            continue
        if path not in found:
            report.refuse(
                "stale-asset-row",
                "%s declares %s as distributed, but no tracked source embeds it"
                % (manifest["inputs"]["assetsRecord"], path))
        actual = rl.sha256_file(full)
        if asset.get("sha256") and actual != asset["sha256"]:
            report.refuse("stale-asset-row",
                          "%s digest is %s, recorded as %s"
                          % (path, actual, asset["sha256"]))
        text = asset.get("licenseTextPath")
        if text and not os.path.isfile(os.path.join(root, text)):
            report.refuse("stale-asset-row",
                          "%s names licence text %s, which is not in the tree"
                          % (path, text))
    report.passed("embedded assets: %d found, %d declared distributed"
                  % (len(found), len(declared)))


def check_licences(root: str, artifact: dict, target: str, report: Report) -> None:
    meta = rl.cargo_metadata(os.path.join(root, artifact["workspaceRoot"]), target)
    by_id = {p["id"]: p for p in meta["packages"]}
    roots = {pid for pid, pkg in by_id.items()
             if pkg["name"] == artifact["rootPackage"]}
    if not roots:
        report.refuse("missing-artifact",
                      "package %r is not in the %s graph"
                      % (artifact["rootPackage"], artifact["workspaceRoot"]))
        return
    linked, _ = rl.closure(meta, roots)
    workspace = set(meta["workspace_members"])
    count = 0
    for pid in sorted(linked):
        pkg = by_id[pid]
        if pid in workspace or pkg.get("source") is None:
            continue
        count += 1
        declared = pkg.get("license")
        if not declared:
            report.refuse("undeclared-licence",
                          "%s %s declares no licence in its manifest"
                          % (pkg["name"], pkg["version"]))
        elif not rl.licence_allowed(declared):
            report.refuse(
                "unreviewed-licence",
                "%s %s declares %r, which is not on the reviewed allow-list in "
                "scripts/rust-deps-inventory.py; it needs a decision record and "
                "review per docs/THIRD-PARTY-POLICY.md before it ships"
                % (pkg["name"], pkg["version"], declared))
    report.passed("%s on %s: %d linked third-party crates, all licences reviewed"
                  % (artifact["id"], target, count))


def check_toolchain(root: str, toolchain: dict, manifest: dict,
                    report: Report) -> None:
    try:
        installed = rl.run(["rustc", "-vV"], cwd=root)
    except SystemExit:
        report.refuse("toolchain-drift", "rustc is not on PATH")
        return
    version = next((l.split("rustc ", 1)[1].strip()
                    for l in installed.splitlines() if l.startswith("rustc ")), "")
    if version != toolchain["rustcVersionString"]:
        report.refuse("toolchain-drift",
                      "installed rustc is %r, the record pins %r"
                      % (version, toolchain["rustcVersionString"]))
    pinned = ""
    with open(os.path.join(root, "rust-toolchain.toml"), encoding="utf-8") as fh:
        for line in fh:
            if line.strip().startswith("channel"):
                pinned = line.split("=", 1)[1].strip().strip('"')
    if pinned != toolchain["channelPin"]:
        report.refuse("toolchain-drift",
                      "rust-toolchain.toml pins channel %r, the record pins %r"
                      % (pinned, toolchain["channelPin"]))
    have = {a["target"] for a in toolchain["artifacts"]
            if a["component"] == "rust-std"}
    declared: set[str] = set()
    for artifact in manifest["artifacts"]:
        declared |= set(artifact["targets"])
    for target in sorted(declared - have):
        report.refuse("target-gap",
                      "artifact target %s has no rust-std row in %s, so the "
                      "standard library shipped for it is unrecorded"
                      % (target, manifest["inputs"]["toolchainRecord"]))
    report.passed("toolchain: rustc %s, %d rust-std targets recorded"
                  % (version, len(have)))


def check_generated(root: str, manifest: dict, report: Report) -> None:
    """NOTICE and the dependency inventory must still match their generators.

    This duplicates two verify.sh stages ON PURPOSE and only here: a release
    validator that trusted files it did not regenerate would pass on a tree
    whose notice had gone stale since the last full run, which is the exact
    failure ED-0094 built the diff to prevent.
    """
    for script, committed, tag in (
            ("scripts/rust-notice.py", manifest["inputs"]["notice"], "notice-stale"),
            ("scripts/rust-deps-inventory.py",
             manifest["inputs"]["dependencyInventory"], "inventory-stale")):
        proc = subprocess.run([sys.executable, script], cwd=root,
                              capture_output=True, text=True)
        if proc.returncode != 0:
            report.refuse(tag, "%s exited %d: %s"
                          % (script, proc.returncode, proc.stderr.strip()[:400]))
            continue
        with open(os.path.join(root, committed), encoding="utf-8") as fh:
            if fh.read() != proc.stdout:
                report.refuse(tag, "%s no longer matches %s; regenerate it"
                              % (committed, script))
            else:
                report.passed("%s matches %s" % (committed, script))


def check_built(root: str, artifact: dict, target: str, report: Report) -> None:
    if artifact["kind"] == "rust-binary":
        out = rl.target_dir(root)
        cross = os.path.join(out, target, "release", artifact["binaryName"])
        host = os.path.join(out, "release", artifact["binaryName"])
        binary = cross if os.path.isfile(cross) else host
        if not os.path.isfile(binary):
            report.refuse("missing-artifact",
                          "%s declares status %r but no binary exists at %s or %s"
                          % (artifact["id"], artifact["status"],
                             os.path.relpath(cross, root), os.path.relpath(host, root)))
            return
        report.passed("%s built: %s (%d bytes, sha256 %s)"
                      % (artifact["id"], os.path.relpath(binary, root),
                         os.path.getsize(binary), rl.sha256_file(binary)))
        return
    gaps = artifact.get("coverage", {}).get("requiresBuiltBundle", [])
    bundle = os.path.join(root, artifact.get("bundlePath", ""))
    if not os.path.isdir(bundle):
        report.refuse(
            "incomplete-coverage",
            "%s is %r: its inventory covers the Rust and Dart halves, and these "
            "remain uncovered without a built bundle: %s. Building it is issue "
            "0024's; this refuses rather than reporting the artifact as clean."
            % (artifact["id"], artifact["status"], "; ".join(gaps)))
        return
    report.passed("%s bundle present at %s"
                  % (artifact["id"], os.path.relpath(bundle, root)))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact")
    parser.add_argument("--target")
    parser.add_argument("--require-built", action="store_true")
    parser.add_argument(
        "--skip-generated", action="store_true",
        help="skip the NOTICE and dependency-inventory regeneration checks; "
             "only for scripts/verify.sh, which has just done both")
    args = parser.parse_args()

    root = rl.repo_root()
    manifest = rl.load_json(root, MANIFEST)
    toolchain = rl.load_json(root, manifest["inputs"]["toolchainRecord"])
    assets = rl.load_json(root, manifest["inputs"]["assetsRecord"])
    report = Report()

    artifacts = manifest["artifacts"]
    if not artifacts:
        report.refuse("empty-artifact-set",
                      "%s declares no artifacts. An empty set is not a release "
                      "with nothing to check; it is a manifest that has lost "
                      "its subject." % MANIFEST)
    if args.artifact:
        artifacts = [a for a in artifacts if a["id"] == args.artifact]
        if not artifacts:
            rl.die("release-validate: no artifact %r in %s"
                   % (args.artifact, MANIFEST))

    host = next(line.split(": ", 1)[1].strip()
                for line in rl.run(["rustc", "-vV"], cwd=root).splitlines()
                if line.startswith("host:"))

    check_embeds(root, manifest, assets, report)
    check_toolchain(root, toolchain, manifest, report)
    if args.skip_generated:
        report.passed(
            "NOTICE and dependency-inventory checks SKIPPED (--skip-generated); "
            "the caller is responsible for having regenerated and diffed both")
    else:
        check_generated(root, manifest, report)
    for artifact in artifacts:
        target = args.target or (host if host in artifact["targets"]
                                 else artifact["targets"][0])
        check_licences(root, artifact, target, report)
        if args.require_built:
            check_built(root, artifact, target, report)

    print("domowik-release-validate %s" % VALIDATOR_VERSION)
    print("manifest: %s (%d artifact(s))" % (MANIFEST, len(manifest["artifacts"])))
    print()
    for line in report.checks:
        print("  ok      %s" % line)
    if report.failures:
        print()
        for tag, detail in report.failures:
            print("  REFUSED %s: %s" % (tag, detail))
        print()
        print("RELEASE VALIDATION FAILED: %d refusal(s)" % len(report.failures))
        return 1
    print()
    print("RELEASE VALIDATION PASSED")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
