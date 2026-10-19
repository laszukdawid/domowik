//! Wall-axis admission diagnostics shared by raw commands and gesture builders.
//!
//! Stored history is not reinterpreted. A new transition checks added or
//! geometrically changed axes against final same-level walls. This also lets a
//! legacy document's unrelated fields be edited without accepting new overlap.
//!
//! Two kinds of block, in R3.1.2 precedence order. The REFUSING axis classes —
//! coincidence and collinear overlap — can never be resolved. The UNRESOLVED
//! classes — a crossing or an endpoint-on-interior contact left unsplit — are
//! what the shared builder resolves by splitting, so a gesture never produces
//! them; a raw, external or history batch that does is refused instead
//! (BEHAVIOR-SPEC §3.1 "Wall-contact admission", ED-0119). v0.1 has no
//! unconnected overpasses.

use domowik_format::{topology_cell, Document, Point, Wall};
use domowik_geometry::wall_contact::{axis_contact, crossing_point, AxisContact, AxisRole};
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
    /// Two axes cross at interior points of both and neither is split there.
    /// A crossing in a topology cell holding an endpoint of EACH wall is a
    /// corner, not this.
    #[serde(rename = "unresolved-wall-crossing")]
    #[error(
        "unresolved-wall-crossing: walls {wall} and {other} cross without being split \
         at the crossing; same-level crossing walls must meet at a join"
    )]
    UnresolvedCrossing { wall: Uuid, other: Uuid },
    /// An endpoint of `other` lies on the interior of `wall`, which is not
    /// split there. Not this when the point shares a topology cell with one of
    /// `wall`'s endpoints: that is an endpoint-endpoint contact.
    #[serde(rename = "unsplit-wall-contact")]
    #[error(
        "unsplit-wall-contact: an end of wall {other} lies on the interior of wall \
         {wall}, which is not split there"
    )]
    UnsplitContact { wall: Uuid, other: Uuid },
}

impl WallContactBlock {
    /// True for the two classes R3.1.2 refuses outright; false for the
    /// unresolved classes, which a builder resolves by splitting.
    pub fn is_axis_refusal(&self) -> bool {
        matches!(
            self,
            Self::WallCoincidence { .. } | Self::CollinearOverlap { .. }
        )
    }
}

/// All refusing and unresolved contacts involving a new or changed final
/// wall axis: the admission guard for every newly effective batch.
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
/// A crossing or an endpoint-on-interior contact is unresolved unless the
/// contact point shares a topology cell with an endpoint of each wall it would
/// otherwise split (ED-0103 N-2 identity, ED-0119): the shared builder treats
/// that point as the endpoint and splits nothing, and this must agree. A
/// crossing point is constructed only for pairs classified as crossing.
///
/// Results sort by class (coincidence, overlap, crossing, unsplit contact),
/// then the UUID pair.
/// Work is O(n + k*n + r*log(r)), auxiliary storage O(n + r), for n final
/// walls, k affected axes and r refusals. A one-wall edit never performs a
/// document-wide all-pairs classification. There is no mutation or cache.
pub fn wall_contact_refusals(before: &Document, after: &Document) -> Vec<WallContactBlock> {
    classify(after, &changed_walls(before, after), true)
}

/// Only the refusing axis classes of [`wall_contact_refusals`]: coincidence
/// and collinear overlap, which no split can resolve.
///
/// What a builder checks BEFORE resolving contacts (R3.1.2 precedence puts
/// these first). Its final candidate is then judged by the full guard.
pub fn axis_contact_refusals(before: &Document, after: &Document) -> Vec<WallContactBlock> {
    classify(after, &changed_walls(before, after), false)
}

fn changed_walls(before: &Document, after: &Document) -> HashSet<Uuid> {
    let previous: HashMap<_, _> = before.walls.iter().map(|w| (w.id, w)).collect();
    after
        .walls
        .iter()
        .filter(|wall| {
            previous
                .get(&wall.id)
                .is_none_or(|old| !same_axis(old, wall))
        })
        .map(|w| w.id)
        .collect()
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
    classify(after, changed, true)
}

/// Whether an endpoint of `w` shares `p`'s topology cell.
fn ends_in_cell(w: &Wall, p: Point) -> bool {
    let cell = topology_cell(p);
    topology_cell(w.start()) == cell || topology_cell(w.end()) == cell
}

fn classify(after: &Document, changed: &HashSet<Uuid>, unresolved: bool) -> Vec<WallContactBlock> {
    let mut blocks = Vec::new();
    for wall in after.walls.iter().filter(|w| changed.contains(&w.id)) {
        for other in &after.walls {
            if other.id == wall.id
                || other.level_id != wall.level_id
                || (changed.contains(&other.id) && other.id < wall.id)
            {
                continue;
            }
            blocks.extend(pair_block(wall, other, unresolved));
        }
    }
    blocks.sort_unstable();
    blocks
}

