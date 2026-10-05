//! Asset descriptors — ASSET-STRATEGY §10.2.
//!
//! A descriptor is PACKAGE METADATA, not a plan entity. FORMAT-SPEC §2.3
//! fixes v0.1 at eight entity kinds; a descriptor has no `levelId`, is never
//! addressed by `createEntity`/`setFields`/`deleteEntity`, and takes no part
//! in replay or projection. It sits beside the asset-root table and describes
//! a blob, whoever references it.
//!
//! The descriptor is what BEHAVIOR-SPEC §9.6 reads. Its condition matrix keys
//! on `requirement` — required versus optional decides whether a missing blob
//! blocks an editable open — and its placeholder rule reads
//! `placeholderDimensions`. Without a descriptor there is no honest answer to
//! either question, which is why "treat everything as degraded" is not an
//! acceptable stand-in.

use serde::{Deserialize, Serialize};

/// Whether the referencing document can be edited while this asset is
/// unresolved (ASSET-STRATEGY §10.2; behaviour in BEHAVIOR-SPEC §9.6).
///
/// A value that is neither `required` nor `optional` is kept VERBATIM as
/// [`Requirement::Undeclared`] rather than rejected. Two reasons, and the
/// second is the one that bites:
///
/// - §8 U2 and row L32 forbid silent stripping on the write-back path. A
///   later version may define a third level, and rejecting the whole
///   descriptor would stop an older build reading a newer file at all, while
///   silently rewriting the value would lose it.
/// - The container reads exactly this field to decide the §9.6 matrix and
///   treats anything unrecognised as undeclared. If this type rejected those
///   values instead, the two would disagree on precisely the malformed inputs
///   where agreement matters most.
///
/// `Undeclared` is treated as `Required` wherever the matrix asks — fail
/// closed — so an unknown level can never be quietly more permissive than a
/// known one.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Requirement {
    /// Unresolved ⇒ editable open blocked, read-only degraded inspection.
    Required,
    /// Unresolved ⇒ document opens editable with a non-blocking diagnostic.
    Optional,
    /// The field was not present at all.
    ///
    /// Distinct from an unrecognised VALUE so that absence round-trips as
    /// absence: writing `"requirement": ""` back for a descriptor that never
    /// carried the field would invent data, which §8 U2 forbids just as
    /// firmly as dropping it.
    ///
    /// The DEFAULT, which is what `#[serde(default)]` on the field reaches for
    /// when the key is missing.
    #[default]
    Absent,
    /// Any other JSON value, kept exactly as written — including `null` and
    /// non-strings, so a round trip is byte-faithful rather than coerced.
    Undeclared(serde_json::Value),
}

impl Requirement {
    /// The token as written, when it was written as a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Required => Some("required"),
            Self::Optional => Some("optional"),
            Self::Absent => None,
            Self::Undeclared(v) => v.as_str(),
        }
    }

    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    /// Whether an unresolved asset at this level blocks an editable open.
    ///
    /// Everything that is not exactly `optional` blocks. §9.6 has no row for a
    /// requirement level it does not define, and the safe reading of a missing
    /// row is the one that refuses to edit rather than the one that permits
    /// it. Absent, `null`, a number, a misspelling and a future third level
    /// all land here together, which is deliberate: the container's open-mode
    /// gate makes exactly the same call on exactly the same field, and the two
    /// must not disagree on malformed input.
    pub fn blocks_editing(&self) -> bool {
        !matches!(self, Self::Optional)
    }
}

impl Serialize for Requirement {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Required => s.serialize_str("required"),
            Self::Optional => s.serialize_str("optional"),
            // Never reached: the field carries `skip_serializing_if` for this
            // case, so an absent requirement is omitted rather than written.
            Self::Absent => s.serialize_none(),
            Self::Undeclared(v) => v.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for Requirement {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // ANY JSON value is accepted. Rejecting `null`, a number or a
        // misspelling would make the whole descriptor unparsable, and then the
        // one field the §9.6 gate needs would be invisible precisely when the
        // descriptor is malformed — the case where failing closed matters
        // most. Tolerant parsing, strict validation, reported separately.
        let raw = serde_json::Value::deserialize(d)?;
        Ok(match raw.as_str() {
            Some("required") => Self::Required,
            Some("optional") => Self::Optional,
            _ => Self::Undeclared(raw),
        })
    }
}

