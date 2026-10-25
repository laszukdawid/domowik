use domowik_format::*;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

fn stairs() -> Document {
    let mut d = Document::with_ground_floor();
    let low = d.levels[0].id;
    let mut upper = Level::new("Synthetic upper");
    upper.elevation_mm = 2600.0;
    let high = upper.id;
    d.levels.push(upper);
    let c = LevelConnection {
        id: Uuid::now_v7(),
        name: "Synthetic connection".into(),
        from: ConnectionEndpoint {
            level_id: low,
            position_mm: [0.0, 0.0],
            offset_mm: 0.0,
        },
        to: ConnectionEndpoint {
            level_id: high,
            position_mm: [3780.0, 0.0],
            offset_mm: 0.0,
        },
        unknown: Default::default(),
    };
    d.staircases.push(Staircase {
        id: Uuid::now_v7(),
        name: "Synthetic stair".into(),
        connection_id: c.id,
        width_mm: 924.0,
        risers: 15,
        guard_height_mm: 1000.0,
        opening_ids: vec![],
        unknown: Default::default(),
    });
    d.level_connections.push(c);
    d
}

#[test]
fn invalid_stairs_are_reported_without_hiding_valid_geometry() {
    let mut d = stairs();
    let valid = d.staircases[0].clone();
    let mut inverted = d.level_connections[0].clone();
    inverted.id = Uuid::now_v7();
    inverted.to.offset_mm = -3000.0;
    let mut bad = valid.clone();
    bad.id = Uuid::now_v7();
    bad.connection_id = inverted.id;
    let mut dangling = valid.clone();
    dangling.id = Uuid::now_v7();
    dangling.connection_id = Uuid::now_v7();
    d.level_connections.push(inverted);
    d.staircases.insert(0, bad.clone());
    d.staircases.push(dangling.clone());
    let low = d.levels[0].id;
    assert!(!d.validate_plan().is_empty());
    assert!(domowik_geometry::architecture::stair(&d, &bad).is_err());
    assert!(domowik_geometry::architecture::plan(&d, low).is_err());
    let (plan, diagnostics) = domowik_geometry::architecture::plan_reporting(&d, low);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].id, valid.id);
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics[0].contains(&bad.id.to_string()));
    assert!(diagnostics[1].contains(&dangling.id.to_string()));
    let scene = domowik_scene::build(&d, &[low], &BTreeSet::new(), &BTreeMap::new()).unwrap();
    assert_eq!(scene.diagnostics, diagnostics);
    assert!(!scene.nodes.is_empty());
    let prefix = format!("geometry:stair:{}:", valid.id);
    let stair_meshes: Vec<_> = scene
        .meshes
        .keys()
        .filter(|name| name.starts_with("geometry:stair:"))
        .collect();
    assert!(!stair_meshes.is_empty());
    assert!(stair_meshes.iter().all(|name| name.starts_with(&prefix)));
}
#[test]
fn staircase_renders_once_from_either_level_and_treads_reach_correct_elevations() {
    let d = stairs();
    let low = d.levels[0].id;
    let high = d.levels[1].id;
    let scene = |levels: &[Uuid]| {
        domowik_scene::build(&d, levels, &BTreeSet::new(), &BTreeMap::new()).unwrap()
    };
    let a = scene(&[low]);
    let b = scene(&[high]);
    let both = scene(&[low, high]);
    assert_eq!(a.nodes.len(), b.nodes.len());
    assert_eq!(a.nodes.len(), both.nodes.len());
    assert!(!a.nodes.is_empty());
    assert!(scene(&[]).nodes.is_empty());
    let prefix = format!("geometry:stair:{}:", d.staircases[0].id);
    let first = &a.meshes[&format!("{prefix}0:0")];
    let last = &a.meshes[&format!("{prefix}13:0")];
    let max_y = |vertices: &Vec<f32>| {
        vertices
            .as_chunks::<10>()
            .0
            .iter()
            .map(|v| v[1])
            .fold(f32::NEG_INFINITY, f32::max)
    };
    assert!((max_y(first) - 2.6 / 15.0).abs() < 1e-5);
    assert!((max_y(last) - 2.6 * 14.0 / 15.0).abs() < 1e-5);
    for mesh in a.meshes.values() {
        assert!(mesh.iter().all(|x| x.is_finite()));
    }
    let rail = &a.meshes[&format!("{prefix}14:0")];
    for v in rail.as_chunks::<10>().0 {
        let n = v[3].hypot(v[4]).hypot(v[5]);
        assert!((n - 1.0).abs() < 1e-5);
    }
}
#[test]
fn standalone_sloped_bodies_and_their_hosted_openings_render_without_stairs() {
    let mut d = Document::with_ground_floor();
    let low = d.levels[0].id;
    let body = ConstructionBody {
        id: Uuid::now_v7(),
        level_id: low,
        name: "Synthetic ramp body".into(),
        role: BodyRole::Platform,
        footprint: RectFootprint {
            start_mm: [0.0, 0.0],
            end_mm: [4000.0, 0.0],
            width_mm: 1000.0,
        },
        bottom_offsets_mm: [0.0, 1000.0],
        thickness_mm: 100.0,
        unknown: Default::default(),
    };
    let id = body.id;
    d.construction_bodies.push(body);
    d.hosted_openings.push(HostedOpening {
        id: Uuid::now_v7(),
        name: "Synthetic body cut".into(),
        host: OpeningHost::ConstructionBody(id),
        footprint: RectFootprint {
            start_mm: [1000.0, 0.0],
            end_mm: [2000.0, 0.0],
            width_mm: 2000.0,
        },
        unknown: Default::default(),
    });
    assert!(d.validate_plan().is_empty());
    let scene = domowik_scene::build(&d, &[low], &BTreeSet::new(), &BTreeMap::new()).unwrap();
    assert_eq!(scene.nodes.len(), 2);
    let max = scene
        .meshes
        .iter()
        .filter(|(name, _)| name.starts_with("geometry:body:"))
        .flat_map(|(_, v)| v.as_chunks::<10>().0.iter())
        .map(|v| v[1])
        .fold(0.0, f32::max);
    assert!((max - 1.1).abs() < 1e-5);
}
