# 0014: Implement CLI `new`, `info`, and `validate`

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0037, 0010, 0011, 0013, 0026, 0029

## Current disposition — 2026-10-05

Keep partial: CLI workflows and separate warnings are delivered, but the
complete validation contract is not.

`crates/domowik-cli/src/lib.rs` has Report.warnings and WarningReport; the old
'errors only' claim is superseded. The exit module still defines only 0/1/2.
Finish strict/compatibility-aware mode, six-way exit taxonomy and report
coverage for asset digests; preserve existing stdout/stderr probes.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Original problem

Create documents, inspect schema/entity/asset summaries, and validate logical,
archive, asset, and derived-geometry integrity without destructive rewrites.

## Acceptance criteria

- [ ] Validation has strict and compatibility-aware modes and distinguishes
      errors from warnings.
- [ ] Human and machine-readable output identifies paths/entity IDs/digests.
- [ ] Exit codes cover valid, invalid, unsupported version, I/O, usage, and
      internal failure.
- [ ] Unknown optional data follows read-only/opaque preservation rules.
- [ ] Resource-limit and hostile-container failures are reported safely.

## Progress — 2026-08-30 (Rust core)

Partially delivered by the Rust rewrite. See
`docs/decisions/0008-rust-core-implementation.md`.

Delivered: `new`, `info`, `validate`, plus `add-wall`, `add-opening`,
`remove-wall`, `remove-opening`. Stream and exit-code discipline is enforced by
verify.sh probes P1-P5 (0 success/stdout, 2 usage/stderr, 1 failure/stderr).

Not delivered: `apply` (scripted edits), `ops`, `compact`, asset commands.
Those depend on the operation log, which is not ported.

## Progress — 2026-08-31 (machine-readable output)

`info --json` and `validate --json` emit a report identifying levels, walls,
openings and rooms by UUID as well as by the 1-based index the human report and
the editing commands use. Rooms carry their canonical boundary rings, so a
caller can draw the plan without re-deriving geometry.

`validate --json` on an invalid plan prints the report to STDOUT and still
exits non-zero. A caller that asked for machine-readable output wants the
problems; an exit code alone does not carry them, and putting the report on
stderr would make it indistinguishable from a command failure.

The report also surfaces operations replay discarded as STALE. FORMAT-SPEC I5
makes those a projection outcome rather than an error, so they are silent to a
user by design — but a script that cares must be able to see them, and the
human report cannot show them without misrepresenting them as problems.

### Acceptance criteria

- [x] Human and machine-readable output identifies paths/entity IDs/digests.
      Paths and entity ids, yes. **Digests, no** — nothing in a package is
      content-addressed yet; that arrives with the asset store (issue 0026).
- [ ] Validation has strict and compatibility-aware modes and distinguishes
      errors from warnings. One mode, errors only. FORMAT-SPEC §4.3 defines a
      warning class (duplicate level elevations, ceiling below wall height,
      parallel area overlap) and none of it is implemented, so there is nothing
      to distinguish yet.
- [ ] Exit codes cover valid, invalid, unsupported version, I/O, usage, and
      internal failure. Three codes: 0, 1 (failure), 2 (usage). Unsupported
      version, I/O and internal failure all collapse into 1.
- [x] Unknown optional data follows read-only/opaque preservation rules.
      Delivered under issue 0010 on the same date; §8 U2's preservation branch.
- [x] Resource-limit and hostile-container failures are reported safely.
      L6, L23 and per-entry size caps are enforced before allocation, and the
      reader is capped so a lying header cannot outrun the limit.

## Status set by ED-0095 — 2026-09-01

**In Progress (partial scope).** `new`, `info`, `validate` and machine-readable output delivered. NOT delivered: a warning class distinct from errors, digests in the report, and the six-way exit-code taxonomy — three codes exist.
