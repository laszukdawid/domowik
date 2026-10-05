# 0073: Copy editor diagnostics and simplify numeric errors

- **Phase:** 2
- **Priority:** Medium
- **Status:** Done
- **Depends on:** 0024, 0048

## Problem

Editor status messages are truncated in the footer and only fully visible in a
tooltip, making errors awkward to share for diagnosis. An opening-fit error
also exposed binary64 calculation artifacts (many decimal places) and repeated
the `command rejected` prefix, obscuring the useful explanation.
## Scope

Make the complete footer status message directly copyable. Present opening-fit
measurements at human scale and report a rejection prefix only once. Warning
rows are already selectable and remain so.

## Acceptance criteria

- [x] A footer action copies the complete status message, including text
      beyond its visible truncation.
- [x] Opening-fit and host-height diagnostics use integer millimetre values
      and a rounded one-decimal position fraction where applicable, not
      binary64 tails.
- [x] A rejected gesture's `command rejected` prefix occurs no more than once.
- [x] Focused clipboard, formatting, and bridge tests cover these behaviors.

## Closure

- **Implementation/spec commits:** Pending local changes (no commit requested).
  Evidence: editor-shell clipboard test, format error-formatting test, and
  bridge rejection-prefix test. Full `./scripts/verify.sh` passes all stages
  except the expected `P13 clean tree` check.
- **Follow-up issues:** None.
