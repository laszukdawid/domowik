#!/usr/bin/env python3
"""Emit a deterministic third-party dependency inventory for the Rust workspace.

docs/THIRD-PARTY-POLICY.md requires every dependency to be licence-compatible
and recorded BEFORE adoption. The Go release-inventory verifier enforced that
for the Go module; it was frozen with the Go core, so this keeps the obligation
enforced for the Rust workspace in the interim.

Fail-closed by construction: `scripts/verify.sh` regenerates this and diffs it
against the committed copy, so ADDING A DEPENDENCY FAILS THE BUILD until it is
recorded here and its licence reviewed.

Scope, stated honestly: this reports what crates DECLARE in their manifests. It
is not a licence audit, does not read LICENSE files, does not detect a crate
whose declared licence is wrong, and does not build NOTICE. Those remain owed
by the Rust release process that replaces cmd/domowik-sbom.

It also does NOT filter by target platform. `r-efi` (UEFI) and `linux-raw-sys`
are listed as runtime dependencies even on a macOS build, where they are never
compiled. Over-reporting is the safe direction for a licence guard -- it can
raise a crate that is not shipped, but it cannot miss one that is.

AND IT DOES NOT SEE SPIKE WORKSPACES. `cargo metadata` runs below with no
--manifest-path, so it resolves the ROOT workspace only. Spike crates are their
own workspaces, excluded from the root one by ED-0097 so a spike cannot join the
distributed build graph by accident, and that exclusion hides them from this
guard too.

Today that costs nothing and the claim is checkable: every one of the 54
third-party crates in the issue-0024 bridge spike's closure is already in this
inventory, because the spike depends only on the core crates plus serde,
serde_json and uuid, which the root workspace already pulls. The set difference
is empty. It stops costing nothing the moment a spike adds a dependency the
root does not have, and a spike is exactly where a dependency gets normalised
before anyone reviews it (raised by the issue-0025 worker, 2026-09-12).

What covers it: `scripts/release-validate.py` audits licences PER ARTIFACT, and
`release/rust-manifest.json` declares the Flutter bundle with the spike as its
workspace root, so the spike's linked closure is audited there under the same
allow-list. That validator is not yet a verify.sh stage, which is the gap worth
knowing about. Extending THIS file to spikes was considered and rejected: its
two kinds are `runtime` (distributed) and `dev` (test and benchmark tooling),
and a spike crate is neither, so it would need a third classification and would
blur the one distinction ED-0097 exists to keep sharp.
"""

import json
import subprocess
import sys

# Apache-2.0 compatible, permissive. A crate whose declared expression is not
# on this list fails the run rather than being waved through: copyleft or
# proprietary requires an explicit decision record and review per the policy.
# Each entry was reviewed against docs/THIRD-PARTY-POLICY.md, which prefers
# permissive licences and requires an explicit decision record for copyleft or
# proprietary components.
#
# Every expression below is either a single permissive licence or a DISJUNCTION
# ("OR") that offers Apache-2.0 or MIT among its alternatives. A disjunction
# lets the distributor choose one branch, so a copyleft alternative appearing
# inside an OR does not impose copyleft — `r-efi`'s
# "MIT OR Apache-2.0 OR LGPL-2.1-or-later" is taken under MIT/Apache-2.0, and
# the LGPL branch is simply not exercised. An expression joining terms with
# "AND" would be different and is not admitted here except for the reviewed
# Unicode-3.0 data licence, which imposes attribution only.
ALLOWED = {
    "MIT OR Apache-2.0",
    "Apache-2.0 OR MIT",
    "MIT/Apache-2.0",
    "MIT",
    "Apache-2.0",
    "Unlicense OR MIT",
    "Unlicense/MIT",
    "BSD-2-Clause OR Apache-2.0 OR MIT",
    "(MIT OR Apache-2.0) AND Unicode-3.0",
    # Reviewed 2026-08-30 with the first working release:
    "0BSD OR MIT OR Apache-2.0",                              # adler2
    "MIT OR Zlib OR Apache-2.0",                              # miniz_oxide
    "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT",    # rustix, linux-raw-sys
    "MIT OR Apache-2.0 OR LGPL-2.1-or-later",                 # r-efi; taken under MIT
    # Reviewed 2026-08-31 with the PNG export renderer (issue 0017, ED-0093).
    # THIRD-PARTY-POLICY names BSD and Zlib among the PREFERRED permissive
    # licences, so none of these needs an ADR; each is recorded because the
    # policy also requires knowing what is shipped.
    "BSD-2-Clause",                                           # arrayref
    "BSD-3-Clause",                                           # tiny-skia, tiny-skia-path
    "Zlib",                                                   # zlib-rs
    "Zlib OR Apache-2.0 OR MIT",                              # bytemuck; taken under Apache-2.0
    # Reviewed 2026-09-12 by root (R6) as a BOUNDED DEPENDENCY REVIEW, against
    # the pinned upstream LICENSE and Cargo.toml at
    # raw.githubusercontent.com/nagisa/rust_libloading/0.8.9/. ISC is a
    # permissive grant whose only condition is that the copyright notice and
    # the permission notice appear in all copies, which scripts/rust-notice.py
    # discharges by reproducing the crate's own LICENSE verbatim.
    #
    # WHAT THIS IS NOT, recorded because the distinction is the whole basis of
    # the approval: technical project-policy acceptance. NOT legal clearance,
    # and NOT the owner personally reviewing the licence.
    #
    # No crate in the current graph declares ISC. It is admitted ahead of
    # `libloading` 0.8.9, which reaches a distributed closure only through
    # `wgpu` in the issue-0025 spike.
    "ISC",                                                    # libloading (issue 0025)
}


