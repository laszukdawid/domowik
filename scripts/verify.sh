#!/usr/bin/env bash
# Canonical verification entry point for issue 0008 (contract v4, section 5),
# extended by issue 0036 with dynamic release-inventory checks. Runs every
# hard check locally; ci.yml invokes the same script. All issue-0008 baseline
# product checks are retained; issue 0062 replaces the Go corpus gates with
# their Rust counterparts and retires the now-empty Go module guards.
set -euo pipefail

# Resolved from this script's own location, not from $PWD: several stages `cd`
# into subdirectories inside a subshell, and one of them needs to invoke a
# sibling script by an absolute path from there.
REPO_ROOT=$(cd -P "$(dirname "$0")/.." && pwd -P)

# ED-0077 / ED-0087 R3: NAME THE FAILING STAGE.
#
# Before this, every stage failed SILENTLY: `set -e` aborted the run with no
# message, so the tail of the output showed the last stage that SUCCEEDED and
# the run looked like it had ended normally. On 2026-08-31 two pairs reported
# "verify.sh exit 0" for branches that could not have passed, because they read
# the tail instead of capturing $?. An unnamed failure is indistinguishable
# from a pass to anything that is not checking the exit status.
#
# Note what this deliberately does NOT do: it does not echo a stage name BEFORE
# running the stage. A pre-announcement says the stage STARTED, never that it
# PASSED, which is the same defect wearing a label. The name is printed on the
# FAILURE path, carrying the captured exit status of the command that failed.
#
# The block between the markers below is executed verbatim, under every bash
# on the machine, by scripts/ci/verify-banner-regression.sh. Keep it
# self-contained: the regression test extracts these lines and nothing else.
# BEGIN FAILURE-REPORTING PREAMBLE
CURRENT_STAGE="startup"
stage() { CURRENT_STAGE="$1"; }

# ERRTRACE, on bash 3.x only.
#
# Several stages below run inside `( ... )`, and bash 3.2 -- still /bin/bash
# on every macOS host, GitHub's macos-15 runners included -- NEVER runs a
# top-level ERR trap when a subshell fails. GitHub Actions run 34672441495 hit
# exactly that: the macOS bundle stage failed and the run printed NEITHER
# banner, so the CI job summary reported "did not complete ... may have been
# interrupted, e.g. by a timeout" for a stage that had genuinely failed. That
# is the ED-0077 silence this trap exists to end, returned on the one platform
# where the macOS stages actually run.
#
# `set -E` makes the trap reach subshells. It is CONDITIONAL because bash >= 4
# already reports the failure from the parent, and enabling it there fires the
# trap in both the subshell and its parent -- two banners for one failure,
# which is misleading in its own way. Measured, not assumed:
#
#   bash 3.2, no -E: no banner        bash 5.3, no -E: one banner (parent)
#   bash 3.2, -E:    one (subshell)   bash 5.3, -E:    two banners
if [ "${BASH_VERSINFO[0]}" -lt 4 ]; then
  set -E
fi

trap 'rc=$?; printf "\n=== VERIFICATION FAILED ===\nSTAGE: %s\nline:  %s\nexit:  %s\n" \
  "$CURRENT_STAGE" "$LINENO" "$rc" >&2; exit "$rc"' ERR
# END FAILURE-REPORTING PREAMBLE

if [ "${DOMOWIK_REQUIRE_LINUX_EDITOR:-0}" = 1 ]; then
  stage "mandatory Linux editor prerequisites"
  bash "$REPO_ROOT/scripts/check-linux-editor-prerequisites.sh"
fi

# Issue 0062 / ED-0104: the last active Go package is now Rust. Its corpus
# gates run below after cargo test. The Go build, test, format, vet and module
# guards are retired with the root module; _reference/go remains frozen.

# WITHDRAWN: the Go release-inventory verifier (0036).
#
# `internal/release`, `cmd/domowik-sbom` and `release/` describe a GO product
# -- two linux binaries built from ./cmd/domowik -- and its manifest enforces
# `artifacts must not be empty` as a fail-closed guard against shipping
# nothing. With the Go core frozen there is no Go artifact to describe, and the
# three honest options were: point the manifest at a build tool (a lie), relax
# the empty-set guard (weakening a fail-closed check), or freeze the release
# machinery together with the product it describes. The third was chosen.
#
# The committed SPDX documents and NOTICE move with it to _reference/go/release
# as the record of the last Go state. They are NOT deleted and NOT regenerated.
#
# THE OBLIGATION SURVIVES THE TOOLING. Domowik still ships under Apache-2.0 and
# docs/THIRD-PARTY-POLICY.md still requires every distributed dependency to be
# licence-compatible and recorded. The Rust workspace has just introduced a new
# dependency surface, so that obligation is now LARGER, not suspended. The
# stage below keeps it enforced until a Rust release/SBOM process exists.

# ===== Rust core stages =====
#
# The Rust workspace is the CORE (owner decision, ED-0086/ED-0091); the Go tree
# under _reference/go/ is frozen reference and is invisible to the Go toolchain.
# Cargo must be on PATH -- this deliberately does NOT source ~/.cargo/env,
# because a verification run that depends on a developer's home directory
# cannot be trusted in CI.
mkdir -p build/ci
stage "cargo present"
command -v cargo >/dev/null 2>&1 || {
  echo "cargo not found on PATH; install the toolchain pinned in rust-toolchain.toml" >&2
  exit 127
}

stage "cargo fmt"
cargo fmt --all -- --check

stage "cargo build"
cargo build --workspace --all-targets --locked

stage "cargo clippy"
cargo clippy --workspace --all-targets --locked -- -D warnings

stage "cargo test"
cargo test --workspace --locked

# Keep a separately named failure stage for the corpus contract, even though
# the workspace test suite also runs it (ED-0077).
stage "corpus gates"
cargo test --locked -p domowik-devtools --test corpus_gates

