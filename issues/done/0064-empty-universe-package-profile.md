# 0064: Decide the package profile of a plan with no assets

- **Phase:** 1
- **Priority:** Medium
- **Status:** Done
- **Depends on:** None

## Current disposition — 2026-10-05

Close: empty U(P) is portable, and both writer and CLI reporting implement
that decision.

Profile::derive in `crates/domowik-container/src/lib.rs` uses vacuous
completeness; §7.9 states the rule. Current container and CLI tests cover
portable/thin requests on empty plans plus a non-empty control. All passed
locally on 2026-10-05. The original unchecked list is historical and the later
completed acceptance list is authoritative.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Original problem

A package holding no assets always reads back as `thin`, whichever profile was
asked for. A user can run `pack --portable`, get a result that is correct in
every other respect, and find the manifest says `thin`.

FORMAT-SPEC §7.9 defines portable as a package that "embeds every blob of
`U(P)` at its §10.3-derived path and digest-verifies each". When `U(P)` is
empty that is vacuously true, so the text arguably makes an asset-free package
portable. The writer in `crates/domowik-container/src/lib.rs` derives the field
instead of taking it from the caller — §7.9 calls the profile "a fact about the
package, not a preference", which is right — but its rule requires a non-empty
table before it will say portable:

```rust
manifest.profile = if !manifest.asset_root.is_empty()
    && manifest
        .asset_root
        .iter()
        .all(|r| package.assets.contains_key(&r.digest))
{
    Profile::Portable
} else {
    Profile::Thin
};
```

So the empty case falls to `Thin` through the guard rather than through §7.9.

**Nothing is broken today and no data is at risk.** The package is internally
consistent, it round-trips, and `pack` reports the profile that was requested,
which is what probe P13b asserts. This is a question about what the format
means, surfacing as a manifest field that can disagree with the user's
intention.

## How it was found

Issue 0026 briefly changed `pack` to report the profile the written file
declares rather than the flag requested. Verification caught it: probe P13b
packs an asset-free plan with `--portable` and greps for `portable package`,
and the new message read

```
Wrote … as a thin package: 0 referenced asset(s), all embedded and verified.
```

— a sentence contradicting itself. Issue 0026 reverted its own change (commit
`45e31f4`) rather than weaken the probe or edit another issue's file, and
routed the question here.

## Scope

In scope: deciding what `profile` a package with an empty `U(P)` must declare,
and making the writer and FORMAT-SPEC §7.9 agree with each other.

Out of scope: the non-empty cases, which are settled and correct; `pack`'s
reported message, which follows whatever the answer turns out to be; and any
change to how the profile is derived when assets are present.

## What issue 0026 would do, and why it did not

**Drop the `!manifest.asset_root.is_empty()` guard**, so an empty `U(P)` is
portable by vacuous truth. Reasons:

- It follows the §7.9 text as written rather than adding a condition the
  specification does not state.
- It removes the case where a user's stated intention and the manifest
  disagree.
- Probe P13b already expects `portable package` for exactly this plan, so it
  stays green either way.

It was not done because the derivation is `domowik-container` code, which is
issue 0029's territory, and because the alternative is defensible: a package
with no assets is arguably neither profile, and defining it as `thin` by
convention is a legitimate answer — it just needs saying in §7.9 so nobody
re-litigates it.

Whoever picks this up should start from that reasoning rather than
rediscovering it. If the answer is "thin by convention", §7.9 wants a sentence
to that effect; if it is "portable by vacuous truth", the guard goes and the
container's own profile-derivation test moves with it.

## Acceptance criteria

- [x] FORMAT-SPEC §7.9 states what an empty `U(P)` implies for the `profile`
      field, rather than leaving it to a writer's guard.
- [x] The container's derivation matches that statement, with a test that
      fails if the empty case changes.
- [x] Probe P13b and the `pack` message agree with both.

## Decided 2026-09-12 — portable by vacuous truth

**An empty `U(P)` is PORTABLE.** FORMAT-SPEC §7.9 now says so, and the writer's
`!asset_root.is_empty()` guard is gone. This is the direction this file already
recommended; the reasoning was not rediscovered, it was checked and adopted.

Why portable rather than thin-by-convention. §7.9 defines portable as embedding
every blob of `U(P)` and digest-verifying each, which over an empty table holds
with nothing left to check — no blob can fail to embed and none can fail to
verify, so the package cannot break when moved, which is the whole content of
the word. "Thin by convention" is defensible but needs a rule the text does not
contain, and it would make one package's profile a matter of convention while
every other package's is derived. §7.9 calls the profile a fact about the
package, not a preference, so the derivation was the thing to fix.

**Three sites pinned the old behaviour, and all three moved deliberately.** A
container round-trip test asserting "a package with no assets is thin"; the
ED-0108 development-conversion test; and the CLI's `info --json` assertion.
None was wrong to exist — each pinned the writer faithfully — and each is now
updated with the §7.9 citation rather than deleted.

**The non-empty derivation is untouched**, which is this issue's stated scope
boundary: the `.all(|r| assets.contains_key(&r.digest))` test is the same
expression it was, with only the emptiness guard removed in front of it.

### The first fix was incomplete, and review caught it (R20)

The first attempt changed only the derivation and left `pack` reporting the
REQUESTED profile, with a comment claiming request and manifest now agreed in
every case. That claim was FALSE, and in the same class as the bug being fixed.
Removing the emptiness guard means `pack --thin` on an empty `U(P)` also emits
portable — correctly, since §7.9 makes the profile a fact rather than a
preference — while the message still printed `thin`. The contradiction did not
go away; it changed direction.

**The reporting is now truthful.** `pack` names the profile the file DECLARES,
derived through `Profile::derive`, the single shared rule the writer itself
uses — a second copy of the rule is how this defect arose in the first place.
When the emitted profile differs from the flag, the message says so rather than
silently substituting:

    --thin was requested; a plan with no assets is portable under
    FORMAT-SPEC §7.9, because there is no blob that could be left out.

A user who typed `--thin` is entitled to know the flag did not survive contact
with the format.

**Why BOTH empty-universe tests were required**, and this is the part worth
keeping: the `--portable` test PASSES against the defect, because the message
and the manifest happen to coincide there. Only the `--thin` test discriminates.
Mutation-proven — restoring "report the request" leaves `--portable` green and
turns `--thin` red.

### Acceptance

- [x] §7.9 states what an empty `U(P)` implies, rather than leaving it to a
      writer's guard.
- [x] The derivation matches, with a test that fails if the empty case changes:
      `an_asset_free_package_is_portable_not_thin`, mutation-proven by
      restoring the guard.
- [x] Probe P13b and the `pack` message agree with both. Three tests:
      `pack_portable_on_an_asset_free_plan_reports_what_the_manifest_says`,
      `pack_thin_on_an_asset_free_plan_emits_portable_and_says_so` (the one
      that discriminates), and `pack_thin_with_assets_still_emits_thin` as the
      non-empty control proving the untouched path is untouched. P13b operates
      on an asset-bearing plan, is unaffected, and `scripts/verify.sh` is not
      modified — no probe was weakened to fit.

Green on this branch: `cargo fmt --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo test --workspace`. No full `verify.sh`:
`main` is frozen pending the operator's check, and P13b runs inside it.

## Closure

- **Implementation/spec:** present in committed application baseline
  `a3cc8ee`; closure reconciled by issue 0094 on 2026-10-05.
- **Current verification:** CLI/container tests passed locally; all three
  empty-request/non-empty-control reporting tests and the container derivation
  test ran. Issue 0094 records final canonical verification.
- **Follow-up issues:** none
