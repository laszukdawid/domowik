#!/usr/bin/env python3
"""Generate a deterministic CycloneDX SBOM for the Rust source tree or one artifact.

Issue 0036. Replaces the frozen Go generator (`cmd/domowik-sbom`, kept at
`_reference/go/`), which described two linux binaries built from a package that
no longer exists.

    release-sbom.py source [--target TRIPLE]
    release-sbom.py artifact --id ID [--target TRIPLE] [--binary PATH]
                             [--bundle PATH] [--release-grade]

WHAT THIS IS HONEST ABOUT, because the last generator's problem was not its
code but the claims layered on top of it:

- `cargo metadata` IS NOT A GUI SBOM. For the Flutter bundle it covers the
  Rust half and the Dart lockfile, and it says in the document itself which
  components it could not see without a built bundle. `--release-grade`
  turns that admission into a non-zero exit rather than a footnote.
- A SOURCE SBOM IS NOT PROOF OF A BINARY'S CONTENTS (issue 0036 criterion 6).
  Artifact documents are target-filtered and carry the measured binary's own
  digest; source documents carry neither and say so.
- Proc-macro and build-dependency crates are marked `excluded` scope. Their
  code runs in the compiler and is not shipped; only their output is. Calling
  `syn` a distributed component would put it in a notice bundle it has no
  claim on.
- The standard library is a DISTRIBUTED component and cargo cannot see it. It
  is statically linked into every binary and comes from the pinned toolchain
  record, not from the graph.

Deterministic by construction: no wall clock, sorted everywhere, and a
serialNumber derived from the document's own content. Two runs over one tree
are byte-identical, which is what `cmp` in the test script asserts.

First-party, Apache-2.0, stdlib-only, not distributed with Domowik.
"""

from __future__ import annotations

import argparse
import os
import re
import sys

# No .pyc beside the sources. `releaselib` is the first module this repo
# imports rather than runs, and verify.sh's final stage asserts a clean
# tree -- a __pycache__ directory would fail a run for a cache nobody
# asked for.
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import releaselib as rl  # noqa: E402

GENERATOR = "domowik-release-sbom"
GENERATOR_VERSION = "0.1.0"
MANIFEST = "release/rust-manifest.json"
SPEC_VERSION = "1.6"

# Legacy slash spellings are not valid SPDX expressions. They are normalised
# for the `expression` field and the crate's own declared string is kept in a
# property, so nothing is silently rewritten.
SLASH = re.compile(r"^([^/]+)/([^/]+)$")


def licence_block(declared: str | None) -> tuple[list, str]:
    raw = declared or ""
    if not raw:
        return ([{"license": {"name": "NOT DECLARED"}}], raw)
    match = SLASH.match(raw.strip())
    expression = "%s OR %s" % match.groups() if match else raw
    return ([{"expression": expression}], raw)


def crate_component(pkg: dict, scope: str, role: str) -> dict:
    purl = "pkg:cargo/%s@%s" % (pkg["name"], pkg["version"])
    licences, raw = licence_block(pkg.get("license"))
    component = {
        "type": "library",
        "bom-ref": purl,
        "name": pkg["name"],
        "version": pkg["version"],
        "purl": purl,
        "scope": scope,
        "licenses": licences,
    }
    refs = []
    if pkg.get("repository"):
        refs.append({"type": "vcs", "url": pkg["repository"]})
    if refs:
        component["externalReferences"] = refs
    props = [{"name": "domowik:role", "value": role}]
    if raw:
        props.append({"name": "domowik:declaredLicense", "value": raw})
    if pkg.get("source"):
        props.append({"name": "domowik:source", "value": pkg["source"]})
    component["properties"] = props
    return component