/// Declared placeholder dimensions in millimetres (§10.2
/// `placeholderDimensions`), consumed by the BEHAVIOR-SPEC §9.6 placeholder
/// rule.
///
/// **This type carries no fallback.** §9.6's "fixed 1000 mm box when none are
/// declared" is the RENDERER's rule, applied where the placeholder is drawn or
/// hit-tested (ED-0106), so an absent declaration stays absent here rather
/// than being silently materialised into a number a descriptor never said.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extent {
    /// Across the object, in plan.
    pub width_mm: f64,
    /// VERTICAL. Not part of the plan footprint.
    pub height_mm: f64,
    /// Through the object, in plan.
    pub depth_mm: f64,
}

impl Extent {
    /// The plan footprint: `(width, depth)`.
    ///
    /// §9.6 asks for "an axis-aligned box using dimensions declared by the
    /// descriptor", and the plan is two-dimensional, so two of these three
    /// numbers form it. §10.2 names the field `placeholderDimensions` and says
    /// only "width/height/depth in mm"; it does not bind them to axes, which
    /// issue 0012 identified as a gap that has to be answered before the
    /// declared branch of the placeholder rule can be written.
    ///
    /// **Width by depth, with height vertical.** The naive reading — width by
    /// height — is wrong for a floor plan, and issue 0012's example shows why:
    /// a 600 × 2100 × 400 door-like object has a footprint of 600 by 400, and
    /// 2100 is how tall it stands. Taking width by height would draw a
    /// placeholder more than three times too deep and would make a tall thin
    /// object overlap walls it does not touch. The reading also matches
    /// BEHAVIOR-SPEC §2.1, where a level's height fields are elevations and
    /// thicknesses measured vertically, never plan extents.
    ///
    /// This is issue 0026's reading of a gap, recorded rather than assumed,
    /// and it is one of TWO answers the declared branch needs — whether the
    /// declared extent also scales with `scaleUniform` belongs to ED-0106 and
    /// is not settled here.
    pub fn plan_footprint(&self) -> (f64, f64) {
        (self.width_mm, self.depth_mm)
    }
}

/// Attribution metadata (§10.2 `provenance`). **Never functional**: nothing
/// here selects, locates, or validates a blob. Identity is the digest.
///
/// Member names follow FORMAT-SPEC §7.4's normative descriptor example —
/// `displayName`, `sourceDescription`, `author`, `acquiredAt` — not the prose
/// paraphrase in §10.2, which describes the members rather than naming them.
///
/// The BLOCK is required; its members are not. §10.2 requires a structured
/// provenance block and lists what it records, but a photograph a user took
/// themselves has no meaningful source URI or author string, and demanding one
/// would invent a requirement the specification does not state — and invite
/// the fabricated values that any mandatory-but-unknowable field attracts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    /// The name the file had when imported. Display only — never a lookup key,
    /// which is what §10.2's "never functional" means.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Where it came from, as a description or URI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// RFC 3339 UTC (§10.2 `provenance` acquisition date).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acquired_at: Option<String>,
}

/// Licence metadata (§10.2 `license`), optional for user-imported content and
/// surfaced informationally by `validate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct License {
    /// An SPDX expression OR a licence URI. §10.2 permits either and this type
    /// does not adjudicate between them: an SPDX validator is a dependency and
    /// a dataset, and guessing which of the two a string is would make
    /// `validate` wrong in the cases that matter.
    pub expression: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copyright: Option<String>,
}