def _canonical(expression: str) -> str:
    """Key a licence expression by the SET of licences it offers, not by spelling.

    THE DEFECT THIS REMOVES. The check was exact string membership, so the same
    dual licence spelled in a different order was a build failure. The evidence
    is in the list above: seven of its entries exist only to spell a set that
    is already present -- "MIT OR Apache-2.0", "Apache-2.0 OR MIT" and
    "MIT/Apache-2.0" are one licence written three ways, and so are
    "MIT OR Zlib OR Apache-2.0" and "Zlib OR Apache-2.0 OR MIT". Every one was
    added when some crate turned up spelling it differently. Left alone, the
    list grows a permutation at a time and a reviewer reads spellings instead
    of licences.

    Found by the issue-0025 worker against the wgpu graph: `raw-window-handle`
    declares "MIT OR Apache-2.0 OR Zlib" and `rustc-hash` declares
    "Apache-2.0/MIT", and both were refused for word order alone while the
    identical licence sets sat in the list above.

    WHY THIS ADMITS NOTHING NEW. In SPDX, `OR` offers the distributor a choice;
    the order of the alternatives carries no meaning, so two expressions with
    the same alternatives impose the same obligations. Only flat expressions
    joined by ONE operator are normalised. Anything with parentheses or mixing
    `AND` with `OR` is compared exactly as before, because the reviewed entry
    "(MIT OR Apache-2.0) AND Unicode-3.0" means something order-insensitivity
    must not be allowed to reason about loosely.

    The legacy slash form is Cargo's old spelling of a disjunction and is
    expanded to `OR` on both sides of the comparison.
    """
    text = expression.strip()
    if "(" in text or ")" in text:
        return text
    expanded = text.replace("/", " OR ")
    has_or = " OR " in expanded
    has_and = " AND " in expanded
    if has_or and has_and:
        return text
    if has_or:
        return "OR:" + " OR ".join(sorted(t.strip() for t in expanded.split(" OR ")))
    if has_and:
        return "AND:" + " AND ".join(sorted(t.strip() for t in expanded.split(" AND ")))
    return text


# The reviewed list keyed by licence set. ALLOWED stays the human-readable
# record -- a reviewer should read licences there, not permutations.
ALLOWED_CANONICAL = {_canonical(e) for e in ALLOWED}


def is_allowed(declared: str) -> bool:
    return _canonical(declared) in ALLOWED_CANONICAL


def main() -> int:
    meta = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    )

    workspace = set(meta["workspace_members"])
    local = {p["id"] for p in meta["packages"] if p["id"] in workspace}

    # Classify each crate by the strongest dependency kind that reaches it.
    # A crate reachable as a normal dependency is DISTRIBUTED with Domowik; a
    # dev-only crate (test/bench harness) is not, and the policy treats the two
    # differently.
    runtime = set()
    for node in meta["resolve"]["nodes"]:
        if node["id"] not in local:
            continue
        for dep in node["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                runtime.add(dep["pkg"])

    # Transitively close runtime over normal edges.
    by_id = {n["id"]: n for n in meta["resolve"]["nodes"]}
    frontier = list(runtime)
    while frontier:
        cur = frontier.pop()
        for dep in by_id.get(cur, {}).get("deps", []):
            if any(k["kind"] is None for k in dep["dep_kinds"]) and dep["pkg"] not in runtime:
                runtime.add(dep["pkg"])
                frontier.append(dep["pkg"])

    rows, offenders = [], []
    for pkg in meta["packages"]:
        if pkg["id"] in local:
            continue
        lic = pkg.get("license") or "UNSPECIFIED"
        kind = "runtime" if pkg["id"] in runtime else "dev"
        rows.append((pkg["name"], pkg["version"], kind, lic))
        if not is_allowed(lic):
            offenders.append(f"{pkg['name']} {pkg['version']}: {lic}")

    rows.sort()

    out = [
        "# Rust third-party dependency inventory",
        "",
        "GENERATED by scripts/rust-deps-inventory.py. Do not edit by hand;",
        "`scripts/verify.sh` regenerates this and fails on any difference.",
        "",
        "`runtime` crates are distributed with Domowik. `dev` crates are test and",
        "benchmark tooling and are not distributed.",
        "",
        "| Crate | Version | Kind | Declared licence |",
        "|-------|---------|------|------------------|",
    ]
    out += [f"| {n} | {v} | {k} | {lic} |" for n, v, k, lic in rows]
    out += [
        "",
        f"Total: {len(rows)} third-party crates "
        f"({sum(1 for r in rows if r[2] == 'runtime')} runtime, "
        f"{sum(1 for r in rows if r[2] == 'dev')} dev).",
        "",
    ]
    sys.stdout.write("\n".join(out))

    if offenders:
        sys.stderr.write(
            "rust-deps-inventory: licence not on the reviewed allow-list:\n  "
            + "\n  ".join(offenders)
            + "\nAdd an explicit decision record and review per "
              "docs/THIRD-PARTY-POLICY.md before allowing it.\n"
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