def stdlib_components(toolchain: dict, target: str) -> list[dict]:
    """The standard library, which cargo metadata does not report.

    It is statically linked into every binary produced here. Leaving it out
    would make the document claim a component set it does not have, which is
    the specific failure issue 0036 criterion 2 names.
    """
    version = toolchain["version"]
    purl = "pkg:generic/rust-std@%s" % version
    rows = [a for a in toolchain["artifacts"]
            if a["component"] == "rust-std" and a["target"] == target]
    component = {
        "type": "library",
        "bom-ref": purl,
        "name": "rust-std",
        "version": version,
        "purl": purl,
        "scope": "required",
        "licenses": [{"expression": "MIT OR Apache-2.0"}],
        "externalReferences": [{"type": "website", "url": toolchain["license"]["url"]}],
        "properties": [
            {"name": "domowik:role",
             "value": "rust standard library, statically linked"},
            {"name": "domowik:rustcVersion", "value": toolchain["rustcVersionString"]},
            {"name": "domowik:gitCommitHash", "value": toolchain["gitCommitHash"]},
            {"name": "domowik:aggregateNoticeGap",
             "value": "the upstream COPYRIGHT file covering vendored components "
                      "such as compiler-builtins (Apache-2.0 WITH LLVM-exception) "
                      "is not committed to this repository; see the toolchain record"},
        ],
    }
    if rows:
        component["hashes"] = [{"alg": "SHA-256", "content": rows[0]["sha256"]}]
        component["externalReferences"].append(
            {"type": "distribution", "url": rows[0]["url"]})
    else:
        component["properties"].append(
            {"name": "domowik:toolchainArtifactMissing",
             "value": "no rust-std row for target %s in the toolchain record" % target})
    return [component]


def asset_components(root: str, assets: dict, subtree: str,
                     manifest: dict, artifact: dict | None) -> list[dict]:
    # A file a build script copies into ONE artifact (copiedAssets in the
    # manifest, such as the browser editor's Roboto) belongs to that artifact
    # alone, even though it sits under other artifacts' embed subtrees. With
    # no artifact (the source SBOM) every distributed asset is listed.
    copied_by = {c["path"]: a["id"] for a in manifest["artifacts"]
                 for c in a.get("copiedAssets", [])}
    out = []
    for asset in assets.get("assets", []):
        if asset.get("status") != "distributed":
            continue
        path = asset.get("path", "")
        owner = copied_by.get(path)
        if owner is not None and artifact is not None and owner != artifact["id"]:
            continue
        if owner is None and subtree and not (
                path == subtree or path.startswith(subtree + "/")):
            continue
        ref = "asset:%s" % path
        component = {
            "type": "file",
            "bom-ref": ref,
            "name": asset["name"],
            "version": asset.get("version", ""),
            "scope": "required",
            "licenses": [{"license": {"id": asset["license"]}}]
            if asset.get("license") else [{"license": {"name": "NOT DECLARED"}}],
            "properties": [
                {"name": "domowik:role",
                 "value": "bundled non-code asset, copied into the bundle"
                 if owner is not None else "bundled non-code asset, embedded"},
                {"name": "domowik:path", "value": path},
                {"name": "domowik:modified",
                 "value": "yes" if asset.get("modified") else "no"},
            ],
        }
        if asset.get("sha256"):
            component["hashes"] = [{"alg": "SHA-256", "content": asset["sha256"]}]
        if asset.get("sourceUrl"):
            component["externalReferences"] = [
                {"type": "distribution", "url": asset["sourceUrl"]}]
        out.append(component)
    return out


def parse_pubspec_lock(path: str) -> list[dict]:
    """A pubspec.lock reader for the subset pub actually emits.

    Deliberately not a YAML library: adding a third-party parser to read one
    machine-generated file with a fixed two-level shape would need its own
    provenance record under docs/THIRD-PARTY-POLICY.md, which is a worse trade
    than forty lines of indentation handling.
    """
    packages: list[dict] = []
    section = name = None
    current: dict = {}
    in_description = False
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            indent = len(line) - len(line.lstrip())
            text = line.strip()
            if indent == 0:
                if current:
                    packages.append(current)
                    current = {}
                section = text.rstrip(":")
                name = None
                continue
            if section != "packages":
                continue
            if indent == 2:
                if current:
                    packages.append(current)
                name = text.rstrip(":")
                current = {"name": name}
                in_description = False
            elif indent == 4:
                if text.rstrip(":") == "description":
                    in_description = True
                    continue
                in_description = False
                key, _, value = text.partition(":")
                current[key.strip()] = value.strip().strip('"')
            elif indent >= 6 and in_description:
                key, _, value = text.partition(":")
                current["description." + key.strip()] = value.strip().strip('"')
    if current:
        packages.append(current)
    return sorted(packages, key=lambda p: p["name"])