# Issue 0004: syntax-validate every fenced ```json block in committed docs,
# then the same parse-only guard over the conformance corpus manifest.
# Parse-only by design; schema/conformance checking belongs to issues
# 0028/0029 (see the tool's own scope statement).
#
# `cargo run` rather than a path into target/: the profile and the binary name
# stay the toolchain's business, and a stale binary from an earlier profile
# cannot be picked up silently. The workspace is already built by the stage
# above, so this compiles nothing.
JSONCHECK=(cargo run --locked -q -p domowik-devtools --bin domowik-jsoncheck --)
stage "jsoncheck docs"
"${JSONCHECK[@]}" docs
stage "jsoncheck corpus"
"${JSONCHECK[@]}" testdata/corpus

# PERFORMANCE IS A GATE, NOT AN ASPIRATION (owner direction, 2026-08-30).
# Benchmarks must COMPILE on every run, so a change that breaks the measurement
# harness fails here rather than being discovered the next time someone
# remembers to measure. Running them is separate: timings are host-dependent and
# do not belong in a pass/fail gate (ED-0070, measurement authority).
# Third-party licence guard for the Rust workspace. Fail-closed: the inventory
# is regenerated and diffed, so adding a dependency FAILS here until it is
# recorded and its licence reviewed (docs/THIRD-PARTY-POLICY.md). The generator
# itself exits non-zero on a licence outside the reviewed allow-list.
stage "rust dependency inventory"
python3 scripts/rust-deps-inventory.py > build/ci/rust-deps.generated
diff -u docs/RUST-DEPENDENCIES.md build/ci/rust-deps.generated

# THE NOTICE OBLIGATION, enforced the same way. Domowik ships under Apache-2.0
# and several runtime crates carry licences that REQUIRE their notices to be
# reproduced in distributions — BSD-2-Clause and BSD-3-Clause both say so, and
# Apache-2.0 §4(d) requires carrying forward any NOTICE file. Regenerating and
# diffing means a dependency change fails the build until the notice is
# refreshed, rather than shipping a stale one.
stage "rust notice"
python3 scripts/rust-notice.py > build/ci/notice.generated
diff -u NOTICE build/ci/notice.generated

stage "cargo bench compile"
cargo bench --workspace --locked --no-run

# THE INTERACTION HARNESS MUST STILL RUN, not merely compile (issue 0032).
#
# `--smoke` takes three samples of everything. Three samples on an unknown CI
# machine are NOT a measurement and nothing here compares a timing against a
# threshold -- the thresholds in docs/BENCHMARK-INTERACTION.md are calibrated
# to a named reference host and CI is not it (ED-0070). What this gates is
# that the corpus still generates, every gesture class still finds a target
# somewhere, and the locality comparison still produces rows. A report that
# quietly lost a corpus or a class would otherwise keep looking healthy while
# measuring less than it claims.
stage "interaction benchmark smoke"
cargo run --release --locked -q -p domowik-bench -- --smoke > build/ci/bench-smoke.md
for section in "## Corpus" "## Derived geometry" "## Operation mix" \
               "## Invalidation locality"; do
  grep -qF "$section" build/ci/bench-smoke.md || {
    echo "benchmark report is missing the '$section' section" >&2
    exit 1
  }
done
for corpus in flat-s flat-m house-l wings-l campus-xl islands \
              shape-pinched shape-overlap shape-nonconvex; do
  grep -qF "| \`$corpus\` |" build/ci/bench-smoke.md || {
    echo "benchmark report no longer covers corpus '$corpus'" >&2
    exit 1
  }
done
for class in wall-create-free wall-create-joined wall-create-split \
             endpoint-drag joint-drag body-drag detach numeric-edit \
             wall-delete opening-create opening-edit opening-delete \
             opening-cascade-delete host-reverse host-shorten-reject; do
  grep -qF "| \`$class\` |" build/ci/bench-smoke.md || {
    echo "benchmark report no longer exercises class '$class' on any corpus" >&2
    exit 1
  }
done
locality_rows=$(sed -n '/## Invalidation locality/,$p' build/ci/bench-smoke.md \
                | grep -c '^| `' || true)
[ "$locality_rows" -ge 40 ] || {
  echo "locality table has $locality_rows rows; expected at least 40" >&2
  exit 1
}

