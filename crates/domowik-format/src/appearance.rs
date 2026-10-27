//! Optional, presentation-only solid colours. No geometry or asset bytes change.
use crate::{AssetInstance, Wall};
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
}
