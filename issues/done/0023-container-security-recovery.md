# 0023: Define container threat model and recovery contract

- **Phase:** 0
- **Priority:** High
- **Status:** Done
- **Depends on:** 0021

## Problem

Define untrusted-input limits and the observable atomic-save/recovery contract
before selecting or implementing ZIP/another container.

## Acceptance criteria

- [x] Limits cover expanded and entry bytes, count, names/paths, depth,
      decompression ratio, numeric magnitude, operations, image/model
      complexity, and splat chunks where applicable.
- [x] Duplicate paths, traversal, encrypted/unsupported entries, malformed
      lengths, digest mismatch, and trailing/ambiguous data behavior is defined.
- [x] Save contract states when old or new package is guaranteed readable and
      lists filesystem/platform assumptions (`fsync`, rename, directory sync).
- [x] Fault-injection plan covers write, flush, replace, and recovery points.
- [x] Large-file streaming/ZIP64 requirements are explicit.

## Closure record

Completed under PRE-MERGE WORK APPROVED after the outcome contract v4 worker
and reviewer countersignature. Main repository commit `1dc2d1d`
(`docs(issue-0023): define container security and recovery`) was integrated by
merge commit `9bb561f`. The review's residual non-blocking observations remain
owned by issue 0021. This file moved to `done/` and the index was updated in
this closure commit.