/// A retrieval hint (§10.2 `hints`, semantics in §10.5/§10.8).
///
/// Timestamps are **advisory**: §10.5 states expiry neither enables nor blocks
/// retrieval, consent does. They exist for display and freshness diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hint {
    pub url: String,
    /// RFC 3339 UTC.
    pub added_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verified_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// ASSET-STRATEGY §10.2, the v0.1 descriptor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetDescriptor {
    /// `sha256:<64 lowercase hex>` per §10.1.
    pub digest: String,
    /// `len(B)` — the logical blob byte sequence. Declared ≠ actual ⇒
    /// rejection (§10.1).
    pub byte_length: u64,
    /// `type/subtype`.
    pub media_type: String,
    /// Token naming the descriptor's intended role (`background-image`,
    /// `furniture-model`, `texture`, …). Drives diagnostics and lookup policy;
    /// this crate does not interpret specific values, because §10.2 gives an
    /// open example list rather than an enumeration.
    pub purpose: String,
    /// The §10.2 `encoding / profile` column: an exact-comparison string
    /// identifying the compression/container profile. Serialised as
    /// `encoding`; the specification names the column with a slash, which is
    /// not a usable key, and `encoding` is the half that names the value.
    pub encoding: String,
    /// `required` | `optional`; anything else, including absent, is
    /// [`Requirement::Undeclared`] or [`Requirement::Absent`] and blocks.
    #[serde(default, skip_serializing_if = "Requirement::is_absent")]
    pub requirement: Requirement,
    pub provenance: Provenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_dimensions: Option<Extent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<Hint>,
    /// Fields this version does not interpret, kept verbatim (FORMAT-SPEC §8
    /// U2, which forbids silent stripping everywhere and not only on
    /// entities).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

/// FORMAT-SPEC §9.3 row **L19** — string and URI field lengths.
///
/// Cited rather than paraphrased, because these are the numbers a hostile or
/// merely careless descriptor is measured against.
pub mod l19 {
    /// `mediaType ≤ 127 B`.
    pub const MEDIA_TYPE_BYTES: usize = 127;
    /// `URI/hint URL ≤ 2,048 B`.
    pub const URI_BYTES: usize = 2_048;
    /// `display strings ≤ 1,024 B`.
    pub const DISPLAY_BYTES: usize = 1_024;
    /// `digest identifier exactly 71 characters` (`sha256:` + 64 lowercase
    /// hex, FIPS 180-4).
    pub const DIGEST_CHARS: usize = 71;
}

/// One thing wrong with a descriptor.
///
/// Validation reports EVERY problem rather than the first. A descriptor is
/// usually wrong in more than one way at once — a hand-edited one especially —
/// and a validator that stops early makes the user fix one field, run again,
/// and find another.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DescriptorError {
    #[error(
        "{digest:?} is not `sha256:` plus 64 lowercase hex ({} characters)",
        l19::DIGEST_CHARS
    )]
    Digest { digest: String },
    #[error("the table key {key:?} does not match the descriptor's own digest {digest:?}")]
    KeyMismatch { key: String, digest: String },
    #[error("{field} is empty, and §10.2 requires a value")]
    Empty { field: &'static str },
    #[error("mediaType {value:?} is not `type/subtype`")]
    MediaType { value: String },
    #[error("{field} is {actual} bytes, over the row L19 limit of {limit}")]
    TooLong {
        field: &'static str,
        actual: usize,
        limit: usize,
    },
    #[error("{field} {value:?} is not an RFC 3339 UTC timestamp")]
    Timestamp { field: &'static str, value: String },
    #[error("hint URL {url:?} is invalid: {reason}")]
    Hint { url: String, reason: String },
    #[error("hint URL {url:?} appears more than once; §10.5 gives hints set semantics")]
    DuplicateHint { url: String },
    #[error("placeholderDimensions.{axis} is {value}; a box needs a finite, non-negative extent")]
    Extent { axis: &'static str, value: f64 },
    #[error("the descriptor under key {key:?} does not parse: {reason}")]
    Unparsable { key: String, reason: String },
    #[error(
        "requirement {value} is neither \"required\" nor \"optional\" (§10.2); it is kept as \
         written and treated as blocking"
    )]
    UnknownRequirement { value: serde_json::Value },
}

