//! Operations, batches, and the positions that order them.
//!
//! Implements FORMAT-SPEC §4.1 (batches), §4.2 (operations) and the ordering
//! keys of §4.4 F3 and §4.6 LWW1.

use domowik_format::Entity;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

/// What an operation does to its target entity (FORMAT-SPEC §4.2 O2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OperationKind {
    /// Payload is the complete entity object per §2.3.
    CreateEntity {
        /// The entity as it should exist after this operation.
        entity: Entity,
    },
    /// Payload maps TOP-LEVEL entity field names to new values.
    ///
    /// Absent fields leave the entity unchanged; `null` clears an optional
    /// field to absent. O3: set- and array-valued fields are replaced
    /// WHOLESALE, never merged element-wise, so concurrency resolves at field
    /// granularity and never at element granularity.
    SetFields {
        /// Field name to new value, by serialized name.
        fields: Map<String, Value>,
    },
    /// Writes a tombstone (§4.5 I3). No payload.
    DeleteEntity,
}

/// One operation (FORMAT-SPEC §4.2 O1).
///
/// `operation_id` is an EXPLICIT field: deriving identifiers from serialized
/// bytes would violate §3.3 consequence 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub operation_id: Uuid,
    pub entity_id: Uuid,
    #[serde(flatten)]
    pub kind: OperationKind,
}

impl Operation {
    /// Creates an entity.
    pub fn create(entity: Entity) -> Self {
        Self {
            operation_id: Uuid::now_v7(),
            entity_id: entity.id(),
            kind: OperationKind::CreateEntity { entity },
        }
    }

    /// Sets top-level fields on an existing entity.
    pub fn set_fields(entity_id: Uuid, fields: Map<String, Value>) -> Self {
        Self {
            operation_id: Uuid::now_v7(),
            entity_id,
            kind: OperationKind::SetFields { fields },
        }
    }

    /// Deletes an entity, writing a tombstone.
    pub fn delete(entity_id: Uuid) -> Self {
        Self {
            operation_id: Uuid::now_v7(),
            entity_id,
            kind: OperationKind::DeleteEntity,
        }
    }

    /// True when this kind REQUIRES a pre-existing target (§4.5 I5).
    ///
    /// `createEntity` establishes existence at its own position and
    /// `deleteEntity` closes an interval; both define an interval rather than
    /// being classified by one. Only `setFields` needs its target to be live
    /// beforehand — but a delete against an absent or already-tombstoned
    /// target is stale on the same terms, which [`crate::replay`] handles
    /// separately because the reason differs.
    pub fn requires_live_target(&self) -> bool {
        matches!(self.kind, OperationKind::SetFields { .. })
    }
}

/// A batch's ordering position, FORMAT-SPEC §4.4 F3's `P`.
///
/// `P = (causalPastSize, counterBase, actorId)`, compared lexicographically
/// with `actorId` as an unsigned 128-bit integer (E5).
///
/// P is a SORT KEY, not a set of pairwise precedence constraints, and F3 says
/// why that distinction is the point: a rule of the form "causality, with
/// incomparable pairs ordered by ascending counter" states constraints that
/// can form a CYCLE and then define no order at all, whereas a sort by one
/// key with unique values cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    /// Number of operations in the batch's causal past (F1/F3).
    ///
    /// **`u128`, and §9.3 row L33 makes that mandatory rather than cautious.**
    /// The value is a sum of up to L18 frontier members, each an L13 signed-64-bit
    /// counter, so its maximum is 65,536 × (2⁶³ − 1) = 2⁷⁹ − 2¹⁶ — 79 bits. A
    /// conforming reader "represents and COMPARES every value in [0, 2⁷⁹ − 2¹⁶]
    /// exactly", and "wrapping and saturating arithmetic are FORBIDDEN", because
    /// either changes canonical order. No input is rejected for exceeding this:
    /// L18 and L13 already rejected anything that could, so the obligation is
    /// the reader's alone.
    pub causal_past_size: u128,
    /// The batch's first operation counter (B3).
    pub counter_base: u64,
    /// The producing actor, ordered as a u128 (E5).
    pub actor: u128,
}

/// A write position, FORMAT-SPEC §4.6 LWW1.
///
/// `P` refined by the operation's index within its batch. The leading three
/// components are exactly F3's `P`, so write positions never interleave two
/// batches' operations (B1), and the winner for a field is exactly the LAST
/// write to it in canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WritePosition {
    /// The containing batch's ordering position.
    pub batch: Position,
    /// Index within `operations` (B4).
    pub operation_index: usize,
}