# CLI stream and exit-code probes P1-P3, REINSTATED against the Rust binary.
#
# These were withdrawn when ./cmd/domowik was frozen, on the record that
# reinstating them was an acceptance criterion of the CLI port rather than an
# optional follow-up. The port has landed, so they are back.
#
# Expected-failure cases use the guard form so `set -e` does not abort on the
# binary's CORRECT non-zero exit status; streams are captured separately so
# "wrote something to the wrong stream" is a detectable failure rather than an
# invisible one.
stage "cli build"
mkdir -p build/ci && rm -rf build/ci/*
cargo build --release --locked -p domowik-cli
CLI=target/release/domowik

# KEEP THE PROBES' STDERR. On 2026-08-31 a run exited 101 — Rust's panic code —
# somewhere in this block, and the panic message went to the console where it
# was lost to a truncated tail. Five full re-runs and eighty-five targeted
# replays could not reproduce it, so the cause is unknown and the next
# occurrence must not be unknowable too.
#
# `tee` rather than a plain redirect: the message still reaches the console for
# a human watching, AND lands in a file that survives. Applied to the shell
# rather than to each command because the exit-code probes below assert 1 and 2
# specifically, and wrapping them individually is how a harness starts
# flattening the codes it exists to check (ED-0077).
exec 2> >(tee -a build/ci/probes.err >&2)

# P1: unknown subcommand -> exit 2, stderr non-empty, stdout EMPTY
stage "probe P1 malformed input"
rc=0; "$CLI" frobnicate >build/ci/p1.out 2>build/ci/p1.err || rc=$?
test "$rc" -eq 2
test -s build/ci/p1.err && test ! -s build/ci/p1.out

# P2: no arguments -> exit 2, usage on stderr ONLY
stage "probe P2 no arguments"
rc=0; "$CLI" >build/ci/p2.out 2>build/ci/p2.err || rc=$?
test "$rc" -eq 2
test -s build/ci/p2.err && test ! -s build/ci/p2.out

# P3: --version -> exit 0, match on stdout, stderr EMPTY
stage "probe P3 version"
"$CLI" --version >build/ci/p3.out 2>build/ci/p3.err
grep -q '^domowik ' build/ci/p3.out
test ! -s build/ci/p3.err

# P4: a runtime failure exits 1, NOT 2. The distinction between "you used this
# wrong" and "this did not work" is the whole reason the codes differ, and a
# harness that flattens them cannot tell a user which mistake they made.
stage "probe P4 runtime failure"
rc=0; "$CLI" info build/ci/definitely-absent.domo \
  >build/ci/p4.out 2>build/ci/p4.err || rc=$?
test "$rc" -eq 1
test -s build/ci/p4.err && test ! -s build/ci/p4.out

# P5: the round trip a user actually performs -- create, edit, inspect, render.
# This is an END-TO-END check: it fails if persistence, validation, geometry or
# rendering breaks, which no unit test covers together.
stage "probe P5 end-to-end round trip"
"$CLI" new build/ci/probe.domo >/dev/null
"$CLI" add-wall build/ci/probe.domo 0 0 4000 0 >/dev/null
"$CLI" add-wall build/ci/probe.domo 4000 0 4000 3000 >/dev/null
"$CLI" add-wall build/ci/probe.domo 4000 3000 0 3000 >/dev/null
"$CLI" add-wall build/ci/probe.domo 0 3000 0 0 >/dev/null
"$CLI" info build/ci/probe.domo | grep -q 'walls: 4'
# Corners auto-join (R3.1.2); four walls placed corner to corner give four.
"$CLI" info build/ci/probe.domo | grep -q 'joins: 4'
"$CLI" validate build/ci/probe.domo | grep -q 'valid'
# Floor area is measured to the INNER faces of the walls: with the 120 mm
# default thickness that is 3880 x 2880 = 11.17 m², not the 4000 x 3000
# centre-line figure of 12.00 m². A regression to centre lines fails here.
"$CLI" info build/ci/probe.domo | grep -q '11.17 m²'
"$CLI" export-svg build/ci/probe.domo build/ci/probe.svg >/dev/null
grep -q '^<svg' build/ci/probe.svg
# One room fill and one wall component: the four walls union into a single
# occupied region with the room as its hole.
test "$(grep -c '<path' build/ci/probe.svg)" -eq 2

# P6: the operation log is the ONLY path by which a document changes.
# The four wall commands emit batches; new supplies a non-undoable baseline.
stage "probe P6 operation log"
"$CLI" info build/ci/probe.domo | grep -q 'operations:     '
"$CLI" ops build/ci/probe.domo | grep -q 'canonical order'
"$CLI" ops build/ci/probe.domo | grep -q 'create wallJoin'

# P7: undo appends a compensating batch and removes the effect, and undoing
# the undo restores it. History GROWS both times (BEHAVIOR-SPEC R8.1.1).
stage "probe P7 undo"
before=$("$CLI" info build/ci/probe.domo | sed -n 's/^  operations: *\([0-9]*\) .*/\1/p')
"$CLI" undo build/ci/probe.domo >/dev/null
"$CLI" info build/ci/probe.domo | grep -q 'walls: 3'
"$CLI" undo build/ci/probe.domo >/dev/null
"$CLI" info build/ci/probe.domo | grep -q 'walls: 4'
"$CLI" info build/ci/probe.domo | grep -q '11.17 m²'
# R8.1.1: undo APPENDS a compensating batch, it does not remove history. The
# wall counts above show the effect reversed; only this shows the mechanism.
# Without it the probe passes for an implementation that pops the log, which is
# the thing R8.1.1 forbids (found by project review, 2026-09-01).
after=$("$CLI" info build/ci/probe.domo | sed -n 's/^  operations: *\([0-9]*\) .*/\1/p')
test "$after" -gt "$before"

# P8: an external batch applies, and a second delivery of the SAME batch is
# ignored rather than applied twice (FORMAT-SPEC V1/I2).
stage "probe P8 apply"
python3 - > build/ci/batch.json <<'PYEOF'
import json, uuid
level = str(uuid.uuid4())
print(json.dumps({
    "batchId": str(uuid.uuid4()),
    "actorId": str(uuid.uuid4()),
    "counterBase": 1,
    "frontier": {},
    "operations": [{
        "operationId": str(uuid.uuid4()),
        "entityId": level,
        "kind": "createEntity",
        "entity": {
            "type": "level", "id": level, "name": "Loft",
            "elevationMm": 2700.0, "floorThicknessMm": 200.0,
            "defaultWallHeightMm": 2400.0, "defaultCeilingHeightMm": 2400.0,
        },
    }],
}))
PYEOF
"$CLI" apply build/ci/probe.domo build/ci/batch.json | grep -q '^Applied'
"$CLI" info build/ci/probe.domo | grep -q 'levels: 2'
"$CLI" apply build/ci/probe.domo build/ci/batch.json | grep -q 'already applied'
"$CLI" info build/ci/probe.domo | grep -q 'levels: 2'

# P9: PNG export produces a real raster, bounded, and deterministic.
stage "probe P9 png export"
"$CLI" export-png build/ci/probe.domo build/ci/probe.png --scale 0.2 \
  | grep -q 'px at 0.200 px/mm'
# The PNG signature, checked on the bytes rather than trusting the message.
head -c 8 build/ci/probe.png | od -An -tx1 | tr -d ' \n' | grep -q '^89504e470d0a1a0a$'
# Determinism: the same document at the same scale is byte-identical.
"$CLI" export-png build/ci/probe.domo build/ci/probe2.png --scale 0.2 >/dev/null
cmp build/ci/probe.png build/ci/probe2.png
# An absurd scale is CLAMPED, not refused, and says so.
"$CLI" export-png build/ci/probe.domo build/ci/huge.png --scale 500 \
  | grep -q 'Scale reduced'

# Restore stderr before leaving the probe block, so later stages are not
# writing into a file named for the probes.
exec 2>&1