def dart_components(lock_path: str) -> list[dict]:
    out = []
    for pkg in parse_pubspec_lock(lock_path):
        version = pkg.get("version", "")
        source = pkg.get("source", "unknown")
        purl = "pkg:pub/%s@%s" % (pkg["name"], version)
        component = {
            "type": "library",
            "bom-ref": purl,
            "name": pkg["name"],
            "version": version,
            "purl": purl,
            # A dev/test package resolved into the lock is not shipped; the
            # lock alone cannot tell us which, so the honest scope is the
            # dependency class pub recorded.
            "scope": "required" if pkg.get("dependency", "").startswith("direct")
            else "optional",
            "licenses": [{"license": {"name": "NOT DECLARED IN LOCKFILE"}}],
            "properties": [
                {"name": "domowik:role", "value": "dart package (%s)" % source},
                {"name": "domowik:dependencyClass",
                 "value": pkg.get("dependency", "unknown")},
                {"name": "domowik:licenceGap",
                 "value": "pubspec.lock records no licence; resolving it needs the "
                          "package archive, which this generator does not fetch"},
            ],
        }
        if pkg.get("description.sha256"):
            component["hashes"] = [
                {"alg": "SHA-256", "content": pkg["description.sha256"]}]
        if pkg.get("description.url"):
            component["externalReferences"] = [
                {"type": "distribution", "url": pkg["description.url"]}]
        out.append(component)
    return out


def build_document(root: str, subject: dict, components: list[dict],
                   notes: list[str], epoch: int) -> dict:
    components = sorted(components, key=lambda c: (c["type"], c["name"],
                                                   c.get("version", "")))
    payload = {
        "bomFormat": "CycloneDX",
        "specVersion": SPEC_VERSION,
        "version": 1,
        "metadata": {
            "timestamp": rl.iso_utc(epoch),
            "tools": {"components": [{
                "type": "application",
                "name": GENERATOR,
                "version": GENERATOR_VERSION,
                "author": "Domowik Contributors",
            }]},
            "component": subject,
            "properties": [{"name": "domowik:note", "value": n} for n in notes],
        },
        "components": components,
    }
    document = {"$schema": "http://cyclonedx.org/schema/bom-1.6.schema.json",
                "serialNumber": rl.serial_number(payload)}
    document.update(payload)
    return document


def head_commit(root: str) -> str:
    return rl.run(["git", "rev-parse", "HEAD"], cwd=root).strip()


def find_artifact(manifest: dict, artifact_id: str) -> dict:
    for artifact in manifest["artifacts"]:
        if artifact["id"] == artifact_id:
            return artifact
    rl.die("release-sbom: no artifact %r in %s (have: %s)"
           % (artifact_id, MANIFEST,
              ", ".join(a["id"] for a in manifest["artifacts"])))


