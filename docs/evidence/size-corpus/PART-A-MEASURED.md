# Part A size corpus, measured (issues 0035, 0060)

ROADMAP Phase 1 exits on "size targets are measured and published". These are
the measurements. Produced by `crates/domowik-cli/tests/size_corpus.rs`, which
builds both ASSET-STRATEGY §10.9 Part A scenarios to their normative
construction, writes real `.domo` packages, reopens them, and asserts the
published budgets on every test run.

Host: Apple Silicon (macOS 26.6.2, Darwin 25.6.0, arm64), Rust 1.98.0, `zip`
with Deflate.

## Measured

| Scenario | Entities | Packaged | Target | Estimate |
|---|---:|---:|---:|---:|
| `empty-v1` | 1 | **471–474 B** (0.5 KiB) | < 16 KiB | ≤ 10 KiB |
| `unfurnished-1000-v1` | 1,000 | **20,486–20,570 B** (20.0 KiB) | < 256 KiB | 67.6–131.1 KiB |

Both are inside target. The second is **3.3× under the low end of its
estimate**, and that gap is worth explaining rather than celebrating.

### Why these are ranges

Entity identifiers are UUIDv7 and differ on every run, so Deflate sees
different bytes each time and the packaged size moves by a few tens of bytes.
The ranges above are the spread over six consecutive runs on one host. The
earlier revision of this document published a single figure, which was one
sample presented as if it were exact; the scenario's geometry is
formula-determined and repeatable, its identifiers are not.

## Room records are now included, and they were the missing forty

`unfurnished-1000-v1` specifies 1,000 entities: 4 levels, 480 walls, 320
joins, 156 openings, and 40 room-metadata records. Until issue 0060 the
records did not exist — BEHAVIOR-SPEC §5.2 was unimplemented, rooms were
derived geometry with nothing persisted — so this file measured 960 entities
and said so, and `unfurnished-1000-v1` stood as a lower bound rather than a
measurement.

**That caveat is discharged.** The records are built to §10.9: one per ring
instance, seeded at the ring centre `(15000·(r mod 5) + 6000,
12000·(r div 5) + 4500)`, carrying `{levelId, seedMm}` with the optional
display name and both style references absent. A separate test projects every
one of them and asserts it lands `associated` with its own ring, so the forty
rows measure the specified scenario rather than forty rows of about the right
size.

The earlier estimate of the shortfall was close. It guessed "roughly 7 KiB
before compression and under 1 KiB after". Measured, the forty records add
4,851 B uncompressed (188,363 B to 193,214 B, which is exact because UUIDs are
fixed-width) and roughly 700–800 B packaged, a delta of the same order as the
run-to-run spread above and therefore quoted loosely on purpose.

## Why the estimate was pessimistic

§10.9's budget was built on two assumptions: 150–250 bytes per entity
uncompressed, and a compression factor of 2.0–2.5×.

| Assumption | Assumed | Measured |
|---|---|---|
| Uncompressed payload | 150–250 B/entity | **193 B/entity** — inside the range |
| Compression factor | 2.0–2.5× | **9.4×** |

The payload assumption was accurate. The compression assumption was not, and
the reason is structural: the document is a JSON array of entities that differ
only in a handful of coordinates, so Deflate's window sees the same field names
and the same value shapes repeatedly. A general-purpose 2.0–2.5× figure is what
you assume for mixed content; highly repetitive structured JSON does much
better.

Both assumptions are asserted by the test, so if either moves the build says
so. That matters more than the totals: a number that beats its budget for a
reason nobody wrote down is not evidence, it is luck.

## What is NOT measured

- **The `furnished-cached-v1` scenario**, which §10.9 keeps dormant pending
  BEHAVIOR-SPEC §10 acceptance and issue 0004.
- **Part B history framing.** These measure a package with an empty operation
  log. A plan built by 1,000 CLI commands carries 1,000 batches, and `compact`
  folds them away; the size of a package mid-history is not measured here.
- **Encoding benchmarks** behind §10.9's promotion gate. The compression factor
  above is a measurement of the shipped encoder, not a comparison of
  candidates.