# ISSUE 0036: the release inventory, validated against the binary just built.
#
# Placed HERE and not next to the licence stages above because the first call
# needs target/release/domowik to exist: without it the validator refuses
# `missing-artifact`, which is correct behaviour and a useless stage.
#
# TWO CALLS, AND THE SECOND IS NOT REDUNDANT. `--artifact` FILTERS the artifact
# list, so the first call audits only the root workspace graph. The second call
# drops the filter and is the one that reaches the 0024 spike, which is its own
# Cargo workspace (ED-0097) and is invisible to both the first call and to
# `rust-deps-inventory.py`, neither of which passes a --manifest-path. An
# earlier draft of this stage ran the first call alone and claimed spike
# coverage in a comment; it did not have it (root, R19). Both invocations are
# mutation-proven in scripts/ci/release-inventory-test.sh: an UNREVIEWED
# LICENCE EXPRESSION reachable only from the spike fails the second call and
# PASSES the first. The mutant is `ryu` 1.0.23, "Apache-2.0 OR BSL-1.0", which
# is off the reviewed allow-list; BSL-1.0 is permissive, not copyleft, and
# calling it copyleft would misstate what the guard caught.
#
# The second call omits `--require-built` deliberately. The Flutter bundle is
# declared `staged`, so requiring a built artifact would refuse
# `incomplete-coverage` on an artifact nobody has asked this run to build.
#
# Measured cost on an Apple Silicon host with a warm cargo cache: 0.245s for
# the first call, 0.30s for the second. Both are one `cargo metadata` slower
# cold.
#
# `--skip-generated` is deliberate and is the ONLY place it is correct. The
# `rust notice` and `rust dependency inventory` stages above have already
# regenerated and diffed NOTICE and docs/RUST-DEPENDENCIES.md; repeating that
# here would cost two more `cargo metadata` runs and a second pass over every
# runtime crate's licence files to reprove what this run just proved. Run
# standalone, the validator checks them by default, because a release gate that
# trusted files it had not regenerated is the ED-0094 failure.
#
# What this ADDS over the licence stages above: the CLI's target-filtered
# linked closure rather than the workspace-wide list, the standard library from
# the pinned toolchain record, an `include_bytes!`, pubspec-font and
# copied-asset scan against docs/third-party/assets.json, the measured binary, and the spike workspace.
stage "release inventory"
python3 scripts/release-validate.py --require-built --artifact domowik-cli \
  --skip-generated
python3 scripts/release-validate.py --skip-generated

# The SBOM must be reproducible, which is a property no single run can show.
# Two generations of the same document over the same tree, compared byte for
# byte: a generator that read the clock, iterated a set, or embedded a random
# serialNumber fails here rather than at the moment somebody tries to diff two
# releases.
stage "release sbom determinism"
python3 scripts/release-sbom.py artifact --id domowik-cli > build/ci/sbom1.json
python3 scripts/release-sbom.py artifact --id domowik-cli > build/ci/sbom2.json
cmp build/ci/sbom1.json build/ci/sbom2.json

# A file copied into one artifact (copiedAssets) is listed for that artifact
# only: the browser editor's Roboto is in the web SBOM and not the macOS one,
# while the pubspec symbols subset is in both (ED-0121).
stage "release sbom asset membership"
python3 scripts/release-sbom.py artifact --id domowik-gui-web \
  --target wasm32-wasip1 > build/ci/sbom-web.json
python3 scripts/release-sbom.py artifact --id domowik-gui-macos \
  --target aarch64-apple-darwin > build/ci/sbom-macos.json
python3 - build/ci/sbom-web.json build/ci/sbom-macos.json <<'PY'
import json, sys
def assets(path):
    with open(path, encoding="utf-8") as handle:
        return {c["name"] for c in json.load(handle)["components"]
                if c["bom-ref"].startswith("asset:")}
web, macos = assets(sys.argv[1]), assets(sys.argv[2])
assert {"Roboto Regular", "Noto Sans Symbols 2 Subset"} <= web, web
assert "Noto Sans Symbols 2 Subset" in macos and "Roboto Regular" not in macos, macos
PY

# P13a: the asset store. A background image imports, deduplicates, verifies,
# renders beneath the plan, and survives a save.
stage "probe P13a asset store"
"$CLI" new build/ci/asset.domo >/dev/null
"$CLI" export-png build/ci/asset.domo build/ci/scan.png --scale 0.2 >/dev/null || true
"$CLI" add-wall build/ci/asset.domo 0 0 4000 0 >/dev/null
"$CLI" export-png build/ci/asset.domo build/ci/scan.png --scale 0.2 >/dev/null
"$CLI" asset add build/ci/asset.domo build/ci/scan.png --scale 8 | grep -q '^Imported'
"$CLI" asset list build/ci/asset.domo | grep -q 'referenced by: Level 1'
"$CLI" asset verify build/ci/asset.domo | grep -q 'verified against their digests'
# Content addressing means a second import stores no second copy.
"$CLI" asset add build/ci/asset.domo build/ci/scan.png --scale 8 | grep -q 'deduplicate'
"$CLI" asset list build/ci/asset.domo | grep -q '1 embedded blob'
# The blob survives an ordinary edit (§7.4 invariant I2).
"$CLI" add-wall build/ci/asset.domo 4000 0 4000 3000 >/dev/null
"$CLI" asset verify build/ci/asset.domo | grep -q 'verified'
"$CLI" asset remove build/ci/asset.domo \
  "$("$CLI" asset list build/ci/asset.domo | grep -o 'sha256:[0-9a-f]*' | head -1)" >/dev/null
"$CLI" asset list build/ci/asset.domo | grep -q 'no assets'

# P13b: pack --portable is all-or-nothing, and --thin is refused rather than
# approximated.
stage "probe P13b pack"
"$CLI" pack build/ci/asset.domo build/ci/portable.domo --portable | grep -q 'portable package'
"$CLI" info build/ci/portable.domo | grep -q 'walls: 2'
rc=0; "$CLI" pack build/ci/asset.domo build/ci/thin.domo --thin \
  >build/ci/thin.out 2>build/ci/thin.err || rc=$?
test "$rc" -eq 2
test -s build/ci/thin.err && test ! -s build/ci/thin.out
test ! -e build/ci/thin.domo

