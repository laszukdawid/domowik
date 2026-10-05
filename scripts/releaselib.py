#!/usr/bin/env python3
"""Shared pieces of the Rust release inventory: graph, digests, determinism.

Issue 0036. The Go pipeline this replaces lived in `internal/release` and is
frozen at `_reference/go/release`; `docs/RELEASE-INVENTORY.md` keeps its design
as the argument a Rust replacement has to answer. This module exists so the
SBOM generator and the release validator cannot drift apart in how they read
the dependency graph or decide what is distributed -- they were the two halves
of one Go package and splitting them into two scripts without a shared core is
how the answers start disagreeing.

It is first-party, Apache-2.0, stdlib-only, and not distributed with Domowik.
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import re
import subprocess
import sys

# The one reviewed licence allow-list, imported rather than copied.
#
# `scripts/rust-deps-inventory.py` owns it: verify.sh has regenerated and
# diffed its output since ED-0091, so it is the list that has actually been
# enforced. A second copy here would be a second thing to review, and the
# copy that drifts is always the one nobody is diffing.
_INVENTORY = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                          "rust-deps-inventory.py")


def _inventory_module():
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("_rust_deps_inventory", _INVENTORY)
    if spec is None or spec.loader is None:
        raise SystemExit("releaselib: cannot load %s" % _INVENTORY)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def allowed_licences() -> set[str]:
    """The reviewed list itself, for reporting how many entries there are."""
    return set(_inventory_module().ALLOWED)


def licence_allowed(declared: str) -> bool:
    """The SAME verdict the dependency inventory reaches, not a reimplementation.

    The inventory compares by licence SET rather than by spelling, so two
    expressions offering the same alternatives in a different order agree. A
    validator doing its own `in ALLOWED` test would refuse crates the inventory
    admits, and the two would disagree about whether the tree is shippable.
    """
    return _inventory_module().is_allowed(declared)


def repo_root() -> str:
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def run(argv: list[str], cwd: str | None = None) -> str:
    proc = subprocess.run(argv, capture_output=True, text=True, cwd=cwd)
    if proc.returncode != 0:
        raise SystemExit(
            "releaselib: %s failed (exit %d)\n%s"
            % (" ".join(argv), proc.returncode, proc.stderr.strip())
        )
    return proc.stdout


def cargo_metadata(root: str, target: str | None = None) -> dict:
    """The resolved graph, optionally narrowed to one target triple.

    `--filter-platform` matters and is not a tidy-up: unfiltered metadata
    reports `r-efi` (UEFI) and `linux-raw-sys` on a macOS build, where neither
    is ever compiled. Over-reporting is the right default for the licence
    GUARD, which is why docs/RUST-DEPENDENCIES.md keeps it; it is the wrong
    default for a document that claims to say what is IN a binary.
    """
    argv = ["cargo", "metadata", "--format-version", "1", "--locked"]
    if target:
        argv += ["--filter-platform", target]
    return json.loads(run(argv, cwd=root))


def target_dir(root: str) -> str:
    """Where cargo actually puts build output.

    `target/` under the workspace is only the DEFAULT. CARGO_TARGET_DIR moves
    it, and CI and parallel worktrees both do move it; a validator that looked
    only in the default place would report a binary missing that had just been
    built, which is a false refusal and erodes the gate it is meant to be.
    """
    override = os.environ.get("CARGO_TARGET_DIR")
    if override:
        return override if os.path.isabs(override) else os.path.join(root, override)
    return os.path.join(root, "target")


def source_date_epoch(root: str) -> int:
    """Unix seconds, from the environment or HEAD. Never the wall clock.

    A generator that reads the clock emits a different document every run and
    cannot be diffed, which removes the only mechanism that has held for the
    dependency inventory.
    """
    raw = os.environ.get("SOURCE_DATE_EPOCH")
    if raw is not None:
        if not re.fullmatch(r"[0-9]+", raw):
            raise SystemExit(
                "releaselib: SOURCE_DATE_EPOCH must be non-negative unix "
                "seconds, got %r" % raw
            )
        return int(raw)
    return int(run(["git", "log", "-1", "--format=%ct", "HEAD"], cwd=root).strip())


def iso_utc(epoch: int) -> str:
    import datetime

    return (
        datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc)
        .strftime("%Y-%m-%dT%H:%M:%SZ")
    )


def sha256_file(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_json(obj) -> str:
    """One byte-for-byte spelling, so two runs can be compared with `cmp`."""
    return json.dumps(obj, indent=2, ensure_ascii=False, sort_keys=False) + "\n"


def serial_number(payload) -> str:
    """A UUID derived from the document's own content, not from randomness.

    CycloneDX wants a serialNumber per document. `uuid4()` would make every
    run differ, which defeats the diff. This hashes the payload and stamps the
    RFC 4122 version/variant bits, so the same inputs always produce the same
    urn and different inputs almost never do.
    """
    raw = hashlib.sha256(canonical_json(payload).encode("utf-8")).digest()
    b = bytearray(raw[:16])
    b[6] = (b[6] & 0x0F) | 0x80  # version 8: name-based, custom construction
    b[8] = (b[8] & 0x3F) | 0x80  # RFC 4122 variant
    h = b.hex()
    return "urn:uuid:%s-%s-%s-%s-%s" % (h[:8], h[8:12], h[12:16], h[16:20], h[20:])


# ===== the dependency graph =====

def _is_proc_macro(pkg: dict) -> bool:
    return any("proc-macro" in t.get("kind", []) for t in pkg.get("targets", []))


def closure(meta: dict, roots: set[str]) -> tuple[set[str], set[str]]:
    """(linked, build_only) package ids reachable from `roots`.

    `linked` follows NORMAL edges only: those crates' code is compiled into
    the artifact. `build_only` is everything reached through a build-dependency
    edge plus every proc-macro crate, whose code runs INSIDE the compiler and
    is never shipped -- only its output is. Classifying a proc-macro as linked
    would put `syn` in a notice bundle it has no claim on.

    Dev edges are followed by neither: test and benchmark harnesses are not
    distributed (ED-0094).
    """
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    by_id = {p["id"]: p for p in meta["packages"]}

    linked: set[str] = set()
    build_only: set[str] = set()

    def walk(start: set[str], into: set[str], normal_only: bool) -> None:
        stack = list(start)
        while stack:
            current = stack.pop()
            for dep in nodes.get(current, {}).get("deps", []):
                kinds = {k.get("kind") for k in dep.get("dep_kinds", [])}
                if kinds == {"dev"}:
                    continue
                if normal_only and None not in kinds:
                    continue
                if dep["pkg"] in into:
                    continue
                into.add(dep["pkg"])
                stack.append(dep["pkg"])

    walk(roots, linked, normal_only=True)
    # Build edges out of anything reachable, plus the roots themselves.
    walk(roots | linked, build_only, normal_only=False)

    # A proc-macro is build-time wherever it was reached from.
    for pid in list(linked):
        if _is_proc_macro(by_id[pid]):
            linked.discard(pid)
            build_only.add(pid)

    build_only -= linked
    build_only -= roots
    return linked, build_only


# ===== embedded assets =====

EMBED_RE = re.compile(r'include_(bytes|str)\s*!\s*\(\s*"([^"]+)"\s*\)')


def scan_embeds(root: str, subtree: str) -> list[dict]:
    """Files compiled into the artifact by `include_bytes!`/`include_str!`.

    THE LIMIT, STATED. This is a regex over tracked `.rs` sources. The Go
    pipeline asked the compiler (`go list -json -deps`) and got an exact
    answer; Rust offers no equivalent short of a compiler plugin, so a path
    assembled with `concat!` or a macro is invisible here. It catches the
    literal form, which is the form every embed in this tree uses, and it is
    a guard where there was previously nothing at all -- `docs/third-party/
    assets.json` is hand-maintained, so before this an undeclared embed
    changed no generated file and failed no stage.
    """
    tracked = run(["git", "ls-files", "--", subtree], cwd=root).splitlines()
    found = []
    for rel in sorted(tracked):
        if not rel.endswith(".rs"):
            continue
        with open(os.path.join(root, rel), encoding="utf-8", errors="replace") as fh:
            for lineno, line in enumerate(fh, 1):
                for match in EMBED_RE.finditer(line):
                    target = os.path.normpath(
                        os.path.join(os.path.dirname(rel), match.group(2))
                    )
                    found.append({"path": target, "source": rel, "line": lineno,
                                  "macro": "include_%s!" % match.group(1)})
    return sorted(found, key=lambda e: (e["path"], e["source"], e["line"]))


def load_json(root: str, relpath: str) -> dict:
    path = os.path.join(root, relpath)
    if not os.path.isfile(path):
        raise SystemExit("releaselib: required input missing: %s" % relpath)
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def die(message: str) -> None:
    sys.stderr.write(message.rstrip() + "\n")
    raise SystemExit(1)
