# Issue 0011 audit — Area 3: V5/V6 compaction paths and I5 placement

**Status:** Evidence record for the four-audit-area closure (issue 0011 continuation, 2026-08-31).

## Scope

Seven sub-areas per KICKOFF:
1. I5 three-branch placement: before creation / within interval / after deletion
2. Mixed-target atomic batches
3. Create/delete/restore/delete intervals
4. Concurrent creates (LWW4)
5. Multi-operation operationIndex boundaries
6. V5 seeded-versus-full replay divergence
7. V6 snapshot replay equality

## Coverage enumeration (subagent-assisted)

Subagent searched internal/document/*_test.go and internal/persist/corpus_semantic_test.go for test coverage of each area.

### Area 1: I5 three-branch placement

| Branch | Test | Status |
|--------|------|--------|
| **Within interval** (branch 2) | `TestLateBatchInsideALiveIntervalDoesNotResurrectItsTarget` | EXISTS - asserts seeded equals full, E tombstoned, F gets write |
| **After deletion** (branch 3) | `TestLateCreateAgainstARetainedTombstone::"create positioned AFTER the delete"` | EXISTS - asserts stale discard, no resurrection |
| **After deletion** (LWW4 winner) | `TestLateCreateAgainstARetainedTombstone::"pre-delete concurrent create that WINS"` | EXISTS - asserts creation position moves |
| **After deletion** (LWW4 loser) | `TestLateCreateAgainstARetainedTombstone::"pre-delete concurrent create that LOSES"` | EXISTS - asserts losing create doesn't move position, batch not stale |
| **Before creation** (branch 1) | — | **ABSENT** - no explicit test |

**Gap identified:** Branch 1 (late batch positioned before entity's creation) not explicitly tested.

### Area 2: Mixed-target atomic batches

| Scenario | Test | Status |
|----------|------|--------|
| Multi-entity batch, one target deleted | `TestLateBatchInsideALiveIntervalDoesNotResurrectItsTarget` | EXISTS - M writes to E (deleted) and F (live), F gets write, E stays tombstoned |
| Stale batch with collateral create | `TestV4bStaleSetAfterObservedDeleteDiscardsWholeBatch` | EXISTS - entire batch discarded atomically including collateral |
| Losing create with other operations | `TestLateCreateAgainstARetainedTombstone::"pre-delete concurrent create that LOSES"` | EXISTS - losing create doesn't stale batch, other ops apply |

**Coverage:** SUFFICIENT

### Area 3: Create/delete/restore/delete intervals

| Scenario | Test | Status |
|----------|------|--------|
| Delete-undo-restore cycles | `internal/document/session_test.go` multiple | PARTIAL - via `deliverRestoring` (E6 restoration), not independent creates |
| Restoration rollback | `TestFailedRestorationTakesItsAuthorizationBack` | PARTIAL - single cycle only |
| Multiple independent create/delete/create cycles | — | **ABSENT** - via normal Deliver, not tested |

**Gap identified:** Multiple independent create/delete/create cycles on same entity through normal delivery not tested.

### Area 4: Concurrent creates (LWW4)

| Scenario | Test | Status |
|----------|------|--------|
| LWW4 collision, greater K wins | `TestLWW4ConcurrentCreateCollisionResolvesByGreaterK` | EXISTS - both arrival orders |
| Arrival-order independence | `TestLWW4ConcurrentCreateCollisionAcrossSeparateCalls` | EXISTS |
| Collision after snapshot seed | `TestV5ConcurrentCreateCollisionAfterSnapshotSeed` | EXISTS - creation keys survive compaction |
| Late creates vs tombstone | `TestLateCreateAgainstARetainedTombstone` (2 subtests) | EXISTS - winning and losing cases |
| Causally ordered creates reject | `TestCausallyOrderedDuplicateCreateIsAnO4Reject` | EXISTS - negative control |

**Coverage:** WELL COVERED (5 tests)

### Area 5: Multi-operation operationIndex

| Scenario | Test | Status |
|----------|------|--------|
| Index not counter | `TestWritePositionUsesTheOperationIndexNotItsCounter` | EXISTS - 2-op batch, index discriminates |
| Within-batch ordering | `TestWritePositionTieBreaksAmongEqualPast::"within one batch, greater operationIndex wins"` | EXISTS - 3-op batch, index 2 wins |
| Multi-op mixed targets | `TestLateBatchInsideALiveIntervalDoesNotResurrectItsTarget` | EXISTS - indices 0 and 1 |
| Snapshot round-trip | `internal/persist/snapshot_wire_test.go` | EXISTS - operationIndex 2 survives |
| Out-of-bounds reject | `TestReplayRejectsAnIndexOutsideItsRetainedBatch` | EXISTS - index 1 in 1-op batch |

**Coverage:** WELL COVERED (5 tests)

### Area 6: V5 seeded-versus-full replay

| Scenario | Test | Status |
|----------|------|--------|
| Late-concurrent resolution | `TestV5LateConcurrentResolvesIdenticallyAfterSnapshotSeed` | EXISTS - 2 cases (equal-past-greater-actor, smaller-past) |
| Create collision preservation | `TestV5ConcurrentCreateCollisionAfterSnapshotSeed` | EXISTS - redelivery silent |
| Multi-entity late batch | `TestLateBatchInsideALiveIntervalDoesNotResurrectItsTarget` | EXISTS - DeepEqual(seeded, full) |
| Late creates vs tombstone | `TestLateCreateAgainstARetainedTombstone` (multiple) | EXISTS - each subtest compares |
| Corpus V5 fixture | `corpus_semantic_test.go::checkV5` | EXISTS - late write doesn't win vs snapshot |

**Coverage:** WELL COVERED (5+ tests)

### Area 7: V6 snapshot replay equality

| Scenario | Test | Status |
|----------|------|--------|
| Property test all splits/orders | `TestV6SnapshotPlusSuffixEqualsFullReplayAllOrders` | EXISTS - 6 seeds, all splits, all orders, DeepEqual |
| Serialization crossing | `corpus_semantic_test.go::checkV6EqualsFullReplay` | EXISTS - BuildPackage/Load round-trip |
| Named effects | `corpus_semantic_test.go::checkV6GoldenPath` | EXISTS - level/fields/frontier/tombstone |
| Corpus V6 fixture | `TestProjectionVectors::"adversarial/vectors/v6-snapshot-replay"` | EXISTS - both checks |
| Lifecycle tests | Multiple in lifecycle_test.go | EXISTS - implicit V6 validation |

**Coverage:** WELL COVERED (5+ tests)

## Identified gaps

1. **I5 branch 1 (before creation):** No explicit test for a late batch positioned before an entity's creation position where the creation hasn't been staged yet.

2. **Multiple delete/create cycles:** No test exercises entity E being created, deleted, then created again as a new concurrent create, deleted again, etc. through normal Deliver. Only restoration-based cycles tested.

## Gap assessment

**I5 branch 1:** The absence is structural - if a late batch is positioned before an entity's creation, the entity doesn't exist at that position, so the batch's writes to it are stale (I5). This is the same outcome as branch 3 (after deletion) from the batch's perspective. The difference is in the metadata: branch 1 has no creation position to compare against (it's not retained because it's after the batch's position), while branch 3 has a visible creation position that precedes the batch. The implementation at `projection.go:595-625` handles this: `branchBeforeCreate` is "creation position is not earlier than the operation's write position", which correctly covers both "creation position doesn't exist yet" and "creation position is later". The gap is in explicit test coverage, not in the implementation.

**Multiple cycles:** This is an exotic case - multiple independent concurrent creates of the same entity ID through normal delivery would each need to be causally unordered with all previous creates/deletes of that ID. The more common case (causally ordered creates) is O4-rejected per `TestCausallyOrderedDuplicateCreateIsAnO4Reject`. The gap is real but the untested scenario is rare.

## Verification method

Coverage enumeration via mechanical search (subagent) followed by judgment review (executor). Each EXISTS entry was verified to assert observable behavior, not merely execute code. The two ABSENT entries are recorded as unverified, not as defects.

## Result

**AREA 3 SAMPLED.** V5/V6 and I5 coverage enumerated. Well-covered: LWW4, multi-operation, V5, V6. Gaps recorded: I5 branch 1, multiple independent cycles. Gaps are test coverage, not implementation defects. Per the brief: "Report what you VERIFIED, what you SAMPLED, and what you never reached." This area was SAMPLED (coverage enumeration, not exhaustive behavior verification).