# P12: R3.1.2 splitting. A wall dropped across another splits BOTH at the
# crossing and joins the four children into ONE node, and the whole thing is a
# single batch — so one undo restores the pair.
stage "probe P12 wall splitting"
"$CLI" new build/ci/split.domo >/dev/null
"$CLI" add-wall build/ci/split.domo 0 0 4000 0 >/dev/null
"$CLI" add-wall build/ci/split.domo 2000 -2000 2000 2000 | grep -q 'Split 2 wall'
"$CLI" info build/ci/split.domo | grep -q 'walls: 4'
"$CLI" info build/ci/split.domo | grep -q 'joins: 1'
"$CLI" validate build/ci/split.domo | grep -q 'valid'
"$CLI" undo build/ci/split.domo >/dev/null
"$CLI" info build/ci/split.domo | grep -q 'walls: 1'
"$CLI" info build/ci/split.domo | grep -q 'joins: 0'

# P11: machine-readable output. Valid JSON on stdout, and `validate --json`
# on an INVALID plan prints the report AND exits non-zero — a caller that asked
# for machine-readable output wants the problems, not just a code.
stage "probe P11 json output"
"$CLI" info build/ci/probe.domo --json > build/ci/report.json
python3 -c "import json,sys; d=json.load(open('build/ci/report.json')); \
    assert d['valid'] is True, d; \
    assert d['levels'][0]['rooms'][0]['areaMm2'] > 0, d; \
    assert d['levels'][0]['walls'][0]['index'] == 1, d"
"$CLI" validate build/ci/probe.domo --json | python3 -c "import json,sys; json.load(sys.stdin)"

# The INVALID path, which is the half that matters: the report goes to STDOUT
# and the command still exits non-zero. Checking only the valid document
# proves neither (found by project review, 2026-09-01).
"$CLI" new build/ci/bad.domo >/dev/null
"$CLI" add-wall build/ci/bad.domo 0 0 4000 0 >/dev/null
python3 - <<'PYEOF'
import hashlib, json, shutil, zipfile

# The entry names are FORMAT-SPEC §7.3's: the manifest is `domowik.json` and
# the snapshot section is `snapshot/state.json`, whose root is the §7.5 section
# with the projection as a member rather than the document itself.
MANIFEST, SNAPSHOT = "domowik.json", "snapshot/state.json"

src, dst = "build/ci/bad.domo", "build/ci/bad.tmp"
with zipfile.ZipFile(src) as z:
    names = z.namelist()
    blobs = {n: z.read(n) for n in names}

section = json.loads(blobs[SNAPSHOT])
section["document"]["walls"] = [{
    "id": "01948f55-6b18-7d3a-a153-1ee2f33cc402",
    "levelId": "01948f55-6b18-7d3a-a153-1ee2f33cc403",
    "startMm": [0.0, 0.0], "endMm": [4000.0, 0.0],
    "thicknessMm": -5.0, "heightMm": 2500.0,
}]
section["document"]["levels"] = []
blobs[SNAPSHOT] = json.dumps(section).encode()

# Drop the ops/ entries: the snapshot alone is the whole document now.
kept = [n for n in names if not n.startswith("ops/")]

# Rebuild §7.4's `entries` inventory over the bytes actually written. It is the
# authoritative integrity layer and the reader verifies it BEFORE parsing
# anything for meaning, so a stale inventory would make this package reject as
# a corrupt container - and the probe would pass for the wrong reason, never
# reaching the advisory-validation path it exists to check.
manifest = json.loads(blobs[MANIFEST])
manifest["entries"] = {
    n: {
        "sha256": "sha256:" + hashlib.sha256(blobs[n]).hexdigest(),
        "byteLength": len(blobs[n]),
    }
    for n in kept if n != MANIFEST
}
blobs[MANIFEST] = json.dumps(manifest).encode()

with zipfile.ZipFile(dst, "w") as z:
    for n in kept:
        z.writestr(n, blobs[n])
shutil.move(dst, src)
PYEOF
rc=0; "$CLI" validate build/ci/bad.domo --json \
  >build/ci/bad.out 2>build/ci/bad.err || rc=$?
test "$rc" -eq 1
test -s build/ci/bad.out && test ! -s build/ci/bad.err
python3 -c "import json; d=json.load(open('build/ci/bad.out')); \
    assert d['valid'] is False, d; \
    assert any('thicknessMm' in p for p in d['problems']), d"

# P10: a closed downstream pipe is not an error.
#
# `println!` PANICS when stdout cannot be written and Rust ignores SIGPIPE, so
# `domowik info | head -1` exited 101. This ran intermittently red for two
# sessions and was twice dismissed as unreproducible, because whether `head`
# closes the pipe before or after the write completes is a race. `yes` on the
# reader side makes the close immediate rather than racy.
stage "probe P10 closed pipe"
rc=0; "$CLI" info build/ci/probe.domo 2>build/ci/p10.err | head -1 >/dev/null || rc=$?
test "$rc" -eq 0
test ! -s build/ci/p10.err

# P14: compaction. The log folds into the snapshot, the projection does not
# move, a deleted entity stays deleted, and editing continues afterwards.
stage "probe P14 compaction"
"$CLI" info build/ci/probe.domo --json > build/ci/pre-compact.json
# The retention effects are stated BEFORE the irreversible write, and the dry
# run leaves the plan byte-identical.
BEFORE=$(shasum -a 256 < build/ci/probe.domo)
"$CLI" compact build/ci/probe.domo --dry-run | grep -q 'Would compact'
"$CLI" compact build/ci/probe.domo --dry-run | grep -q 'DISCARDED'
test "$BEFORE" = "$(shasum -a 256 < build/ci/probe.domo)"
# Execution asserts locality: nothing in a package says whether a peer holds a
# copy, so §5.1 S6 forbids assuming one does not (issue 0027).
"$CLI" compact build/ci/probe.domo --assert-local-only | grep -q 'folded into the snapshot'
"$CLI" info build/ci/probe.domo | grep -q 'operations:     0 in 0 batch'
# The projection must be identical apart from the batch counts the report
# carries, so compare the level content rather than the whole report.
"$CLI" info build/ci/probe.domo --json > build/ci/post-compact.json
python3 -c "import json; a=json.load(open('build/ci/pre-compact.json')); \
    b=json.load(open('build/ci/post-compact.json')); \
    assert a['levels'] == b['levels'], 'compaction moved the document'"
