//! Optional, presentation-only solid colours. No geometry or asset bytes change.
use crate::{AssetInstance, Opening, OpeningKind, Wall};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

/// Opaque sRGB, eight bits per channel. JSON is `#RRGGBB`; input accepts either
/// hex case, serialization is uppercase. Alpha remains the source material's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor(pub [u8; 3]);

impl RgbColor {
    pub fn parse(value: &str) -> Result<Self, String> {
        if value.len() != 7
            || !value.starts_with('#')
            || !value[1..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("colour must be #RRGGBB".into());
        }
        Ok(Self(std::array::from_fn(|i| {
            u8::from_str_radix(&value[1 + i * 2..3 + i * 2], 16).unwrap()
        })))
    }

    pub fn hex(self) -> String {
        let [r, g, b] = self.0;
        format!("#{r:02X}{g:02X}{b:02X}")
    }

    /// Display-space RGB in [0,1], matching the existing scene vertex colours.
    pub fn rgb(self) -> [f32; 3] {
        self.0.map(|v| f32::from(v) / 255.)
    }
}

impl Serialize for RgbColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.hex())
    }
}
impl<'de> Deserialize<'de> for RgbColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Material keys belong to one exact model digest: `gltf:N`, `obj:NAME`, or
/// `default`. Unknown keys are retained but never applied to another material.
pub type MaterialColors = BTreeMap<String, RgbColor>;

/// Reads the optional wall field, carried in the forward-compatible field map.
pub fn wall_color(wall: &Wall) -> Result<Option<RgbColor>, String> {
    wall.unknown
        .get("color")
        .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
        .transpose()
}

/// Leaf finish, independent of a door's operating mechanism. Glass uses ordinary
/// alpha blending, not refraction; frames and opening geometry are unchanged.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DoorStyle {
    #[default]
    Solid,
    Glass,
}

impl DoorStyle {
    /// Original leaf finish, before an optional color replacement (8-bit sRGB).
    pub fn color(self) -> RgbColor {
        match self {
            Self::Solid => RgbColor([158, 107, 64]),
            Self::Glass => RgbColor([128, 191, 230]),
        }
    }

    /// Linear coverage alpha. A ghosted level multiplies this value.
    pub fn opacity(self) -> f32 {
        match self {
            Self::Solid => 1.,
            Self::Glass => 0.25,
        }
    }
}

/// Optional leaf color on a door. Window metadata remains opaque and inert.
pub fn door_color(opening: &Opening) -> Result<Option<RgbColor>, String> {
    if opening.kind != OpeningKind::Door {
        return Ok(None);
    }
    opening
        .unknown
        .get("color")
        .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
        .transpose()
}

/// Effective door style; absence retains the original solid appearance.
pub fn door_style(opening: &Opening) -> Result<DoorStyle, String> {
    if opening.kind != OpeningKind::Door {
        return Ok(DoorStyle::Solid);
    }
    opening
        .unknown
        .get("doorStyle")
        .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
        .transpose()
        .map(Option::unwrap_or_default)
}

/// Plan/export leaf ink. `None` preserves the view's legacy solid-door ink;
/// technical glyphs stay opaque even when the physical leaf is glass.
pub fn door_plan_color(opening: &Opening) -> Result<Option<RgbColor>, String> {
    let style = door_style(opening)?;
    Ok(door_color(opening)?.or_else(|| (style == DoorStyle::Glass).then(|| style.color())))
}

/// Reads and bounds an instance palette. At most 256 nonempty material keys,
/// each at most 1024 UTF-8 bytes. Absent is the unmodified source appearance.
pub fn material_colors(asset: &AssetInstance) -> Result<MaterialColors, String> {
    let Some(value) = asset.unknown.get("materialColors") else {
        return Ok(BTreeMap::new());
    };
    let colors: MaterialColors =
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    validate_material_colors(&colors)?;
    Ok(colors)
}

pub fn validate_material_colors(colors: &MaterialColors) -> Result<(), String> {
    if colors.len() > 256 || colors.keys().any(|k| k.is_empty() || k.len() > 1024) {
        return Err(
            "materialColors requires at most 256 nonempty keys of at most 1024 bytes".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colors_are_bounded_and_canonical() {
        assert_eq!(RgbColor::parse("#ab12Cd").unwrap().hex(), "#AB12CD");
        for bad in ["red", "#FFF", "#12345678", "#GG0000", "#é0000", "1234567"] {
            assert!(RgbColor::parse(bad).is_err(), "{bad}");
        }
        assert!(
            validate_material_colors(&BTreeMap::from([("".into(), RgbColor([0; 3]))])).is_err()
        );
    }

    #[test]
    fn door_fields_are_validated_but_window_extensions_remain_opaque() {
        let mut doc = crate::Document::with_ground_floor();
        let wall = crate::Wall::new(
            doc.levels[0].id,
            crate::Point::new(0., 0.),
            crate::Point::new(4000., 0.),
            120.,
            2500.,
        );
        let mut door = Opening::door(wall.id, 0.5, 900., 2100., 120.);
        doc.walls.push(wall);
        assert_eq!(door_style(&door).unwrap(), DoorStyle::Solid);
        assert_eq!(door_plan_color(&door).unwrap(), None);
        door.unknown
            .insert("doorStyle".into(), serde_json::json!("glass"));
        assert_eq!(door_style(&door).unwrap().opacity(), 0.25);
        assert_eq!(
            door_plan_color(&door).unwrap(),
            Some(DoorStyle::Glass.color())
        );
        doc.openings.push(door);
        assert!(doc.validate().is_empty());
        for (field, value) in [
            ("color", serde_json::json!("#FFF")),
            ("color", serde_json::Value::Null),
            ("doorStyle", serde_json::json!("invisible")),
            ("doorStyle", serde_json::json!(0.5)),
        ] {
            let mut bad = doc.clone();
            bad.openings[0].unknown.insert(field.into(), value);
            assert!(bad
                .validate()
                .iter()
                .any(|e| matches!(e, crate::ValidationError::InvalidAppearance { .. })));
        }
        doc.openings[0].kind = OpeningKind::Window;
        doc.openings[0]
            .unknown
            .insert("doorStyle".into(), serde_json::json!({"future": true}));
        assert!(doc.validate().is_empty());
        assert_eq!(door_color(&doc.openings[0]).unwrap(), None);
    }
}
