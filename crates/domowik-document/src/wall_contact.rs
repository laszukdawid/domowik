//! Wall-axis admission diagnostics shared by raw commands and gesture builders.
//!
//! Stored history is not reinterpreted. A new transition checks added or
//! geometrically changed axes against final same-level walls. This also lets a
//! legacy document's unrelated fields be edited without accepting new overlap.

use domowik_format::{Document, Wall};
use domowik_geometry::wall_contact::{axis_contact, AxisContact};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// A refusing R3.1.2 axis contact. Pair identities are ascending UUIDs, not
/// dependent on document order or which wall the command names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, thiserror::Error)]
#[serde(tag = "code", rename_all = "kebab-case")]
pub enum WallContactBlock {
    /// Full coincidence takes precedence over every partial overlap.
    #[error("wall-coincidence: walls {wall} and {other} have the same axis")]
    WallCoincidence { wall: Uuid, other: Uuid },
    /// Collinear axes share a strictly positive-length interval.
    #[error("collinear-overlap: walls {wall} and {other} overlap along their axes")]
    CollinearOverlap { wall: Uuid, other: Uuid },
}

/// All refusing contacts involving a new or changed final wall axis.
///
/// Coordinates are stored binary64 millimetres in the format's domain. The
/// classifier uses exact orientation signs; no tolerance or minimum length
/// is introduced. Invalid axes are left to entity validation, never treated
/// as evidence of a valid disjoint pair. Different levels do not interact.
///
/// Affected means added, moved to another level, or changed as an UNDIRECTED
/// segment. Thickness, height, metadata and reversal do not alter axis
/// contact. An unchanged legacy conflict is therefore tolerated, but moving
/// either participant must remove its refusing contacts in the final state.
/// Deletion can repair a conflict; transient intermediate overlap is ignored.
///
/// Results sort by class (coincidence before overlap), then the UUID pair.
/// Work is O(n + k*n + r*log(r)), auxiliary storage O(n + r), for n final
/// walls, k affected axes and r refusals. A one-wall edit never performs a
/// document-wide all-pairs classification. There is no mutation or cache.
pub fn wall_contact_refusals(before: &Document, after: &Document) -> Vec<WallContactBlock> {
    let previous: HashMap<_, _> = before.walls.iter().map(|w| (w.id, w)).collect();
    let changed: HashSet<_> = after
        .walls
        .iter()
        .filter(|wall| {
            previous
                .get(&wall.id)
                .is_none_or(|old| !same_axis(old, wall))
        })
        .map(|w| w.id)
        .collect();
    wall_contact_refusals_among(after, &changed)
}

/// [`wall_contact_refusals`], with the affected ids supplied rather than
/// discovered by diffing two documents.
///
/// Same rule, same results, same ordering: `changed` must be exactly the set
/// the two-document form computes — final walls that are new, moved to another
/// level, or changed as an undirected segment, judged by [`same_axis`].
///
/// It exists because a caller may already KNOW what it touched. Replay holds a
/// rollback journal naming every entity one batch changed, and deriving the set
/// from that costs the batch's size; discovering it by comparison costs a
/// retained copy of the previous document, which is the per-batch copy issue
/// 0038 removes.
pub fn wall_contact_refusals_among(
    after: &Document,
    changed: &HashSet<Uuid>,
) -> Vec<WallContactBlock> {
    let mut blocks = Vec::new();
    for wall in after.walls.iter().filter(|w| changed.contains(&w.id)) {
        for other in &after.walls {
            if other.id == wall.id
                || other.level_id != wall.level_id
                || (changed.contains(&other.id) && other.id < wall.id)
            {
                continue;
            }
            let a = wall.id.min(other.id);
            let b = wall.id.max(other.id);
            match axis_contact(wall.start(), wall.end(), other.start(), other.end()) {
                Some(AxisContact::Coincident) => {
                    blocks.push(WallContactBlock::WallCoincidence { wall: a, other: b })
                }
                Some(AxisContact::CollinearOverlap) => {
                    blocks.push(WallContactBlock::CollinearOverlap { wall: a, other: b });
                }
                _ => {}
            }
        }
    }
    blocks.sort_unstable();
    blocks
}