# Editing still works, and the new work is still undoable.
"$CLI" add-wall build/ci/probe.domo 1000 1000 3000 1000 >/dev/null
"$CLI" validate build/ci/probe.domo | grep -q 'valid'
"$CLI" undo build/ci/probe.domo >/dev/null
"$CLI" info build/ci/probe.domo | grep -q 'walls: 4'
# A second compaction is a no-op rather than an error. The fold needs the
# assertion; the no-op does not, because a package with no batches returns
# before the gate.
"$CLI" compact build/ci/probe.domo --assert-local-only >/dev/null
"$CLI" compact build/ci/probe.domo | grep -q 'already compact'

# P15: `apply --dry-run` writes nothing and reports the projection, and an
# oversized external batch is refused BEFORE it is applied. The batch file is
# untrusted input that never reaches the package reader enforcing §9.3, so
# this asserts the apply path's own bounds from the outside.
stage "probe P15 apply dry-run and input bounds"
"$CLI" new build/ci/dry.domo >/dev/null
python3 - > build/ci/dry-batch.json <<'PYGEN'
import json, uuid


def level(name):
    ident = str(uuid.uuid4())
    return ident, {
        "type": "level", "id": ident, "name": name,
        "elevationMm": 0.0, "floorThicknessMm": 200.0,
        "defaultWallHeightMm": 2400.0, "defaultCeilingHeightMm": 2400.0,
    }


ident, entity = level("Loft")
print(json.dumps({
    "batchId": str(uuid.uuid4()),
    "actorId": str(uuid.uuid4()),
    "counterBase": 1,
    "frontier": {},
    "operations": [{
        "operationId": str(uuid.uuid4()),
        "entityId": ident,
        "kind": "createEntity",
        "entity": entity,
    }],
}))
PYGEN
BEFORE=$(shasum -a 256 < build/ci/dry.domo)
"$CLI" apply build/ci/dry.domo build/ci/dry-batch.json --dry-run | grep -q 'Would apply'
"$CLI" apply build/ci/dry.domo build/ci/dry-batch.json --dry-run | grep -q 'dry-run'
test "$BEFORE" = "$(shasum -a 256 < build/ci/dry.domo)"
# The same batch still applies for real afterwards, so the dry run consumed
# nothing.
"$CLI" apply build/ci/dry.domo build/ci/dry-batch.json | grep -q 'Applied'
"$CLI" info build/ci/dry.domo | grep -q 'levels: 2'
# One over the §9.3 row L23 per-batch operation limit is refused, exit 1, and
# the plan is untouched.
python3 - > build/ci/huge-batch.json <<'PYGEN'
import json, uuid

ops = []
for i in range(65_536):
    ident = str(uuid.uuid4())
    ops.append({
        "operationId": str(uuid.uuid4()),
        "entityId": ident,
        "kind": "createEntity",
        "entity": {
            "type": "level", "id": ident, "name": "L%d" % i,
            "elevationMm": 0.0, "floorThicknessMm": 200.0,
            "defaultWallHeightMm": 2400.0, "defaultCeilingHeightMm": 2400.0,
        },
    })
print(json.dumps({
    "batchId": str(uuid.uuid4()),
    "actorId": str(uuid.uuid4()),
    "counterBase": 1,
    "frontier": {},
    "operations": ops,
}))
PYGEN
BEFORE=$(shasum -a 256 < build/ci/dry.domo)
rc=0; "$CLI" apply build/ci/dry.domo build/ci/huge-batch.json \
  >build/ci/huge.out 2>build/ci/huge.err || rc=$?
test "$rc" -eq 1
grep -q 'L23' build/ci/huge.err
test ! -s build/ci/huge.out
test "$BEFORE" = "$(shasum -a 256 < build/ci/dry.domo)"

# P16: new-plan baselines and level deletion use the shared core guard.
stage "probe P16 level lifecycle"
"$CLI" new build/ci/levels.domo >/dev/null
level_before=$(shasum -a 256 < build/ci/levels.domo)
rc=0; "$CLI" undo build/ci/levels.domo >build/ci/level.out 2>build/ci/level.err || rc=$?
test "$rc" -eq 1
grep -q 'no operations' build/ci/level.err
test "$level_before" = "$(shasum -a 256 < build/ci/levels.domo)"
rc=0; "$CLI" remove-level build/ci/levels.domo 'Level 1' >build/ci/level.out 2>build/ci/level.err || rc=$?
test "$rc" -eq 1
grep -q 'only level' build/ci/level.err
test "$level_before" = "$(shasum -a 256 < build/ci/levels.domo)"
"$CLI" add-level build/ci/levels.domo Upper --elevation 3000 >/dev/null
"$CLI" remove-level build/ci/levels.domo Upper >/dev/null
"$CLI" undo build/ci/levels.domo >/dev/null
"$CLI" info build/ci/levels.domo | grep -q 'Upper'
"$CLI" validate build/ci/levels.domo >/dev/null

# P17: level field flags reach one validated core command; advisory reports
# stay successful, and omitted wall heights resolve the edited level default.
stage "probe P17 level fields and warnings"
"$CLI" new build/ci/level-fields.domo >/dev/null
"$CLI" set-level build/ci/level-fields.domo 'Level 1' --rename Basement \
  --elevation -2800 --floor-thickness 0 --wall-height 3300 --ceiling-height 3000 >/dev/null