impl AssetDescriptor {
    /// Checks the descriptor against §10.2 and row L19, reporting every fault.
    pub fn validate(&self) -> Vec<DescriptorError> {
        let mut faults = Vec::new();

        // The §10.3 mapping is the digest oracle: it accepts exactly
        // `sha256:` + 64 lowercase hex and nothing else, so validating through
        // it cannot drift from what the blob store will accept.
        if domowik_container::blob_entry_name(&self.digest).is_none() {
            faults.push(DescriptorError::Digest {
                digest: self.digest.clone(),
            });
        }

        match &self.requirement {
            Requirement::Required | Requirement::Optional => {}
            // §10.2 makes `requirement` a required field with two values.
            // Reported as a fault, but NOT as a parse failure: the gate still
            // has to be able to read it and fail closed.
            Requirement::Absent => faults.push(DescriptorError::Empty {
                field: "requirement",
            }),
            Requirement::Undeclared(value) => faults.push(DescriptorError::UnknownRequirement {
                value: value.clone(),
            }),
        }

        for (field, value) in [
            ("mediaType", &self.media_type),
            ("purpose", &self.purpose),
            ("encoding", &self.encoding),
        ] {
            if value.is_empty() {
                faults.push(DescriptorError::Empty { field });
            }
        }
        if self.media_type.len() > l19::MEDIA_TYPE_BYTES {
            faults.push(DescriptorError::TooLong {
                field: "mediaType",
                actual: self.media_type.len(),
                limit: l19::MEDIA_TYPE_BYTES,
            });
        }
        if !self.media_type.is_empty() && !is_type_subtype(&self.media_type) {
            faults.push(DescriptorError::MediaType {
                value: self.media_type.clone(),
            });
        }

        for (field, value) in [
            ("provenance.displayName", &self.provenance.display_name),
            (
                "provenance.sourceDescription",
                &self.provenance.source_description,
            ),
            ("provenance.author", &self.provenance.author),
        ] {
            check_display(field, value.as_deref(), &mut faults);
        }
        if let Some(acquired) = &self.provenance.acquired_at {
            if !crate::time::is_rfc3339_utc(acquired) {
                faults.push(DescriptorError::Timestamp {
                    field: "provenance.acquiredAt",
                    value: acquired.clone(),
                });
            }
        }
        if let Some(license) = &self.license {
            check_display("license.expression", Some(&license.expression), &mut faults);
            check_display(
                "license.copyright",
                license.copyright.as_deref(),
                &mut faults,
            );
        }

        if let Some(extent) = self.placeholder_dimensions {
            // §10.2 gives no bound on these, but §9.6 uses them as the sides
            // of an axis-aligned box, and a box cannot have a NaN or negative
            // side. Rejected here rather than left to surface as a placeholder
            // that hit-tests as nothing.
            for (axis, value) in [
                ("widthMm", extent.width_mm),
                ("heightMm", extent.height_mm),
                ("depthMm", extent.depth_mm),
            ] {
                if !value.is_finite() || value < 0.0 {
                    faults.push(DescriptorError::Extent { axis, value });
                }
            }
        }

        let mut seen = std::collections::BTreeSet::new();
        for hint in &self.hints {
            if hint.url.len() > l19::URI_BYTES {
                faults.push(DescriptorError::TooLong {
                    field: "hints[].url",
                    actual: hint.url.len(),
                    limit: l19::URI_BYTES,
                });
            }
            match crate::uri::parse_hint_uri(&hint.url) {
                Ok(parsed) => {
                    // Set semantics are over the NORMALISED form: §10.8 folds
                    // scheme and host, so `HTTPS://Example.com./x` and
                    // `https://example.com/x` are one hint, not two.
                    if !seen.insert(parsed.to_string()) {
                        faults.push(DescriptorError::DuplicateHint {
                            url: hint.url.clone(),
                        });
                    }
                }
                Err(reason) => faults.push(DescriptorError::Hint {
                    url: hint.url.clone(),
                    reason: reason.to_string(),
                }),
            }
            for (field, value) in [
                ("hints[].addedAt", Some(&hint.added_at)),
                ("hints[].lastVerifiedAt", hint.last_verified_at.as_ref()),
                ("hints[].expiresAt", hint.expires_at.as_ref()),
            ] {
                if let Some(value) = value {
                    if !crate::time::is_rfc3339_utc(value) {
                        faults.push(DescriptorError::Timestamp {
                            field,
                            value: value.clone(),
                        });
                    }
                }
            }
        }
        faults
    }
}

