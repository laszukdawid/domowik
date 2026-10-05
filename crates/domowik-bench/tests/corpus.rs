//! The corpus must be valid and reproducible, or every number measured from it
//! is measuring something else.

use domowik_bench::corpus;
use domowik_geometry::walls;

#[test]
fn every_corpus_document_validates() {
    for c in corpus::all().into_iter().chain(corpus::shapes()) {
        let errors = c.doc.validate();
        assert!(
            errors.is_empty(),
            "corpus {} is not a valid document: {:?}",
            c.name,
            errors
        );
    }
}

#[test]
fn generation_is_reproducible() {
    let a = corpus::all();
    let b = corpus::all();
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(x.name, y.name);
        assert_eq!(x.doc, y.doc, "corpus {} differs between runs", x.name);
    }
}

#[test]
fn identifiers_are_ordered_and_distinct() {
    let mut ids = corpus::IdGen::new(1);
    let mut previous = ids.mint();
    for _ in 0..1000 {
        let next = ids.mint();
        assert!(next > previous, "ids must ascend: {previous} then {next}");
        previous = next;
    }
    assert_eq!(previous.get_version_num(), 7, "shaped as a v7 uuid");
}

#[test]
fn the_corpus_covers_the_shapes_the_issue_names() {
    let sizes: Vec<(String, usize, usize, usize)> = corpus::all()
        .iter()
        .map(|c| {
            let s = walls::level_solids(&c.doc, c.primary_level());
            let (_, faces) = walls::plan(&s.walls, &s.joins);
            (
                c.name.to_string(),
                c.doc.walls.len(),
                c.doc.joins.len(),
                faces.len(),
            )
        })
        .collect();
    for (name, walls_n, joins_n, faces) in &sizes {
        assert!(*walls_n > 0, "{name} has no walls");
        assert!(*joins_n > 0, "{name} has no joins");
        assert!(*faces > 0, "{name} encloses no face");
    }
    // Reported so a change in the generator is visible in the diff rather
    // than only in a timing.
    println!("{sizes:#?}");
}
