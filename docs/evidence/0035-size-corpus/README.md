# Issue 0035 Part A size-corpus measurements (evidence-only)

Status: **Evidence** under the Performance Policy and ASSET-STRATEGY §10.9.
These records are measured facts about one environment and one writer; they
are not correctness gates, carry no normative weight, and are fully
separated from the deterministic test suite. Generator version:
`0035-part-a.1` (`internal/sizecorpus`).

## What is recorded

| File | Content |
| --- | --- |
| `budget-empty-v1.json`, `budget-unfurnished-1000-v1.json` | Measured-budget records per scenario: preview-inclusive headline bytes vs strict targets (16 KiB / 256 KiB), per-entry-class breakdown (manifest, descriptors, snapshot, history explicitly none, preview index/data, container overhead), external byte totals excluded from the headline, versions, environment, raw-output pointers |
| `preview-inventory-empty-v1.json`, `preview-inventory-unfurnished-1000-v1.json` | Preview-record inventories: thumbnail digest, byte count, 512×512 dimensions, encoder identity incl. zlib runtime version and ladder-reduction count |
| `encoding-candidates.json` | All five promotion-gate candidates (WebP, AVIF, KTX2/Basis Universal, glTF/GLB + meshoptimizer, font subsetting) recorded NOT MEASURED this session with reasons; no candidate is promoted or becomes a format requirement |
| `packages/*.domo` | The measured thin packages themselves (byte-deterministic outputs of `persist.BuildPackage`) |
| `raw/` | Raw machine-generated outputs: environment facts, python3/zlib identity, Go version |

## Measured results (this environment)

| Scenario | Headline (preview-inclusive) | Target | Verdict |
| --- | ---: | ---: | --- |
| `empty-v1` | 3,597 B | < 16,384 B | strict pass |
| `unfurnished-1000-v1` | 119,394 B | < 262,144 B | strict pass |

History cost is zero by construction: Part A packages carry a complete
snapshot and zero retained operation entries; history framing belongs to
Part B of §10.9 and is reported as its own (empty) class, never folded into
headline budgets.

## Reproduction command

```sh
DOMOWIK_WRITE_0035_RECORDS=1 GOTOOLCHAIN=local go test ./internal/sizecorpus -run TestGatedRecordGeneration -count=1
```

The gated writer refuses to overwrite records whose budget verdict is not a
strict pass (ED-0024: any measurement at or above target demands a recorded
budget revision, never a silent pass). Deterministic tests
(`go test ./internal/sizecorpus`) re-validate committed digests, accounting
identities, round-trips, and regeneration byte-equality whenever the
reference encoder is available; they degrade to loud skips — never silent
acceptance — on machines without python3.

## Environment of record

Linux 7.1.8-arch1-3, AMD Ryzen 9 7950X 16-Core Processor (32 logical CPUs),
MemTotal 64,853,676 KiB, workdir filesystem btrfs, go1.27.0 linux/amd64,
GOTOOLCHAIN=local. Measurements name this actual environment only; nothing
here generalizes to reference hardware.

## Provenance ledger (dev-only tools; ledger-before-reliance)

- **python3 3.14.7-1** (PSF-2.0) — dev-only subprocess providing the
  reference thumbnail DEFLATE through its unmodified standard library;
  never distributed with Domowik; not a Go module dependency. Recorded in
  `docs/third-party/tools.json`.
- **zlib runtime 1.3.2** — effective implementation behind that stdlib;
  artifact and verbatim license already recorded in ASSET-STRATEGY §10.10.
  The §10.9 pinned PNG vector (1809 B,
  SHA-256 `49c8c56d509296669f01cf18365984d4b4a3920b9da04b137eb0443d6eefe29f`)
  hard-binds acceptable output: environments whose DEFLATE differs fail the
  check loudly. Go's own `compress/zlib` does NOT reproduce the vector
  (measured 1991 B, different digest), which is why the stdlib-zlib
  subprocess seam exists.
- No other new tools, dependencies, modules, or bundled assets were added.
  Container compression uses Go stdlib `compress/flate`; container layout
  and writer behavior come from the delivered issue 0026 seams unchanged.

## Spec basis and provisional interpretations

Construction follows ASSET-STRATEGY §10.9 with two ambiguities resolved by
owner decision **ED-0025** (EXECUTIVE-DECISIONS.md; provisional pending an
issue 0021 amendment):

1. Openings: ring instances are numbered globally `g = (level−1)·10 + r`;
   each `g ∈ 0..38` carries one door on the middle sub-wall of each edge
   (bottom, right, top, left order), yielding the normative 156 = 39 × 4.
   Opening identity/order is level, ring, edge.
2. Style registry: the one-entry document style registry is a
   generator-local corpus vocabulary `{S0}`, not a wire object; walls and
   openings serialize `styleRefs:["S0"]`; rooms omit optional style fields;
   the self-check resolves every token against the local vocabulary.

If issue 0021 later amends §10.9 differently, these corpus and measurements
must be regenerated; no compatibility claim attaches to this provisional
corpus. Additional generator-owned conventions (recorded in
`Definition.CanonicalJSON`, whose sha256 binds every budget record):
clockwise ring traversal bottom→right→top→left fixing wall Start/End and
join roles; sub-wall splits at exactly one third / two thirds along each
edge's traversal; entity path grammar `level/<l>`,
`wall/<l>/<r>/<edge>/<k>`, `join/<l>/<r>/<edge>/<k>`,
`opening/<l>/<r>/<edge>`, `room/<l>/<r>`; identity tags =
SHA-256(seedBytes ‖ ASCII(path)) first 16 bytes under the FORMAT-SPEC §2.1
E4 UUIDv8/variant layout; operation/batch/actor ids derived analogously
(`op/create/<entityPath>`, `batch/create`, `actor/create`). Level
elevations are 3000·(l−1) mm (Level 1 at 0 mm, consistent with empty-v1).
The display-only timestamp literal has no field in Part A wire data.

## Reproducibility tier

Tier R holds within the pinned environment (seeds, RNG vector, and
reference-PNG vector verified). Tier W holds for same-toolchain runs of the
delivered writer: regenerated packages are byte-identical to the committed
artifacts (enforced by tests whenever python3 is available). No claim is
made across writers, containers, DEFLATE implementations, or environments
whose vectors fail.

## Publication handoff and dormancy

Measured budgets feed issue 0018's acceptance criterion ("Size corpus and
measured thin-package budgets are published"); 0018 must add the documented
0035 dependency when next activated (issue 0035 Closure section). The
`furnished-cached-v1` scenario stays dormant behind BEHAVIOR-SPEC §10
acceptance; no furniture semantics exist anywhere in this work.