"$CLI" add-level build/ci/level-fields.domo Upper --elevation -2800 >/dev/null
"$CLI" validate build/ci/level-fields.domo >build/ci/level-fields.out 2>build/ci/level-fields.err
test ! -s build/ci/level-fields.err
grep -q '3 warning(s)' build/ci/level-fields.out
"$CLI" add-wall build/ci/level-fields.domo 0 0 4000 0 --level Basement >/dev/null
"$CLI" add-wall build/ci/level-fields.domo 0 2000 4000 2000 --level Basement --height 1800 >/dev/null
"$CLI" validate build/ci/level-fields.domo --json | python3 -c '
import json,sys
d=json.load(sys.stdin)
assert d["valid"] is True and not d["problems"], d
assert len(d["warnings"]) == 3, d
assert {w["code"] for w in d["warnings"]} == {"ceiling-below-wall-height", "duplicate-elevation", "slab-intrudes-below-level"}, d
level=next(l for l in d["levels"] if l["name"] == "Basement")
assert level["floorThicknessMm"] == 0 and level["elevationMm"] == -2800, level
assert [w["heightMm"] for w in level["walls"]] == [3300,1800], level
'
level_fields_before=$(shasum -a 256 < build/ci/level-fields.domo)
rc=0; "$CLI" set-level build/ci/level-fields.domo Basement --rename Broken --wall-height 0 \
  >build/ci/level-fields.out 2>build/ci/level-fields.err || rc=$?
test "$rc" -eq 1
test ! -s build/ci/level-fields.out
test "$level_fields_before" = "$(shasum -a 256 < build/ci/level-fields.domo)"

# ===== The actively developed frontend and native bridge =====
# Its own Cargo workspace is excluded from the root workspace, so it needs
# explicit coverage in addition to the shared core checks above.
stage "frontend bridge build and tests"
(
  cd frontend
  cargo fmt --check
  cargo clippy --all-targets -- -D warnings
  cargo test
  cargo build --release
)

# THE BROWSER BUILDS (issue 0088, ADR 0012): the same bridge crate for
# wasm32-wasip1 and the WebGPU renderer for wasm32-unknown-unknown, linted as
# strictly as the native build (the Flutter browser stages below build them).
# Both targets come with rust-toolchain.toml.
stage "frontend browser modules lint"
(
  cd frontend
  cargo clippy -p domowik-frontend-bridge --target wasm32-wasip1 -- -D warnings
  cargo clippy -p domowik-web-renderer --target wasm32-unknown-unknown -- -D warnings
)

# The C harness proves what the Rust tests cannot: that the exported symbols
# exist with the declared signatures IN THE ARTEFACT THAT SHIPS, that a panic
# is contained unwinding into a foreign frame, and that ownership abuse from a
# caller with no borrow checker is refused rather than acted on. Built with
# AddressSanitizer and UBSan, so the run is a memory-safety gate too.
stage "frontend C harness (asan+ubsan)"
if command -v clang >/dev/null 2>&1; then
  (
    cd frontend
    mkdir -p build
    LIB=target/release/libdomowik_bridge.dylib
    [ -f "$LIB" ] || LIB=target/release/libdomowik_bridge.so
    clang -std=c11 -Wall -Wextra -Werror -g -fsanitize=address,undefined \
      -o build/charness native/charness/main.c
    ./build/charness "$LIB" 2>/dev/null | grep -q 'C HARNESS PASSED'
  )
else
  echo "frontend C harness: SKIPPED - clang not on PATH"
fi

# The Dart harness loads the same library the way Flutter does. Skipped with an
# ANNOUNCEMENT rather than in silence when the SDK is absent: a conditional
# stage that skips quietly cannot be told apart from one that ran and passed
# (ED-0087 R3).
stage "frontend Dart FFI harness"
if command -v dart >/dev/null 2>&1; then
  (
    cd frontend/dart_harness
    LIB=../target/release/libdomowik_bridge.dylib
    [ -f "$LIB" ] || LIB=../target/release/libdomowik_bridge.so
    dart run bin/harness.dart "$LIB" 2>/dev/null | grep -q 'DART HARNESS PASSED'
  )
else
  echo "frontend Dart FFI harness: SKIPPED - dart not on PATH"
fi

stage "Linux native file choosers"
if [ "$(uname -s)" = "Linux" ] && command -v xvfb-run >/dev/null 2>&1 && \
    command -v c++ >/dev/null 2>&1 && command -v pkg-config >/dev/null 2>&1 && \
    pkg-config --exists gtk+-3.0; then
  bash scripts/check-linux-file-choosers.sh
else
  echo "Linux native file choosers: SKIPPED - needs Linux, GTK development tools and Xvfb"
fi

# THE FLUTTER APPLICATION, built and made to verify itself (issue 0024).
#
# The harnesses above load the core from an absolute path handed to them on the
# command line. An APPLICATION has no such path -- it has a bundle -- and
# "bundle packaging" was the largest thing this spike had recorded as assessed
# rather than answered. This stage builds the real app and runs two probes
# FROM INSIDE the built bundle, then checks the packaging claims directly
# rather than trusting that a successful build implies them.
#
# Skips are ANNOUNCED, never silent (ED-0087 R3): a conditional stage that
# passes quietly cannot be told apart from one that ran.
if command -v flutter >/dev/null 2>&1; then
  stage "Flutter editor package resolution"
  (cd frontend && flutter pub get)
  stage "Flutter editor analysis"
  (cd frontend && flutter analyze)
  stage "Flutter editor widget tests"
  (cd frontend && flutter test)
  stage "Flutter viewport measurement harness"
  (cd frontend && flutter test benchmark/viewport_benchmark.dart)
  stage "Flutter level-render measurement harness"
  (cd frontend && flutter test benchmark/level_render_benchmark.dart)
  stage "Flutter 3D camera measurement harness"
  (cd frontend && flutter test benchmark/camera_3d_benchmark.dart)
  # The browser editor (issue 0088): its widget workflows on the real
  # WebAssembly core and WebGPU renderer in a pinned headless Chrome for
  # Testing, then the release bundle built and probed end to end. Mandatory
  # wherever Flutter runs; the browser is downloaded and checksum-verified on
  # first use (frontend/scripts/chrome-for-testing.sh).
  stage "Flutter browser editor tests"
  (cd frontend && ./scripts/test-web.sh)
  stage "Flutter browser editor build"
  (cd frontend && ./scripts/build-web.sh)
  stage "Flutter browser editor release probe"
  (cd frontend && ./scripts/web-probe.sh)
  # The same probe on the dart2wasm/skwasm build ADR 0012 promises works.
  # skwasm loads fonts through a different path from CanvasKit, and encodes
  # their URLs itself, so this build is served under a path holding an
  # encoded space and an encoded reserved character (ED-0121).
  stage "Flutter browser editor wasm build"
  (cd frontend && ./scripts/build-web.sh --wasm --base-href '/domowik%20%23editor/')
  stage "Flutter browser editor wasm release probe"
  (cd frontend && ./scripts/web-probe.sh --base-href '/domowik%20%23editor/')