/// The block one same-level pair raises, if any. `unresolved` adds the two
/// classes after the refusing ones.
fn pair_block(wall: &Wall, other: &Wall, unresolved: bool) -> Option<WallContactBlock> {
    let a = wall.id.min(other.id);
    let b = wall.id.max(other.id);
    match axis_contact(wall.start(), wall.end(), other.start(), other.end())? {
        AxisContact::Coincident => Some(WallContactBlock::WallCoincidence { wall: a, other: b }),
        AxisContact::CollinearOverlap => {
            Some(WallContactBlock::CollinearOverlap { wall: a, other: b })
        }
        AxisContact::ProperCrossing if unresolved => {
            let corner = crossing_point(wall.start(), wall.end(), other.start(), other.end())
                .is_some_and(|p| ends_in_cell(wall, p) && ends_in_cell(other, p));
            (!corner).then_some(WallContactBlock::UnresolvedCrossing { wall: a, other: b })
        }
        AxisContact::EndpointOnInterior { split, at } if unresolved => {
            let (host, end) = match split {
                AxisRole::First => (wall, other),
                AxisRole::Second => (other, wall),
            };
            (!ends_in_cell(host, at)).then_some(WallContactBlock::UnsplitContact {
                wall: host.id,
                other: end.id,
            })
        }
        _ => None,
    }
}

/// Every refusing or unresolved contact in the whole document, between any
/// two same-level walls — a diagnostic of the STORED state, not an admission
/// rule. A conforming document has none; a legacy one may, since history
/// replays unchanged. `domowik check` reports these so an agent can repair
/// them.
///
/// A spatial index over each level's exact axis bounds is the broad phase:
/// any axis contact implies boundary-inclusive overlap of those bounds, and
/// bounds are taken as stored, never rounded, so no contact is missed. Cost
/// is O(n log n) plus the candidate pairs, not n². Invalid axes are skipped,
/// as [`axis_contact`] would refuse them. Sorted as [`wall_contact_refusals`].
pub fn document_contact_blocks(doc: &Document) -> Vec<WallContactBlock> {
    use domowik_geometry::spatial::{Aabb, QueryScratch, SpatialIndex};
    let mut by_level: HashMap<Uuid, Vec<&Wall>> = HashMap::new();
    for wall in &doc.walls {
        let (s, e) = (wall.start(), wall.end());
        if [s, e].iter().all(|p| p.x.is_finite() && p.y.is_finite()) && s != e {
            by_level.entry(wall.level_id).or_default().push(wall);
        }
    }
    let mut blocks = Vec::new();
    for walls in by_level.values() {
        let index = SpatialIndex::build(
            walls
                .iter()
                .enumerate()
                .map(|(i, w)| (Aabb::from_corners(w.start(), w.end()), i)),
        );
        let mut scratch = QueryScratch::default();
        for (i, wall) in walls.iter().enumerate() {
            let region = Aabb::from_corners(wall.start(), wall.end());
            for j in index.query_with_scratch(&region, &mut scratch) {
                if j > i {
                    blocks.extend(pair_block(wall, walls[j], true));
                }
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

    fn axis(level: Uuid, id: u128, a: (f64, f64), b: (f64, f64)) -> Wall {
        let mut wall = Wall::new(
            level,
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
            200.0,
            2500.0,
        );
        wall.id = Uuid::from_u128(id);
        wall
    }

    #[test]
    fn unresolved_classes_follow_the_refusing_ones_and_respect_point_identity() {
        let mut after = Document::with_ground_floor();
        let level = after.levels[0].id;
        let before = after.clone();
        after.walls = vec![
            axis(level, 1, (0.0, 0.0), (4000.0, 0.0)),
            // Crosses 1 mid-span: unresolved.
            axis(level, 2, (2000.0, -1000.0), (2000.0, 1000.0)),
            // Ends on 1's interior: unsplit.
            axis(level, 3, (1000.0, 0.0), (1000.0, 1000.0)),
            // Overlaps 1 along its axis: refused outright, sorted first.
            axis(level, 4, (2500.0, 0.0), (3500.0, 0.0)),
            // Ends a hair inside 1's start, in its cell: a corner.
            axis(level, 5, (0.0001, 0.0), (0.0001, -900.0)),
            // Crosses 1 in the cell of its own start AND of 1's end: a corner.
            axis(level, 6, (3999.9998, 0.0001), (4000.0001, -1000.0)),
        ];
        let id = Uuid::from_u128;
        let all = wall_contact_refusals(&before, &after);
        assert_eq!(
            all,
            vec![
                WallContactBlock::CollinearOverlap {
                    wall: id(1),
                    other: id(4)
                },
                WallContactBlock::UnresolvedCrossing {
                    wall: id(1),
                    other: id(2)
                },
                WallContactBlock::UnsplitContact {
                    wall: id(1),
                    other: id(3)
                },
            ]
        );
        assert_eq!(axis_contact_refusals(&before, &after), all[..1].to_vec());
        assert!(all[0].is_axis_refusal() && !all[1].is_axis_refusal());
        // Unchanged legacy axes are not admission-judged at all; the
        // whole-document diagnostic still finds every one of these pairs.
        assert!(wall_contact_refusals(&after, &after).is_empty());
        assert_eq!(document_contact_blocks(&after), all);
    }
}
