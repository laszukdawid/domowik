//! Stable identities for the rings the arrangement is built from.
//!
//! The full builder takes positional slices; an incremental one cannot,
//! because deleting a wall renumbers every wall after it (`walls.rs`
//! `LevelSolids` is rebuilt in document order). Every input ring is
//! therefore keyed by the document identity it comes from.

use crate::walls::{Endpoint, JoinSpec, LevelSolids};
use domowik_format::Point;
#[cfg(test)]
use std::collections::HashMap;
use uuid::Uuid;

/// The document identity of one input ring.
///
/// A wall contributes its butt-capped rectangle (R3.2.2). A join has no id
/// of its own in the plan solids, so it is keyed by its member set — sorted
/// `(wall id, is_end)` pairs, which is what FORMAT-SPEC §2.3 makes a join
/// (an unordered set, C3) — plus an ordinal that keeps two joins with an
/// identical member set distinct in a malformed document.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceKey {
    /// A wall's rectangle, or its first piece after opening cuts.
    Wall(Uuid),
    /// A wall's further pieces after opening cuts (`piece ≥ 1`, in the
    /// deterministic order `cuts::wall_pieces` returns them).
    WallPiece { wall: Uuid, piece: u32 },
    Join {
        members: Vec<(Uuid, bool)>,
        ordinal: u32,
    },
}

/// One change to the engine's inputs.
#[derive(Debug, Clone)]
pub enum SourceChange {
    /// Insert or replace the ring of `key`. Any winding; an empty or
    /// zero-area ring contributes nothing, exactly as in the full builder.
    Upsert(SourceKey, Vec<Point>),
    /// Remove `key` if present.
    Remove(SourceKey),
}

/// The key of `join` with ordinal 0: its sorted `(wall id, is_end)` members.
pub(super) fn join_key(solids: &LevelSolids, join: &JoinSpec) -> SourceKey {
    let mut members: Vec<(Uuid, bool)> = join
        .members
        .iter()
        .filter_map(|m| {
            let id = *solids.ids.get(m.wall)?;
            Some((id, m.endpoint == Endpoint::End))
        })
        .collect();
    members.sort_unstable();
    SourceKey::Join {
        members,
        ordinal: 0,
    }
}

/// The key of every join of `solids`, in order, repeated member sets
/// numbered by occurrence. The reference for `LevelGeometry::sync`'s
/// incremental numbering (tests).
#[cfg(test)]
pub(super) fn join_keys(solids: &LevelSolids) -> Vec<(SourceKey, &JoinSpec)> {
    let mut seen: HashMap<SourceKey, u32> = HashMap::new();
    let mut out = Vec::with_capacity(solids.joins.len());
    for join in &solids.joins {
        let base = join_key(solids, join);
        let ordinal = seen.entry(base.clone()).or_insert(0);
        let key = match base {
            SourceKey::Join { members, .. } => SourceKey::Join {
                members,
                ordinal: *ordinal,
            },
            wall => wall,
        };
        *ordinal += 1;
        out.push((key, join));
    }
    out
}
