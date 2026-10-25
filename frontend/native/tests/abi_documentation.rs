//! Every export is documented in both places that claim to describe the ABI.
//!
//! This exists because both claims had rotted. The README's "ABI summary"
//! omitted seven of the twenty-eight exports, four of them ordinary API the
//! editor round added (`domowik_plan_edit`, `_derived`, `_open`, `_save`) and
//! three locality probes. Nothing noticed, because nothing was looking: the
//! header and the README were maintained by hand and by memory.
//!
//! The check is textual on purpose. Reading the symbol table would need a
//! built artefact and a platform-specific tool, and would not answer the
//! question being asked, which is whether a person reading these documents
//! sees the real surface.

use std::collections::BTreeSet;

fn implementation_exports() -> BTreeSet<&'static str> {
    let mut names = exported_names(include_str!("../src/lib.rs"));
    names.extend(exported_names(include_str!("../src/asset_library.rs")));
    names.extend(exported_names(include_str!("../src/web.rs")));
    names
}

/// Names declared `#[no_mangle] pub extern "C" fn` in the implementation.
fn exported_names(source: &str) -> BTreeSet<&str> {
    let mut names = BTreeSet::new();
    let mut saw_no_mangle = false;
    for line in source.lines() {
        let line = line.trim();
        if line == "#[no_mangle]" {
            saw_no_mangle = true;
            continue;
        }
        if !saw_no_mangle {
            continue;
        }
        // Attributes and doc comments may sit between the two.
        if line.starts_with('#') || line.starts_with("///") || line.is_empty() {
            continue;
        }
        saw_no_mangle = false;
        // Both forms appear: the ones taking raw pointers are `unsafe`. An
        // earlier version of this matched only the safe form, found 13 of 28,
        // and would have let every `unsafe` export go undocumented. The count
        // assertion below is what caught it.
        let declaration = line
            .strip_prefix("pub unsafe extern \"C\" fn ")
            .or_else(|| line.strip_prefix("pub extern \"C\" fn "));
        if let Some(rest) = declaration {
            let name = rest
                .split(['(', '<', ' '])
                .next()
                .expect("a split always yields one element");
            names.insert(name);
        }
    }
    names
}

/// The fenced block under the README's "ABI summary" heading.
fn readme_abi_block(readme: &str) -> &str {
    let start = readme
        .find("## ABI summary")
        .expect("the README has an ABI summary heading");
    let rest = &readme[start..];
    let open = rest.find("```c").expect("the ABI summary opens a C block");
    let body = &rest[open + 4..];
    let close = body.find("```").expect("the ABI summary closes its block");
    &body[..close]
}

#[test]
fn every_export_is_declared_in_the_c_header() {
    let header = include_str!("../include/domowik_bridge.h");
    let missing: Vec<_> = implementation_exports()
        .into_iter()
        .filter(|name| !header.contains(&format!("{name}(")))
        .collect();
    assert!(
        missing.is_empty(),
        "exports absent from include/domowik_bridge.h: {missing:?}"
    );
}

#[test]
fn every_export_is_listed_in_the_readme_abi_summary() {
    let readme = include_str!("../README.md");
    let block = readme_abi_block(readme);
    let missing: Vec<_> = implementation_exports()
        .into_iter()
        .filter(|name| !block.contains(&format!("{name}(")))
        .collect();
    assert!(
        missing.is_empty(),
        "exports absent from the README ABI summary: {missing:?}"
    );
}

#[test]
fn the_readme_summary_invents_nothing() {
    // The other direction, and it is not symmetric noise: a documented export
    // that does not exist sends a caller looking up a symbol that will fail to
    // resolve at `dlopen` time, which is worse than an undocumented one.
    let readme = include_str!("../README.md");
    let exported = implementation_exports();
    let block = readme_abi_block(readme);

    let mut phantom = Vec::new();
    for (index, _) in block.match_indices("domowik_") {
        let tail = &block[index..];
        let end = tail
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(tail.len());
        let name = &tail[..end];
        if !exported.contains(name) && !phantom.contains(&name) {
            phantom.push(name);
        }
    }
    // `domowik_buf` is the struct the signatures take, not an export.
    phantom.retain(|name| *name != "domowik_buf");
    assert!(
        phantom.is_empty(),
        "the README ABI summary names symbols that are not exported: {phantom:?}"
    );
}

#[test]
fn the_extractor_finds_the_exports_it_is_supposed_to() {
    // A parser that silently matched nothing would make all three tests above
    // pass on an empty set, which is exactly the shape of check this spike has
    // twice had to delete. Pin the count and known names.
    let names = implementation_exports();
    assert_eq!(names.len(), 37, "found: {names:?}");
    assert!(names.contains("domowik_abi_version"));
    assert!(names.contains("domowik_plan_edit"));
    assert!(names.contains("domowik_plan_assets"));
    assert!(names.contains("domowik_plan_stair_preview"));
}
