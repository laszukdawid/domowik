//! Non-destructive solid colours, through shared edit builders and normal saves.
use crate::{CliError, Plan, Result};
use domowik_format::{
    appearance::{self, RgbColor},
    AssetInstance, Document,
};
use serde_json::json;
use std::path::Path;

fn instance<'a>(doc: &'a Document, reference: &str) -> Result<&'a AssetInstance> {
    let matches: Vec<_> = doc
        .asset_instances
        .iter()
        .filter(|a| {
            a.id.to_string() == reference
                || crate::refs::short_id(a.id) == reference
                || a.display_name.as_deref() == Some(reference)
        })
        .collect();
    match matches.as_slice() {
        [a] => Ok(a),
        _ => Err(CliError::Usage(format!(
            "asset {reference:?} is missing or ambiguous; use its UUID"
        ))),
    }
}

/// JSON inventory of walls and placed assets, including stable material keys and
/// original colours when model bytes are available. Read-only; decode failures
/// are explicit per-asset diagnostics, not an empty material list.
pub fn list(path: &Path) -> Result<String> {
    let plan = Plan::open(path)?;
    let mut assets = vec![];
    for a in &plan.document().asset_instances {
        let materials = plan
            .assets
            .get(&a.digest)
            .ok_or_else(|| "model bytes unavailable".to_owned())
            .and_then(|bytes| domowik_models::appearance::materials(bytes));
        assets.push(json!({"id":a.id,"shortId":crate::refs::short_id(a.id),"name":a.display_name,"levelId":a.level_id,"materialColors":appearance::material_colors(a).map_err(CliError::Usage)?,"materials":materials.as_ref().ok(),"diagnostic":materials.as_ref().err()}));
    }
    Ok(serde_json::to_string_pretty(&json!({"walls":plan.document().walls.iter().map(|w| json!({"id":w.id,"shortId":crate::refs::short_id(w.id),"levelId":w.level_id,"color":appearance::wall_color(w).ok().flatten()})).collect::<Vec<_>>(),"assets":assets})).expect("finite colours"))
}

pub fn wall(path: &Path, reference: &str, color: Option<&str>) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let id = crate::refs::resolve_wall(plan.document(), reference, path)?.id;
    let color = color
        .map(RgbColor::parse)
        .transpose()
        .map_err(CliError::Usage)?;
    let ops =
        domowik_edit::appearance::walls(plan.document(), &[id], color).map_err(CliError::Usage)?;
    if ops.is_empty() {
        return Ok("Wall color is unchanged.".into());
    }
    plan.commit_and_save(ops)?;
    Ok(format!(
        "Updated wall #{} color.",
        crate::refs::short_id(id)
    ))
}

/// `KEY=#RRGGBB` entries patch the palette; `KEY=original` resets one material.
/// `reset` clears all overrides. Unknown new keys are refused before any save.
pub fn asset(path: &Path, reference: &str, entries: &[String], reset: bool) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let a = instance(plan.document(), reference)?;
    let mut colors = appearance::material_colors(a).map_err(CliError::Usage)?;
    if reset {
        colors.clear();
    }
    if !entries.is_empty() {
        let bytes = plan
            .assets
            .get(&a.digest)
            .ok_or_else(|| CliError::Failure("model bytes unavailable".into()))?;
        let materials = domowik_models::appearance::materials(bytes).map_err(CliError::Failure)?;
        for entry in entries {
            let (key, value) = entry.rsplit_once('=').ok_or_else(|| {
                CliError::Usage("use --material 'KEY=#RRGGBB' or 'KEY=original'".into())
            })?;
            if !materials.iter().any(|m| m.key == key) {
                return Err(CliError::Usage(format!(
                    "unknown material {key:?}; run color list"
                )));
            }
            if value == "original" {
                colors.remove(key);
            } else {
                colors.insert(
                    key.to_owned(),
                    RgbColor::parse(value).map_err(CliError::Usage)?,
                );
            }
        }
    }
    let id = a.id;
    let ops =
        domowik_edit::appearance::asset(plan.document(), id, &colors).map_err(CliError::Usage)?;
    if ops.is_empty() {
        return Ok("Model colors are unchanged.".into());
    }
    plan.commit_and_save(ops)?;
    Ok(format!(
        "Updated model #{} colors.",
        crate::refs::short_id(id)
    ))
}
