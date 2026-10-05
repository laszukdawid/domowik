#!/usr/bin/env bash
# Exercise the issue-0036 release inventory against deliberately broken trees.
#
# A fail-closed gate nobody has watched reject anything is not evidence that it
# can reject anything. This is the same argument
# scripts/ci/macos-bundle-negative-proof.sh makes for the packaging assertions,
# and it applies with more force here: every check in
# scripts/release-validate.py passes on a healthy tree by doing nothing.
#
# Each case copies the tracked tree into a scratch directory, breaks exactly
# ONE thing, and asserts the validator refuses WITH THE EXPECTED TAG. Asserting
# only that it exited non-zero would pass for a validator that crashed on
# startup, which is how a gate quietly stops gating.
#
# Usage: release-inventory-test.sh [work dir]
# Requires cargo and python3 on PATH. Nothing builds or publishes. The pinned
# spike-only licence fixture may fetch ryu 1.0.23 if absent from Cargo's cache.
set -euo pipefail

REPO_ROOT=$(cd -P "$(dirname "$0")/../.." && pwd -P)
WORK=${1:-$(mktemp -d "${TMPDIR:-/tmp}/domowik-release-test.XXXXXX")}
mkdir -p "$WORK"

# A fixed epoch, so the documents under test do not depend on the copy having
# git history, and so the SOURCE_DATE_EPOCH branch is the one exercised.
export SOURCE_DATE_EPOCH=1700000000

PASSED=0
FAILED=0

pass() { printf '  ok      %s\n' "$1"; PASSED=$((PASSED + 1)); }
fail() { printf '  FAILED  %s\n     %s\n' "$1" "$2"; FAILED=$((FAILED + 1)); }

# A pristine copy of the tracked tree plus anything not yet committed.
#
# Tracked files only, listed explicitly: a recursive copy would drag in
# target/ and build/, and the global repo rule against blanket staging applies
# to the `git add` below too. The copy gets its own git repository because
# `scan_embeds` reads `git ls-files` on purpose -- an untracked stray .rs file
# is not part of the product and must not be scanned as though it were.
fixture() {
  local dest="$1"
  rm -rf "$dest"
  mkdir -p "$dest"
  # Tracked files plus anything not yet committed, minus bytecode caches.
  ( cd "$REPO_ROOT" && git ls-files -co --exclude-standard ) \
    | grep -v '__pycache__' > "$WORK/filelist"
  ( cd "$REPO_ROOT" && tar -cf - -T "$WORK/filelist" ) | ( cd "$dest" && tar -xf - )
  (
    cd "$dest"
    git init -q .
    # Disposable fixtures must not spawn maintenance that outlives a case
    # and races with the next fixture's removal (hosted Git enables it).
    git config maintenance.auto false
    git config gc.auto 0
    git config user.email "test@invalid"
    git config user.name "release inventory test"
    # Explicit source paths from the same list, never a wildcard sweep or
    # forced inclusion of ignored build output.
    tr '\n' '\0' < "$WORK/filelist" | xargs -0 git add --
    git commit -q -m "fixture"
  )
}

# run_case <name> <expected tag|@pass> <mutation function> [validator args...]
run_case() {
  local name="$1" expect="$2" mutate="$3"; shift 3
  local dir="$WORK/case"
  fixture "$dir"
  "$mutate" "$dir"
  local out rc=0
  out=$(cd "$dir" && python3 scripts/release-validate.py "$@" 2>&1) || rc=$?
  if [ "$expect" = "@pass" ]; then
    if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'RELEASE VALIDATION PASSED'; then
      pass "$name"
    else
      fail "$name" "expected a clean pass, got exit $rc: $(printf '%s' "$out" | tail -3 | tr '\n' ' ')"
    fi
    return
  fi
  if [ "$rc" -eq 0 ]; then
    fail "$name" "the validator PASSED a tree broken on purpose (expected $expect)"
  elif printf '%s' "$out" | grep -q "REFUSED $expect"; then
    pass "$name"
  else
    fail "$name" "exited $rc but never printed 'REFUSED $expect': $(printf '%s' "$out" | tail -3 | tr '\n' ' ')"
  fi
}