fn check_display(field: &'static str, value: Option<&str>, faults: &mut Vec<DescriptorError>) {
    if let Some(value) = value {
        if value.len() > l19::DISPLAY_BYTES {
            faults.push(DescriptorError::TooLong {
                field,
                actual: value.len(),
                limit: l19::DISPLAY_BYTES,
            });
        }
    }
}

/// `type/subtype`, with RFC 9110 token characters on each side.
fn is_type_subtype(value: &str) -> bool {
    let Some((ty, sub)) = value.split_once('/') else {
        return false;
    };
    !ty.is_empty() && !sub.is_empty() && [ty, sub].iter().all(|part| part.bytes().all(is_token))
}

fn is_token(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// The `assets/descriptors.json` entry — FORMAT-SPEC §7.3 and §7.4.
///
/// One object keyed by digest, key order insignificant. The container owns the
/// entry and holds its values opaquely so that fields neither side understands
/// survive a write-back (§8 U2, row L32); this type gives those values their
/// §10.2 meaning and hands nothing back that would lose them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DescriptorTable {
    by_digest: std::collections::BTreeMap<String, AssetDescriptor>,
    /// Entries this version could not interpret, kept exactly as read.
    ///
    /// A descriptor whose key disagrees with its digest, or whose body will
    /// not parse, is not USABLE — but it is still someone's data. Dropping it
    /// from [`DescriptorTable::to_object`] would make a read/write cycle
    /// silently strip it, which is the §8 U2 and row L32 violation this crate
    /// refuses everywhere else. They are carried, not understood.
    carried: std::collections::BTreeMap<String, serde_json::Value>,
}

impl DescriptorTable {
    /// Reads the entry, returning what it could read AND every fault found.
    ///
    /// **Never fails as a whole.** An earlier version returned `Err` when any
    /// descriptor had any fault, which meant one malformed entry made every
    /// OTHER descriptor in the package unreadable — and the field the §9.6
    /// gate needs, `requirement`, invisible exactly when a descriptor is
    /// malformed. Parsing is tolerant, validation is strict, and the two are
    /// reported separately so a caller can choose: `verify` prints the faults,
    /// the open path reads the requirement levels and fails closed.
    ///
    /// §7.4 says the object is "keyed by digest", so a key that disagrees with
    /// the descriptor it holds is dropped rather than resolved in one
    /// direction: trusting the key would look up a descriptor describing other
    /// bytes, and trusting the body would silently lose the entry the key
    /// named. The fault names both.
    pub fn from_object(
        object: &serde_json::Map<String, serde_json::Value>,
    ) -> (Self, Vec<DescriptorError>) {
        let mut by_digest = std::collections::BTreeMap::new();
        let mut carried = std::collections::BTreeMap::new();
        let mut faults = Vec::new();
        for (key, value) in object {
            let descriptor: AssetDescriptor = match serde_json::from_value(value.clone()) {
                Ok(d) => d,
                Err(e) => {
                    faults.push(DescriptorError::Unparsable {
                        key: key.clone(),
                        reason: e.to_string(),
                    });
                    carried.insert(key.clone(), value.clone());
                    continue;
                }
            };
            if &descriptor.digest != key {
                faults.push(DescriptorError::KeyMismatch {
                    key: key.clone(),
                    digest: descriptor.digest.clone(),
                });
                carried.insert(key.clone(), value.clone());
                continue;
            }
            faults.extend(descriptor.validate());
            by_digest.insert(key.clone(), descriptor);
        }
        (Self { by_digest, carried }, faults)
    }