/// True when two versions of a wall occupy the same undirected axis on the
/// same level.
///
/// Public so a caller building its own `changed` set for
/// [`wall_contact_refusals_among`] uses this module's definition of "affected"
/// rather than a second, drifting copy of it. Thickness, height, metadata and
/// endpoint reversal do not alter the axis.
pub fn same_axis(a: &Wall, b: &Wall) -> bool {
    a.level_id == b.level_id
        && ((a.start_mm == b.start_mm && a.end_mm == b.end_mm)
            || (a.start_mm == b.end_mm && a.end_mm == b.start_mm))
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::Point;

    fn wall(level: Uuid, id: u128, start: f64, end: f64) -> Wall {
        let mut wall = Wall::new(
            level,
            Point::new(start, 0.0),
            Point::new(end, 0.0),
            200.0,
            2500.0,
        );
        wall.id = Uuid::from_u128(id);
        wall
    }

    #[test]
    fn precedence_and_pairs_do_not_follow_document_order() {
        let mut after = Document::with_ground_floor();
        let level = after.levels[0].id;
        let before = after.clone();
        after.walls = vec![
            wall(level, 3, 2.0, 6.0),
            wall(level, 2, 4.0, 0.0),
            wall(level, 1, 0.0, 4.0),
        ];
        let expected = vec![
            WallContactBlock::WallCoincidence {
                wall: Uuid::from_u128(1),
                other: Uuid::from_u128(2),
            },
            WallContactBlock::CollinearOverlap {
                wall: Uuid::from_u128(1),
                other: Uuid::from_u128(3),
            },
            WallContactBlock::CollinearOverlap {
                wall: Uuid::from_u128(2),
                other: Uuid::from_u128(3),
            },
        ];
        assert_eq!(wall_contact_refusals(&before, &after), expected);
        after.walls.reverse();
        assert_eq!(wall_contact_refusals(&before, &after), expected);
    }

    #[test]
    fn legacy_conflicts_allow_neutral_edits_but_not_changed_overlapping_axes() {
        let mut before = Document::with_ground_floor();
        let level = before.levels[0].id;
        before.walls = vec![wall(level, 1, 0.0, 4.0), wall(level, 2, 2.0, 6.0)];
        let mut after = before.clone();
        after.walls[0].height_mm = 3000.0;
        after.walls[0].thickness_mm = 500.0;
        // Reversal changes stored roles, not the occupied axis.
        let w = &mut after.walls[0];
        std::mem::swap(&mut w.start_mm, &mut w.end_mm);
        assert!(wall_contact_refusals(&before, &after).is_empty());
        after.walls[0].end_mm[0] = 1.0;
        assert_eq!(wall_contact_refusals(&before, &after).len(), 1);
        after.walls.remove(0);
        assert!(wall_contact_refusals(&before, &after).is_empty());
    }

    #[test]
    fn level_change_is_affected_and_endpoint_touch_is_allowed() {
        let mut before = Document::with_ground_floor();
        let level = before.levels[0].id;
        before.walls = vec![
            wall(level, 1, 0.0, 4.0),
            wall(Uuid::from_u128(99), 2, 4.0, 0.0),
        ];
        assert!(wall_contact_refusals(&Document::new(), &before).is_empty());
        let mut after = before.clone();
        after.walls[1].level_id = level;
        assert!(matches!(
            wall_contact_refusals(&before, &after).as_slice(),
            [WallContactBlock::WallCoincidence { .. }]
        ));
        after.walls[1].end_mm[0] = 8.0;
        assert!(wall_contact_refusals(&before, &after).is_empty());
    }
}