def graph_components(root: str, workspace: str, root_package: str,
                     target: str | None) -> list[dict]:
    meta = rl.cargo_metadata(os.path.join(root, workspace), target)
    by_id = {p["id"]: p for p in meta["packages"]}
    roots = {pid for pid, pkg in by_id.items() if pkg["name"] == root_package}
    if not roots:
        rl.die("release-sbom: package %r not in the %s graph"
               % (root_package, workspace))
    linked, build_only = rl.closure(meta, roots)
    workspace_ids = set(meta["workspace_members"])

    components = []
    for pid in sorted(linked):
        pkg = by_id[pid]
        first_party = pid in workspace_ids or pkg.get("source") is None
        components.append(crate_component(
            pkg, "required",
            "first-party crate, linked" if first_party else "crate, linked"))
    for pid in sorted(build_only):
        components.append(crate_component(
            by_id[pid], "excluded",
            "build-time only (proc-macro or build dependency); its code runs "
            "in the compiler and is not shipped"))
    return components


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    src = sub.add_parser("source")
    src.add_argument("--target")
    art = sub.add_parser("artifact")
    art.add_argument("--id", required=True)
    art.add_argument("--target")
    art.add_argument("--binary")
    art.add_argument("--bundle")
    art.add_argument("--release-grade", action="store_true")
    args = parser.parse_args()

    root = rl.repo_root()
    epoch = rl.source_date_epoch(root)
    manifest = rl.load_json(root, MANIFEST)
    toolchain = rl.load_json(root, manifest["inputs"]["toolchainRecord"])
    assets = rl.load_json(root, manifest["inputs"]["assetsRecord"])
    commit = head_commit(root)

    if args.mode == "source":
        target = args.target
        components = graph_components(root, ".", "domowik-cli", target)
        seen = {c["bom-ref"] for c in components}
        for workspace in sorted({a["workspaceRoot"] for a in manifest["artifacts"]}):
            if workspace == ".":
                continue
            for component in graph_components(
                    root, workspace,
                    next(a["rootPackage"] for a in manifest["artifacts"]
                         if a["workspaceRoot"] == workspace), target):
                if component["bom-ref"] not in seen:
                    seen.add(component["bom-ref"])
                    components.append(component)
        components += asset_components(root, assets, "", manifest, None)
        files = []
        for rel in sorted(rl.run(["git", "ls-files"], cwd=root).splitlines()):
            full = os.path.join(root, rel)
            if not os.path.isfile(full) or os.path.islink(full):
                continue
            files.append({
                "type": "file",
                "bom-ref": "file:%s" % rel,
                "name": rel,
                "scope": "required",
                "hashes": [{"alg": "SHA-256", "content": rl.sha256_file(full)}],
                "properties": [{"name": "domowik:role", "value": "tracked source file"}],
            })
        components += files
        subject = {
            "type": "application",
            "bom-ref": "domowik-source@%s" % commit,
            "name": "domowik",
            "version": "0.1.0",
            "licenses": [{"license": {"id": "Apache-2.0"}}],
            "properties": [{"name": "domowik:commit", "value": commit}],
        }
        notes = [
            "SOURCE TREE DOCUMENT. It describes what is in the repository at "
            "commit %s, NOT what is inside any binary. Issue 0036 criterion 6 "
            "forbids treating this as proof of a cross-built artifact's "
            "contents; use the per-artifact document for that." % commit,
            "Target filter: %s." % (args.target or
                                    "none (every platform's crates are listed, "
                                    "including ones never compiled here)"),
            "%d tracked files are listed with their digests." % len(files),
        ]
        print(rl.canonical_json(build_document(root, subject, components, notes,
                                              epoch)), end="")
        return 0

    artifact = find_artifact(manifest, args.id)
    target = args.target or rl.run(["rustc", "-vV"], cwd=root)
    if not args.target:
        target = next(line.split(": ", 1)[1].strip()
                      for line in target.splitlines() if line.startswith("host:"))
    if target not in artifact["targets"]:
        rl.die("release-sbom: %s is not a declared target of artifact %r "
               "(declared: %s). Add it to %s and review the toolchain record "
               "first."
               % (target, args.id, ", ".join(artifact["targets"]), MANIFEST))

    notes: list[str] = []
    incomplete: list[str] = []
    components = graph_components(root, artifact["workspaceRoot"],
                                  artifact["rootPackage"], target)
    components += stdlib_components(toolchain, target)
    components += asset_components(root, assets, artifact.get("embedSubtree", ""),
                                   manifest, artifact)

    if artifact["kind"] == "flutter-bundle":
        lock = os.path.join(root, artifact["dartLock"])
        if not os.path.isfile(lock):
            rl.die("release-sbom: dart lockfile missing: %s" % artifact["dartLock"])
        dart = dart_components(lock)
        components += dart
        notes.append(
            "%d Dart packages resolved from %s. The lockfile records NO "
            "licence for any of them; resolving those needs the package "
            "archives, which this generator does not fetch."
            % (len(dart), artifact["dartLock"]))
        bundle = args.bundle or os.path.join(root, artifact["bundlePath"])
        if os.path.isdir(bundle):
            scan = artifact["bundleScan"]
            fw_dir = os.path.join(bundle, scan["frameworks"])
            names = sorted(os.listdir(fw_dir)) if os.path.isdir(fw_dir) else []
            for name in names:
                ref = "framework:%s" % name
                components.append({
                    "type": "framework",
                    "bom-ref": ref,
                    "name": name,
                    "version": "",
                    "scope": "required",
                    "licenses": [{"license": {"name": "NOT RESOLVED BY SCAN"}}],
                    "properties": [
                        {"name": "domowik:role",
                         "value": "framework embedded in the built macOS bundle"},
                        {"name": "domowik:bundlePath",
                         "value": os.path.join(scan["frameworks"], name)},
                    ],
                })
            assets_dir = os.path.join(bundle, scan["flutterAssets"])
            shipped = []
            for dirpath, _, filenames in os.walk(assets_dir):
                for filename in filenames:
                    full = os.path.join(dirpath, filename)
                    shipped.append(os.path.relpath(full, assets_dir))
            for rel in sorted(shipped):
                full = os.path.join(assets_dir, rel)
                components.append({
                    "type": "file",
                    "bom-ref": "flutter-asset:%s" % rel,
                    "name": rel,
                    "scope": "required",
                    "licenses": [{"license": {"name": "NOT RESOLVED BY SCAN"}}],
                    "hashes": [{"alg": "SHA-256", "content": rl.sha256_file(full)}],
                    "properties": [{"name": "domowik:role",
                                    "value": "file shipped in flutter_assets"}],
                })
            notes.append(
                "Bundle scanned at %s: %d frameworks, %d flutter_assets files. "
                "The scan records WHAT is shipped and its digest; it does not "
                "resolve licences, which is the next staged step."
                % (os.path.relpath(bundle, root), len(names), len(shipped)))
        else:
            incomplete += artifact["coverage"]["requiresBuiltBundle"]
            notes.append(
                "NO BUILT BUNDLE at %s, so this document is INCOMPLETE. Missing: "
                "%s. Build the bundle and rerun with --bundle, or treat this "
                "document as an inventory of the Rust and Dart halves only."
                % (artifact["bundlePath"], "; ".join(incomplete)))

    subject = {
        "type": "application",
        "bom-ref": "%s@%s" % (args.id, target),
        "name": artifact.get("binaryName", args.id),
        "version": "0.1.0",
        "licenses": [{"license": {"id": "Apache-2.0"}}],
        "properties": [
            {"name": "domowik:commit", "value": commit},
            {"name": "domowik:target", "value": target},
            {"name": "domowik:artifactId", "value": args.id},
            {"name": "domowik:artifactKind", "value": artifact["kind"]},
        ],
    }

    binary = args.binary
    if binary is None and artifact["kind"] == "rust-binary":
        out = rl.target_dir(root)
        candidate = os.path.join(out, "release", artifact["binaryName"])
        cross = os.path.join(out, target, "release", artifact["binaryName"])
        binary = cross if os.path.isfile(cross) else candidate
    if binary and os.path.isfile(binary):
        subject["hashes"] = [{"alg": "SHA-256", "content": rl.sha256_file(binary)}]
        subject["properties"] += [
            {"name": "domowik:binaryPath", "value": os.path.relpath(binary, root)},
            {"name": "domowik:binaryBytes", "value": str(os.path.getsize(binary))},
        ]
        notes.append(
            "The subject digest above was MEASURED from the built artifact, so "
            "this document describes that exact file.")
    else:
        incomplete.append("the artifact was not built, so no measured digest")
        notes.append(
            "NO MEASURED BINARY. Without a digest this document describes an "
            "intended component set, not a produced artifact.")

    notes.insert(0,
                 "Target-filtered to %s. Crates that other platforms need are "
                 "absent by design; the workspace-wide list in "
                 "docs/RUST-DEPENDENCIES.md is deliberately broader because it "
                 "guards licences rather than describing a binary." % target)
    if incomplete:
        subject.setdefault("properties", []).append(
            {"name": "domowik:complete", "value": "no"})
    else:
        subject["properties"].append({"name": "domowik:complete", "value": "yes"})

    print(rl.canonical_json(build_document(root, subject, components, notes, epoch)),
          end="")

    if incomplete and args.release_grade:
        rl.die("release-sbom: --release-grade refused for %r: %s"
               % (args.id, "; ".join(incomplete)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