else
  echo "Flutter editor checks: SKIPPED - flutter not on PATH"
fi

stage "frontend Flutter Linux application"
if [ "$(uname -s)" = Linux ] && bash "$REPO_ROOT/scripts/check-linux-editor-prerequisites.sh" >/dev/null 2>&1; then
  stage "Linux editor prerequisite regression"
  bash "$REPO_ROOT/scripts/ci/linux-editor-prerequisites-test.sh"
  stage "frontend Flutter Linux application"
  (
    cd frontend
    flutter build linux --release
    BIN=build/linux/x64/release/bundle/domowik_editor
    test -x "$BIN"
    # Software/Xvfb probes exercise packaging and first-frame correctness,
    # not physical-device input latency or hardware GPU performance.
    DOMOWIK_SPIKE_SELFTEST=1 LIBGL_ALWAYS_SOFTWARE=1 timeout 180 xvfb-run -a "$BIN" | \
      grep -q 'BUNDLE SELFTEST PASSED'
    DOMOWIK_SPIKE_FIRSTFRAME=1 LIBGL_ALWAYS_SOFTWARE=1 timeout 180 xvfb-run -a "$BIN" | \
      grep -q 'FIRST FRAME PROBE PASSED'
  )
else
  echo "frontend Flutter Linux application: SKIPPED - needs Linux editor prerequisites"
fi

stage "frontend Flutter macOS application"
if [ "$(uname -s)" = "Darwin" ] && command -v flutter >/dev/null 2>&1; then
  (
    cd frontend
    # CLEAN, deliberately, and it costs about 25 seconds.
    #
    # An INCREMENTAL `flutter build macos --release` rewrites App.framework
    # without re-sealing the outer bundle, so `codesign --verify --deep
    # --strict` then reports "nested code is modified or invalid" on a build
    # that is otherwise fine. That is a real Flutter behaviour, not ours, and
    # it means the signature assertion below is only meaningful after a clean
    # build. Dropping the assertion to keep the stage fast would leave the one
    # packaging property nobody can check by looking.
    flutter clean >/dev/null
    flutter build macos --release >/dev/null
    APP=build/macos/Build/Products/Release/domowik_editor.app
    BIN="$APP/Contents/MacOS/domowik_editor"

    # The on-disk packaging claims -- core inside the bundle, exact @rpath
    # framework id, both architecture slices, a valid signature -- moved into
    # scripts/check-macos-bundle.sh unchanged (issue 0061). They live in their
    # own script so CI can point the SAME assertions at deliberately broken
    # bundles and watch them reject; assertions nobody has seen fail are not
    # evidence that they can. See scripts/ci/macos-bundle-negative-proof.sh.
    "$REPO_ROOT/scripts/check-macos-bundle.sh" "$APP"

    # A probe that hangs would stall the whole run rather than failing it, so
    # each gets a wall-clock ceiling. macOS ships no `timeout(1)`; perl's
    # alarm is always present.
    run_probe() {
      /usr/bin/perl -e 'alarm shift; exec @ARGV or die "exec: $!"' 180 "$@"
    }

    # The probes carry the assertions that matter -- that the loaded image
    # came from INSIDE this bundle (resolved with dladdr, not assumed), that
    # pixels were actually rasterised, and that no buffer leaked. The shell
    # checks above only establish what is on disk; on their own they would
    # pass on a bundle whose framework the process never loaded.
    DOMOWIK_SPIKE_SELFTEST=1 run_probe "$BIN" 2>/dev/null | \
      grep -q 'BUNDLE SELFTEST PASSED'
    DOMOWIK_SPIKE_FIRSTFRAME=1 run_probe "$BIN" 2>/dev/null | \
      grep -q 'FIRST FRAME PROBE PASSED'
    # The shared Rust renderer through the IOSurface presenter (issue 0101):
    # geometry drawn, upright, navigation without uploads, undo/redo reaching
    # the scene, and a bounded buffer pool across resizes.
    DOMOWIK_3D_PROBE=1 DOMOWIK_3D_ORIENTATION_PROBE=1 run_probe "$BIN" 2>/dev/null | \
      grep -q '3D GPU PROBE PASSED'
  )
else
  echo "frontend Flutter macOS application: SKIPPED - needs macOS and flutter on PATH"
fi

# Issue 0061: this script's OWN failure-reporting contract, exercised under
# every bash on the machine. It is a stage rather than a CI-only step because
# the defect it guards is platform-specific -- bash 3.2 on macOS -- and the
# people most likely to hit it are running verify.sh on a laptop, not in CI.
# It announces its own coverage gap on hosts with no bash 3.x.
stage "failure banner regression"
./scripts/ci/verify-banner-regression.sh

stage "web browser launcher regression"
if [ "$(uname -s)" = Linux ]; then
  ./frontend/scripts/test-run-web-browser.sh
else
  echo "web browser launcher regression: SKIPPED - Linux-only launcher"
fi

# P13: full-run clean-tree assertion
stage "P13 clean tree"
tree_status=$(git status --porcelain)
if [ -n "$tree_status" ]; then
  printf 'Verification left a dirty tree (or started with one):\n%s\n' "$tree_status" >&2
  git diff --stat >&2
  # Lockfile changes distinguish resolver/toolchain drift from build output.
  # Keep the gate fail-closed; do not restore or ignore changed inputs.
  git diff -- frontend/pubspec.lock 'frontend/*/pubspec.lock' \
    frontend/macos/Podfile.lock >&2
  false
fi

printf "\n=== VERIFICATION PASSED === all stages\n"