    /// [`DescriptorTable::from_object`], refusing anything with a fault.
    ///
    /// For callers that want a table or an explanation and have no use for a
    /// partial one — writing a package, or `asset verify`.
    pub fn from_object_strict(
        object: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<Self, Vec<DescriptorError>> {
        match Self::from_object(object) {
            (table, faults) if faults.is_empty() => Ok(table),
            (_, faults) => Err(faults),
        }
    }

    /// Writes the entry back in §7.4's shape, INCLUDING what it could not
    /// interpret.
    ///
    /// A read/write cycle through this type is lossless. Entries that failed
    /// to parse, or whose key disagreed with their digest, come back exactly
    /// as they went in — §8 U2 forbids silent stripping "everywhere", and an
    /// entry nobody here understands is the case the rule exists for.
    pub fn to_object(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut out: serde_json::Map<String, serde_json::Value> = self
            .by_digest
            .iter()
            .filter_map(|(digest, descriptor)| {
                serde_json::to_value(descriptor)
                    .ok()
                    .map(|v| (digest.clone(), v))
            })
            .collect();
        for (key, value) in &self.carried {
            out.insert(key.clone(), value.clone());
        }
        out
    }

    /// Keys this version holds but cannot interpret, ascending.
    pub fn carried_keys(&self) -> impl Iterator<Item = &str> {
        self.carried.keys().map(String::as_str)
    }

    pub fn get(&self, digest: &str) -> Option<&AssetDescriptor> {
        self.by_digest.get(digest)
    }

    pub fn iter(&self) -> impl Iterator<Item = &AssetDescriptor> {
        self.by_digest.values()
    }

    pub fn len(&self) -> usize {
        self.by_digest.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_digest.is_empty()
    }

    /// Adds or replaces a descriptor, keyed by its own digest.
    pub fn insert(&mut self, descriptor: AssetDescriptor) {
        self.by_digest.insert(descriptor.digest.clone(), descriptor);
    }

    pub fn remove(&mut self, digest: &str) -> Option<AssetDescriptor> {
        self.by_digest.remove(digest)
    }

    /// Every descriptor, as the slice the resolver takes.
    pub fn as_vec(&self) -> Vec<AssetDescriptor> {
        self.by_digest.values().cloned().collect()
    }
}

/// Media types this crate can recognise from a file's leading bytes.
///
/// Sniffed from CONTENT, never from a filename extension. A descriptor's
/// `mediaType` drives §9.6's decodability row, and a name is something a user
/// typed — `plan.png` holding a JPEG would otherwise produce a descriptor that
/// says one thing while the blob is another, which is exactly the disagreement
/// content addressing exists to prevent.
///
/// Unrecognised content becomes `application/octet-stream`. §10.2 requires a
/// `mediaType`, and the honest one for bytes nothing here identifies is the
/// one that claims nothing.
pub fn sniff_media_type(bytes: &[u8]) -> &'static str {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.starts_with(PNG) {
        return "image/png";
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return "image/jpeg";
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return "image/gif";
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return "image/webp";
    }
    if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
        return "image/svg+xml";
    }
    "application/octet-stream"
}

impl AssetDescriptor {
    /// A descriptor for a freshly imported file.
    ///
    /// `digest` and `byte_length` come from the import that produced them, not
    /// from re-reading the file: §10.1 says a declared length that disagrees
    /// with the bytes is a rejection, so both have to describe the SAME read.
    ///
    /// `requirement` is the caller's, because it is a product decision and not
    /// a property of the bytes — a background scan a user traces over is
    /// optional, a model a plan cannot be understood without is not.
    ///
    /// `encoding` is `identity`: this crate never transcodes, and §10.1 makes
    /// embedding a byte copy, so the stored form is the imported form.
    pub fn for_import(
        digest: impl Into<String>,
        byte_length: u64,
        leading_bytes: &[u8],
        purpose: impl Into<String>,
        requirement: Requirement,
        display_name: Option<String>,
        acquired_at: Option<String>,
    ) -> Self {
        Self {
            digest: digest.into(),
            byte_length,
            media_type: sniff_media_type(leading_bytes).to_owned(),
            purpose: purpose.into(),
            encoding: "identity".to_owned(),
            requirement,
            provenance: Provenance {
                // §10.2: the original display name is "never functional". It is
                // recorded because a user needs to recognise their own file in
                // a diagnostic, and used for nothing else.
                display_name,
                source_description: Some("user import".to_owned()),
                author: None,
                acquired_at,
            },
            license: None,
            placeholder_dimensions: None,
            hints: Vec::new(),
            unknown: serde_json::Map::new(),
        }
    }
}