nothing() { :; }

break_embed() {
  mkdir -p "$1/crates/domowik-cli/assets"
  printf 'not a real asset' > "$1/crates/domowik-cli/assets/undeclared.bin"
  printf '\nconst SMUGGLED: &[u8] = include_bytes!("../assets/undeclared.bin");\n' \
    >> "$1/crates/domowik-cli/src/label.rs"
  ( cd "$1" && git add -- crates/domowik-cli/assets/undeclared.bin crates/domowik-cli/src/label.rs && git commit -q -m break )
}

break_asset_digest() {
  python3 - "$1" <<'PY'
import json, sys
path = sys.argv[1] + "/docs/third-party/assets.json"
record = json.load(open(path, encoding="utf-8"))
record["assets"][0]["sha256"] = "0" * 64
json.dump(record, open(path, "w", encoding="utf-8"), indent=2, ensure_ascii=False)
PY
}

break_allowlist() {
  # Remove one reviewed licence SET and the crates declaring it must be refused.
  # This exercises the comparison from the side a test can reach: no crate in
  # the registry can be made to declare something new without editing a crate
  # nobody here owns.
  #
  # ALL THREE SPELLINGS GO. Since the guard compares by licence set rather than
  # by string, deleting only "MIT OR Apache-2.0" leaves "Apache-2.0 OR MIT" and
  # "MIT/Apache-2.0" admitting the identical set, and the mutation stops
  # mutating anything. That is exactly what happened when the canonicalisation
  # landed, and this test caught it.
  python3 - "$1" <<'PY'
import sys

path = sys.argv[1] + "/scripts/rust-deps-inventory.py"
text = open(path, encoding="utf-8").read()
removed = 0
for spelling in ('    "MIT OR Apache-2.0",\n',
                 '    "Apache-2.0 OR MIT",\n',
                 '    "MIT/Apache-2.0",\n'):
    assert spelling in text, spelling
    text = text.replace(spelling, "", 1)
    removed += 1
assert removed == 3
open(path, "w", encoding="utf-8").write(text)
PY
}
break_toolchain_pin() {
  python3 - "$1" <<'PY'
import json, sys
path = sys.argv[1] + "/docs/third-party/toolchain-rust.json"
record = json.load(open(path, encoding="utf-8"))
record["channelPin"] = "1.0.0"
json.dump(record, open(path, "w", encoding="utf-8"), indent=2, ensure_ascii=False)
PY
}

break_target_gap() {
  python3 - "$1" <<'PY'
import json, sys
path = sys.argv[1] + "/release/rust-manifest.json"
record = json.load(open(path, encoding="utf-8"))
record["artifacts"][0]["targets"].append("riscv64gc-unknown-linux-gnu")
json.dump(record, open(path, "w", encoding="utf-8"), indent=2, ensure_ascii=False)
PY
}

break_empty_manifest() {
  python3 - "$1" <<'PY'
import json, sys
path = sys.argv[1] + "/release/rust-manifest.json"
record = json.load(open(path, encoding="utf-8"))
record["artifacts"] = []
json.dump(record, open(path, "w", encoding="utf-8"), indent=2, ensure_ascii=False)
PY
}

break_notice() { printf '\nan edit nobody generated\n' >> "$1/NOTICE"; }
break_inventory() { printf '\n| smuggled | 9.9 | runtime | MIT |\n' >> "$1/docs/RUST-DEPENDENCIES.md"; }

