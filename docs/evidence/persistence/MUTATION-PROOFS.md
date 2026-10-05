# Mutation proofs — the §9.7 save state machine (issue 0029)

Nine mutations, nine caught. Two only after their tests were rewritten, and
one test had to be rewritten twice — the second time because an earlier edit
silently did nothing.

| # | Check | Mutation | Test |
|---:|---|---|---|
| 1 | S0 refuses a symlink target, and says WHY | disable the symlink guard | `saving_over_a_symlink_is_refused_rather_than_followed` |
| 2 | S0's temporary name is collision-safe | stop advancing the counter | `two_saves_from_one_process_use_different_temporary_names` |
| 3 | S2 validates the candidate with the full reader | skip validation | `an_existing_plan_survives_a_save_whose_candidate_would_be_unreadable` |
| 4 | S2 cleans up a failed candidate | leave it behind | `an_existing_plan_survives_a_save_whose_candidate_would_be_unreadable` |
| 5 | S2 validates against the READER's limits | validate against defaults | `a_save_that_exceeds_the_readers_limits_is_refused_before_replacing` |
| 6 | a malformed log never becomes a candidate | drop the pre-flight | `a_malformed_log_is_refused_before_a_candidate_is_even_created` |
| 7 | a bare filename resolves to an openable directory | accept the empty parent | `a_bare_filename_resolves_to_a_directory_that_can_be_opened` |

## The three that needed the tests changed first

**A test that asserts the variant does not test the guard (#1).** The symlink
test asserted only `matches!(err, UnsafeTarget { .. })`. Disabling the symlink
guard left it green, because the not-a-regular-file arm fires on a symlink too
— `symlink_metadata` reports `is_file() == false` for a link — with a message
that does not tell the user their path is a link. The test now asserts the
REASON.

**A durability step no save-and-read test can see (#7).** `Path::parent`
returns `Some("")` for a bare filename. Joining onto that still produces a
working relative path, so the writer works either way and every round-trip test
passes. What breaks is S5: `File::open("")` fails, the parent-directory sync
silently does nothing, and the durability step the platform profiles require
is skipped. Extracted as `parent_dir` with its own test, because the property
is about what can be OPENED, not about what a save produces.

**An edit that silently did nothing (#1 again).** The first attempt to
strengthen the symlink test was applied with a string replacement that had no
assertion, against text `cargo fmt` had since reformatted. It matched nothing,
changed nothing, and reported success. The mutation run caught it — the check
stayed NOT PROVEN — and the fix was to assert the pattern was found. A
replacement without a match assertion is the same failure mode as a check
without a mutation proof.

## What §7.6 makes unrepresentable, and what that changed

The malformed-log case was originally written as an S2 failure: two batches
reserving one counter range, expected to write and fail to read back. It does
not write at all. §7.6 names batch entries by `(counterBase, actorId)`, so two
batches reserving one range collide on their entry NAME, and the archive writer
reports "duplicate filename" for what is really a framing anomaly.

That is a better outcome than the one intended — the invalid state is
physically unrepresentable — but the ERROR was wrong, naming a ZIP-layer
symptom instead of the cause. A pre-flight `package.log()` now runs before S0
creates anything, so the caller learns what is wrong with their log.

Finding a genuine S2 case then took a different artifact: a future format
version writes perfectly and cannot be read back. Without S2 that save
succeeds and the NEXT open fails, after the old copy is already gone.