/// A unit of atomic change (FORMAT-SPEC §4.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    pub batch_id: Uuid,
    pub actor_id: Uuid,
    /// The counter of the batch's FIRST operation (B3).
    pub counter_base: u64,
    /// actorId → highest counter of that actor observed (F1).
    ///
    /// A `BTreeMap` rather than a `HashMap`: the frontier is serialized and
    /// compared, and a stable key order keeps both deterministic.
    pub frontier: BTreeMap<Uuid, u64>,
    /// Application order within the batch is this array's order (B4).
    pub operations: Vec<Operation>,
    /// Unrecognised batch fields, carried verbatim (FORMAT-SPEC §8.4 U2).
    ///
    /// A batch is persisted as one `ops/` entry, so a field this version does
    /// not know is data a later version put there. Dropping it on write-back
    /// is the silent stripping §8.4 forbids; the operations inside already
    /// preserve theirs.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl Batch {
    /// Builds a batch. Framing is not validated here — that is
    /// [`crate::log::OperationLog::append`]'s job, because several of the
    /// checks are only answerable against the rest of the log.
    pub fn new(
        actor_id: Uuid,
        counter_base: u64,
        frontier: BTreeMap<Uuid, u64>,
        operations: Vec<Operation>,
    ) -> Self {
        Self {
            batch_id: Uuid::now_v7(),
            actor_id,
            counter_base,
            frontier,
            operations,
            unknown: Default::default(),
        }
    }

    /// The number of operations in this batch's causal past (F1/F3).
    ///
    /// Exactly the sum of the frontier's counters. This is exact BECAUSE B3
    /// makes each actor's ranges contiguous from 1, so `frontier[a]` is the
    /// count of that actor's observed operations rather than merely a
    /// watermark.
    pub fn causal_past_size(&self) -> u128 {
        // Widened BEFORE the addition, not after: summing in `u64` and casting
        // the result is exactly the forbidden accumulator, and it panics in
        // debug and wraps in release rather than failing visibly.
        self.frontier.values().map(|&c| u128::from(c)).sum()
    }

    /// This batch's ordering position (F3 `P`).
    pub fn position(&self) -> Position {
        Position {
            causal_past_size: self.causal_past_size(),
            counter_base: self.counter_base,
            actor: self.actor_id.as_u128(),
        }
    }

    /// The write position of the operation at `index` (LWW1).
    pub fn write_position(&self, index: usize) -> WritePosition {
        WritePosition {
            batch: self.position(),
            operation_index: index,
        }
    }

    /// The inclusive counter range this batch reserves (B3).
    ///
    /// `[counterBase, counterBase + len(operations) - 1]`. Callers must not
    /// use this on an empty batch, which is a framing reject.
    pub fn counter_range(&self) -> (u64, u64) {
        let last = self.counter_base + self.operations.len().saturating_sub(1) as u64;
        (self.counter_base, last)
    }

    /// The counter the frontier advances to after applying this batch (B5).
    pub fn range_end(&self) -> u64 {
        self.counter_range().1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::{Level, Point, Wall};

    fn level() -> Entity {
        Entity::Level(Level::new("Level 1"))
    }

    #[test]
    fn a_batch_reserves_a_contiguous_range_from_its_counter_base() {
        // B3: operation i carries counter counterBase + i.
        let b = Batch::new(
            Uuid::now_v7(),
            42,
            BTreeMap::new(),
            vec![
                Operation::create(level()),
                Operation::delete(Uuid::now_v7()),
                Operation::delete(Uuid::now_v7()),
            ],
        );
        assert_eq!(b.counter_range(), (42, 44));
        assert_eq!(b.range_end(), 44);
    }

    #[test]
    fn causal_past_size_is_the_sum_of_the_frontier_counters() {
        // FORMAT-SPEC §4.9's worked example: a frontier summing to 41 means
        // 41 operations precede the batch causally.
        let actor = Uuid::now_v7();
        let mut frontier = BTreeMap::new();
        frontier.insert(actor, 41);
        let b = Batch::new(
            Uuid::now_v7(),
            42,
            frontier,
            vec![Operation::delete(Uuid::now_v7())],
        );
        assert_eq!(b.causal_past_size(), 41);
        assert_eq!(b.position().causal_past_size, 41);
        assert_eq!(b.position().counter_base, 42);
    }

    #[test]
    fn causal_past_sums_that_cross_the_64_bit_boundaries_still_order_exactly() {
        // FORMAT-SPEC §9.3 row L33's OWN probe design, quoted: "frontiers VALID
        // under L13 and L18 whose sums cross the signed and unsigned 64-bit
        // boundaries — comparing sums of 2⁶³ − 1 against 2⁶³, and 2⁶⁴ − 1
        // against 2⁶⁴, each of which reverses under a 64-bit accumulator while
        // the exact order does not".
        //
        // Every counter below is a valid signed-64-bit value (L13) and every
        // frontier has three members, far under L18's 65,536 — so nothing here
        // is rejectable input. The obligation is the READER's arithmetic.
        let batch_with = |counters: [u64; 3]| {
            let mut frontier = BTreeMap::new();
            for (i, c) in counters.iter().enumerate() {
                frontier.insert(Uuid::from_u128(i as u128 + 1), *c);
            }
            Batch::new(
                Uuid::from_u128(1),
                1,
                frontier,
                vec![Operation::delete(Uuid::now_v7())],
            )
        };

        const I64_MAX: u64 = i64::MAX as u64; // 2^63 - 1, L13's ceiling.

        // Across the SIGNED boundary: 2^63 - 1 against 2^63.
        let below = batch_with([I64_MAX - 2, 1, 1]);
        let above = batch_with([I64_MAX - 2, 1, 2]);
        assert_eq!(below.causal_past_size(), (1u128 << 63) - 1);
        assert_eq!(above.causal_past_size(), 1u128 << 63);
        assert!(below.position() < above.position());

        // Across the UNSIGNED boundary: 2^64 - 1 against 2^64. This is the
        // pair a u64 accumulator reverses — the larger sum wraps to 0.
        let below = batch_with([I64_MAX, I64_MAX, 1]);
        let above = batch_with([I64_MAX, I64_MAX, 2]);
        assert_eq!(below.causal_past_size(), (1u128 << 64) - 1);
        assert_eq!(above.causal_past_size(), 1u128 << 64);
        assert!(
            below.position() < above.position(),
            "a 64-bit accumulator reverses this pair"
        );

        // And the row's stated maximum is representable and comparable:
        // 65,536 x (2^63 - 1) = 2^79 - 2^16.
        let mut widest = BTreeMap::new();
        for i in 0..65_536u128 {
            widest.insert(Uuid::from_u128(i + 1), I64_MAX);
        }
        let widest = Batch::new(
            Uuid::from_u128(1),
            1,
            widest,
            vec![Operation::delete(Uuid::now_v7())],
        );
        assert_eq!(widest.causal_past_size(), (1u128 << 79) - (1u128 << 16));
        assert!(above.position() < widest.position());
    }

    #[test]
    fn an_empty_frontier_means_nothing_observed() {
        let b = Batch::new(
            Uuid::now_v7(),
            1,
            BTreeMap::new(),
            vec![Operation::create(level())],
        );
        assert_eq!(b.causal_past_size(), 0);
    }

    #[test]
    fn positions_order_by_causal_past_before_counter_before_actor() {
        let low = Uuid::from_u128(1);
        let high = Uuid::from_u128(2);
        let p = |past, base, actor: Uuid| Position {
            causal_past_size: past,
            counter_base: base,
            actor: actor.as_u128(),
        };
        // Causality first: a bigger past wins even with a smaller counter.
        assert!(p(5, 1, high) > p(4, 99, low));
        // Then counter base.
        assert!(p(4, 2, low) > p(4, 1, high));
        // Then actor, as an unsigned 128-bit integer (E5).
        assert!(p(4, 1, high) > p(4, 1, low));
    }

    #[test]
    fn write_positions_never_interleave_two_batches() {
        // B1/LWW1: the leading three components are the batch's P, so every
        // operation of the earlier batch precedes every operation of the later.
        let a = Batch::new(
            Uuid::from_u128(1),
            1,
            BTreeMap::new(),
            vec![Operation::delete(Uuid::now_v7()); 3],
        );
        let mut frontier = BTreeMap::new();
        frontier.insert(Uuid::from_u128(1), 3);
        let b = Batch::new(
            Uuid::from_u128(1),
            4,
            frontier,
            vec![Operation::delete(Uuid::now_v7()); 3],
        );
        for i in 0..3 {
            for j in 0..3 {
                assert!(
                    a.write_position(i) < b.write_position(j),
                    "a[{i}] must precede b[{j}]"
                );
            }
        }
    }

    #[test]
    fn only_set_fields_requires_a_pre_existing_target() {
        // §4.5 I5: create ESTABLISHES existence and delete CLOSES an interval;
        // neither is classified by the interval test.
        assert!(!Operation::create(level()).requires_live_target());
        assert!(!Operation::delete(Uuid::now_v7()).requires_live_target());
        assert!(Operation::set_fields(Uuid::now_v7(), Map::new()).requires_live_target());
    }

    #[test]
    fn an_operation_round_trips_through_json_with_the_specified_shape() {
        let wall = Wall::new(
            Uuid::now_v7(),
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let id = wall.id;
        let op = Operation::create(Entity::Wall(wall));
        let json = serde_json::to_string(&op).expect("serializes");
        // §4.2 O1 names these fields; §2.3 names the entity type tag.
        assert!(json.contains(r#""operationId":"#), "got {json}");
        assert!(json.contains(r#""kind":"createEntity""#), "got {json}");
        assert!(json.contains(r#""type":"wall""#), "got {json}");
        let back: Operation = serde_json::from_str(&json).expect("round trips");
        assert_eq!(back, op);
        assert_eq!(back.entity_id, id);
    }

    #[test]
    fn a_set_fields_operation_serializes_its_fields_inline() {
        // The §4.9 example spells the payload as a sibling `fields` object.
        let mut fields = Map::new();
        fields.insert("endMm".to_owned(), serde_json::json!([4000.0, 2500.0]));
        let op = Operation::set_fields(Uuid::now_v7(), fields);
        let json = serde_json::to_string(&op).expect("serializes");
        assert!(json.contains(r#""kind":"setFields""#), "got {json}");
        assert!(
            json.contains(r#""fields":{"endMm":[4000.0,2500.0]}"#),
            "got {json}"
        );
    }
}