echo "=== release inventory: fail-closed proofs ==="
run_case "an undeclared embedded asset is refused" undeclared-embed break_embed
run_case "a changed asset digest is refused" stale-asset-row break_asset_digest
run_case "a licence off the allow-list is refused" unreviewed-licence break_allowlist
run_case "a toolchain pin that drifted is refused" toolchain-drift break_toolchain_pin
run_case "a target with no rust-std row is refused" target-gap break_target_gap
run_case "an empty artifact set is refused" empty-artifact-set break_empty_manifest
run_case "a hand-edited NOTICE is refused" notice-stale break_notice
run_case "a hand-edited dependency inventory is refused" inventory-stale break_inventory
run_case "an unbuilt artifact is refused under --require-built" \
  missing-artifact nothing --require-built --artifact domowik-cli
run_case "the staged GUI bundle is refused under --require-built" \
  incomplete-coverage nothing --require-built --artifact domowik-gui-macos

# --skip-generated exists for scripts/verify.sh, which has already regenerated
# and diffed NOTICE and the dependency inventory. These two pin what it gives
# up and what it must not: a flag that quietly widened to other checks would
# hand verify.sh a weaker gate than the one it thinks it is running.
run_case "--skip-generated still refuses an undeclared embed" \
  undeclared-embed break_embed --skip-generated

run_case "--skip-generated does drop the NOTICE check, which is its whole job" \
  @pass break_notice --skip-generated --artifact domowik-cli

echo
echo "=== licence guard: spelling, not semantics, was the failure mode ==="
if ( cd "$REPO_ROOT" && python3 - <<'LICPY'
import importlib.util, sys

spec = importlib.util.spec_from_file_location("inv", "scripts/rust-deps-inventory.py")
inv = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inv)

# Word order carries no meaning in an SPDX disjunction, so the same licence
# set spelled differently must be admitted. Found against the wgpu graph by
# the issue-0025 worker: two of its three refusals were spelling alone.
for expression in ("MIT OR Apache-2.0 OR Zlib",      # raw-window-handle 0.6.2
                   "Apache-2.0/MIT",                 # rustc-hash 1.1.0
                   "Apache-2.0 OR MIT OR Zlib",
                   "MIT OR Unlicense"):
    assert inv.is_allowed(expression), "reordered but reviewed set refused: %s" % expression

# Nothing new is admitted by the CANONICALISATION. Copyleft stays refused
# outright.
#
# ISC used to be asserted here as refused, and that assertion was correct until
# root reviewed it (R6, 2026-09-12). It is now owned positively by the ISC
# block below rather than deleted, so admitting a licence still has to be
# stated somewhere a reader will find it. This test failing on the day ISC was
# approved is the suite working: an allow-list change that no test noticed
# would be the thing to worry about.
for expression in ("GPL-3.0-only", "MPL-2.0", "AGPL-3.0-or-later",
                   "LGPL-2.1-or-later", "MIT OR GPL-3.0-only"):
    assert not inv.is_allowed(expression), "admitted an unreviewed licence: %s" % expression

# A parenthesised or mixed expression is compared EXACTLY, because reordering
# around an AND is not something this normalisation is entitled to reason about.
assert inv.is_allowed("(MIT OR Apache-2.0) AND Unicode-3.0")
assert not inv.is_allowed("(Apache-2.0 OR MIT) AND Unicode-3.0")
sys.exit(0)
LICPY
); then
  pass "reordered spellings of a reviewed licence set are admitted"
  pass "unreviewed licences and copyleft are still refused"
  pass "parenthesised expressions are still compared exactly"
else
  fail "licence guard canonicalisation" "the assertions above did not hold"
fi

echo
echo "=== licence guard: ISC admitted under R6, copyleft still refused ==="
if ( cd "$REPO_ROOT" && python3 - <<'ISCPY'
import importlib.util, os, sys, tempfile

spec = importlib.util.spec_from_file_location("inv", "scripts/rust-deps-inventory.py")
inv = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inv)

# POSITIVE. Root approved ISC on 2026-09-12 (R6) as a bounded dependency
# review against the pinned upstream LICENSE and Cargo.toml for libloading
# 0.8.9. Technical project-policy acceptance, not legal clearance.
assert inv.is_allowed("ISC"), "ISC was approved under R6 and must be admitted"

# NEGATIVE. Admitting one permissive licence must not loosen anything else.
# MPL-2.0 is here deliberately: ADR 0003 names `wgpu-native`, and the
# crates.io package of that name is MPL-2.0, so this is the trap a reader
# following the ADR literally would fall into.
for expression in ("GPL-3.0-only", "GPL-2.0-or-later", "LGPL-2.1-or-later",
                   "AGPL-3.0-or-later", "MPL-2.0", "EPL-2.0", "CDDL-1.0",
                   "ISC OR GPL-3.0-only", "ISC AND GPL-3.0-only"):
    assert not inv.is_allowed(expression), "copyleft admitted: %s" % expression

sys.exit(0)
ISCPY
); then
  pass "ISC is admitted under R6"
  pass "copyleft and mixed copyleft expressions are still refused"
else
  fail "ISC admission (R6)" "the assertions above did not hold"
fi

# The real crate, when the local registry happens to hold it. ANNOUNCED when
# skipped rather than passing in silence (ED-0087 R3): a conditional check that
# skips quietly cannot be told apart from one that ran.
ISC_CRATE=$(ls -d "$HOME"/.cargo/registry/src/*/libloading-0.8.9 2>/dev/null | head -1 || true)
if [ -n "$ISC_CRATE" ] && [ -f "$ISC_CRATE/LICENSE" ]; then
  if grep -q 'permission notice appear' "$ISC_CRATE/LICENSE" &&
     grep -q '^license = "ISC"' "$ISC_CRATE/Cargo.toml"; then
    pass "libloading 0.8.9 ships an ISC LICENSE carrying the permission notice"
  else
    fail "libloading 0.8.9 ISC text" "the cached crate does not match the reviewed licence"
  fi
else
  echo "  SKIPPED libloading 0.8.9 is not in the local registry cache;"
  echo "          the ISC text check needs the crate present"
fi

# THE NOTICE OBLIGATION, ASSERTED ON GENERATOR OUTPUT AND MUTATION-PROVEN.
#
# The first version of this called `licence_files()` and then read the file
# back itself. It never invoked the generator's output path, so deleting the
# licence-text emission from scripts/rust-notice.py left it green. Root caught
# that (R18). A test that passes when the behaviour it names is removed is not
# evidence of anything.
#
# What runs now: the real generator, into a real NOTICE, asserted on the
# copyright and permission text, and then the emission is deliberately removed
# to watch the assertion go RED before it is restored.
echo
echo "=== notice generation: asserted on output, and proven able to fail ==="
NDIR="$WORK/notice"
fixture "$NDIR"
if ( cd "$NDIR" && python3 - <<'NOTICEPY'
import json, os, subprocess, sys

ISC_TEXT = (
    "Copyright © 2015, Simonas Kazlauskas\n\n"
    "Permission to use, copy, modify, and/or distribute this software for any "
    "purpose with or without\nfee is hereby granted, provided that the above "
    "copyright notice and this permission notice appear\nin all copies.\n"
)


def generate():
    proc = subprocess.run([sys.executable, "scripts/rust-notice.py"],
                          capture_output=True, text=True)
    assert proc.returncode == 0, proc.stderr
    return proc.stdout


# ---- the CRATE path, which is the one an ISC dependency will travel ----
before = generate()

# THE ANCHOR IS DERIVED FROM THE OUTPUT, not hardcoded. The first draft used
# "Permission is hereby granted, free of charge", which opens the MIT licence
# AND the OFL. The bundled Noto asset kept that phrase alive through the
# mutation, so the test passed where it should have failed. Taking the first
# body line under the first licence header binds the assertion to the crate
# path specifically and cannot go stale as the crate set changes.
lines = before.splitlines()
header = next(i for i, l in enumerate(lines) if l.startswith("--- LICEN"))
anchor = next(l for l in lines[header + 1:] if l.strip())

emission = '''            with open(path, encoding="utf-8", errors="replace") as handle:
                out.append(handle.read().rstrip())
            out.append("")'''
src = open("scripts/rust-notice.py", encoding="utf-8").read()
assert emission in src

# try/finally, because the first draft left the generator gutted when an
# assertion failed and poisoned every later check in this fixture.
try:
    open("scripts/rust-notice.py", "w", encoding="utf-8").write(
        src.replace(emission, '            out.append("")', 1))
    after = generate()
    assert anchor not in after, (
        "removing the crate emission did NOT drop %r from the generated "
        "NOTICE; the assertion above cannot fail and proves nothing" % anchor)
    assert len(after) < len(before) / 2, (
        "gutting the emission barely changed the output: %d -> %d bytes"
        % (len(before), len(after)))
finally:
    open("scripts/rust-notice.py", "w", encoding="utf-8").write(src)
assert generate() == before, "restoring the emission did not restore the output"

# ---- the ASSET path, carrying real ISC text through the real generator ----
os.makedirs("docs/third-party/testdata", exist_ok=True)
with open("docs/third-party/testdata/ISC.txt", "w", encoding="utf-8") as fh:
    fh.write(ISC_TEXT)
record = json.load(open("docs/third-party/assets.json", encoding="utf-8"))
record["assets"].append({
    "name": "ISC fixture", "version": "0.0.0",
    "path": "docs/third-party/testdata/ISC.txt",
    "license": "ISC",
    "licenseTextPath": "docs/third-party/testdata/ISC.txt",
    "status": "distributed", "modified": False,
})
json.dump(record, open("docs/third-party/assets.json", "w", encoding="utf-8"),
          indent=2, ensure_ascii=False)

with_isc = generate()
def fixture_notice(text):
    # Runtime crates may carry the same ISC text. Assert only this asset's
    # section so their retained notices cannot hide this fixture's omission.
    section = text.split("ISC fixture 0.0.0\n", 1)[1]
    return section.split("========================================================================", 1)[0]

with_isc_asset = fixture_notice(with_isc)
assert "Copyright © 2015, Simonas Kazlauskas" in with_isc_asset, \
    "the ISC copyright notice is absent from the generated NOTICE"
assert "this permission notice appear" in with_isc_asset, \
    "the ISC permission notice is absent from the generated NOTICE"
assert "Declared licence: ISC" in with_isc_asset

# Omission must go red: drop the text pointer and the notice must lose the text
# AND say so, rather than silently omitting it.
record["assets"][-1].pop("licenseTextPath")
json.dump(record, open("docs/third-party/assets.json", "w", encoding="utf-8"),
          indent=2, ensure_ascii=False)
omitted = generate()
omitted_asset = fixture_notice(omitted)
assert "this permission notice appear" not in omitted_asset, \
    "the ISC text survived removal of its licenceTextPath; the assertion " \
    "above cannot fail"
assert "No licence text is committed beside this asset" in omitted_asset
sys.exit(0)
NOTICEPY
); then
  pass "generated NOTICE carries crate licence text, and loses it when the emission is removed"
  pass "generated NOTICE carries the ISC copyright and permission notice"
  pass "omitting the ISC text turns the assertion red and is reported, not silent"
else
  fail "notice generation (R18)" "the generator-output assertions did not hold"
fi

# THE ALL-ARTIFACT LICENCE PATH, MUTATION-PROVEN AGAINST A SPIKE-ONLY CRATE.
#
# `--artifact domowik-cli` filters the artifact list, so the licence audit runs
# ONLY over the root workspace graph and never reaches the 0024 spike, which is
# its own workspace. The verify.sh stage claimed otherwise until root caught it
# (R19). These two cases pin the real behaviour of both invocations.
#
# A spike-only dependency has to be MANUFACTURED, because none exists: the
# spike's linked closure is a strict subset of the CLI's (34 against 57, set
# difference empty, measured). So the fixture adds a REGISTRY crate carrying an
# unreviewed licence expression to the spike workspace alone.
break_spike_only_licence() {
  # Add a REGISTRY crate with an unreviewed licence to the spike workspace only.
  #
  # `ryu` 1.0.23 declares "Apache-2.0 OR BSL-1.0", which is not on the reviewed
  # allow-list, and it is in no closure here: serde_json switched to `zmij`, so
  # neither the CLI nor the spike pulls it. A fresh CI runner need not have this
  # historical version cached, so lock resolution may fetch it. The exact pin
  # is fixture-only: none of its code is built or distributed with Domowik.
  #
  # IT MUST BE A REGISTRY CRATE. The first draft created a local crate and took
  # a path dependency on it, which proved nothing: `check_licences` skips any
  # package whose `source` is null, so a path dependency is treated as
  # first-party and is never audited. That is a real blind spot, reported
  # separately; it is not what this case tests.
  python3 - "$1" <<'SPIKEPY'
import sys
path = sys.argv[1] + "/frontend/native/Cargo.toml"
text = open(path, encoding="utf-8").read()
anchor = "[dependencies]\n"
assert anchor in text
open(path, "w", encoding="utf-8").write(
    text.replace(anchor, anchor + 'ryu = "=1.0.23"\n', 1))
SPIKEPY
  # The generators run `cargo metadata --locked`, so the spike's lockfile has to
  # learn about the crate. Try offline first, then permit the pinned fixture
  # fetch; do not assume the maintainer's cache exists on a clean CI runner.
  ( cd "$1/frontend" &&
    { cargo generate-lockfile --offline >/dev/null 2>&1 || cargo generate-lockfile; } )
}

echo
echo "=== licence audit: which invocation actually reaches the spike ==="
run_case "an unreviewed spike-only licence is refused by the all-artifact audit" \
  unreviewed-licence break_spike_only_licence --skip-generated

run_case "the same tree PASSES under --artifact domowik-cli, which never looks at the spike" \
  @pass break_spike_only_licence --skip-generated --artifact domowik-cli

echo
echo "=== release inventory: the healthy tree still passes ==="
run_case "an unmodified tree validates" @pass nothing

echo
echo "=== release inventory: determinism and release-grade ==="
DET="$WORK/det"
fixture "$DET"
(
  cd "$DET"
  for mode in "source" "artifact --id domowik-cli"; do
    # shellcheck disable=SC2086
    python3 scripts/release-sbom.py $mode > "$WORK/run1.json"
    # shellcheck disable=SC2086
    python3 scripts/release-sbom.py $mode > "$WORK/run2.json"
    if cmp -s "$WORK/run1.json" "$WORK/run2.json"; then
      printf '  ok      two runs of `%s` are byte-identical\n' "$mode"
    else
      printf '  FAILED  `%s` is not deterministic\n' "$mode"
      exit 1
    fi
  done
) || FAILED=$((FAILED + 1))
PASSED=$((PASSED + 2))

rc=0
( cd "$DET" && python3 scripts/release-sbom.py artifact --id domowik-gui-macos \
    --target aarch64-apple-darwin --release-grade >/dev/null 2>"$WORK/grade.err" ) || rc=$?
if [ "$rc" -ne 0 ] && grep -q 'release-grade refused' "$WORK/grade.err"; then
  pass "--release-grade refuses the incomplete GUI inventory"
else
  fail "--release-grade refuses the incomplete GUI inventory" \
    "exit $rc, stderr: $(tr '\n' ' ' < "$WORK/grade.err")"
fi

rc=0
( cd "$DET" && python3 scripts/release-sbom.py artifact --id nonexistent \
    >/dev/null 2>"$WORK/unknown.err" ) || rc=$?
if [ "$rc" -ne 0 ] && grep -q 'no artifact' "$WORK/unknown.err"; then
  pass "an undeclared artifact id is refused, not invented"
else
  fail "an undeclared artifact id is refused, not invented" "exit $rc"
fi

echo
if [ "$FAILED" -ne 0 ]; then
  echo "=== RELEASE INVENTORY TESTS FAILED === $FAILED failed, $PASSED passed"
  exit 1
fi
echo "=== RELEASE INVENTORY TESTS PASSED === $PASSED checks"
