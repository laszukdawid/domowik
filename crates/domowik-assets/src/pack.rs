//! Package profiles — ASSET-STRATEGY §10.4, and the size classes issue 0026
//! asks `asset list` to distinguish.
//!
//! Planning is separated from writing on purpose. §10.4 requires a conversion
//! to be "explicit, all-or-nothing, and report embedded-vs-external byte
//! totals BEFORE packing", which is only possible if the whole decision — what
//! goes in, what stays out, what is missing, what it all weighs — exists as a
//! value before a single byte is written. [`plan_pack`] produces that value
//! and writes nothing.
//!
//! The two profiles fail in opposite directions, which is the point:
//!
//! - **Portable** must embed every blob in `U(P)`. Anything it cannot embed
//!   and verify aborts the whole conversion with a report naming EVERY such
//!   asset, not the first one — a report that stops early makes a user fix one
//!   thing, run again, and find another.
//! - **Thin** must NOT embed them, and therefore has to know they survive
//!   elsewhere. A thin package written over blobs that are in neither the
//!   cache nor a hint dangles by construction, so that is an omission too.

use std::collections::{BTreeMap, BTreeSet};

use crate::cache::CacheStore;
use crate::descriptor::AssetDescriptor;

/// Which profile a conversion targets (§10.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Blobs live in the cache or behind a policy-gated hint.
    Thin,
    /// Every blob in `U(P)` is embedded and digest-verified.
    Portable,
}

impl Profile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Thin => "thin",
            Self::Portable => "portable",
        }
    }
}

/// One asset a conversion cannot honour, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Omission {
    pub digest: String,
    pub reason: String,
}

/// The three byte classes issue 0026 requires `asset list` to keep apart.
///
/// `remote_bytes` is **declared, never measured**: it is the sum of the
/// `byteLength` of descriptors whose blob is neither embedded nor cached. No
/// retrieval happens, so nothing here has weighed those bytes, and reporting
/// them in the same breath as measured totals without saying so would invite
/// a user to trust a number nobody checked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SizeTotals {
    /// Blobs embedded in the package, measured.
    pub package_bytes: u64,
    /// Blobs resident in the local content cache, measured.
    pub cache_bytes: u64,
    /// Declared size of blobs held neither in the package nor the cache.
    pub remote_bytes: u64,
}

/// A conversion, decided but not performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackPlan {
    pub profile: Profile,
    /// Digests the new package will embed, ascending.
    pub embed: Vec<String>,
    /// Digests the new package will leave outside itself, ascending.
    pub external: Vec<String>,
    /// Blobs carried across even though `U(P)` does not name them.
    ///
    /// FORMAT-SPEC §7.4 invariant I2: a blob referenced only by a field this
    /// version drops into an `unknown` map looks exactly like an orphan from
    /// here. §6 requires collection to be fail-closed, so these are kept.
    pub carried: Vec<String>,
    pub totals: SizeTotals,
}

/// Why a conversion was refused, with every failing asset named.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "cannot write a {profile} package: {} of {universe} referenced asset(s) cannot be honoured",
    omissions.len()
)]
pub struct OmissionReport {
    pub profile: &'static str,
    pub universe: usize,
    pub omissions: Vec<Omission>,
}

impl OmissionReport {
    /// The report as a user reads it: one line per asset, ascending by digest.
    pub fn render(&self) -> String {
        let mut out = format!("{self}\n");
        for omission in &self.omissions {
            out.push_str(&format!("  {}: {}\n", omission.digest, omission.reason));
        }
        out
    }
}

/// Everything a conversion looks at.
pub struct PackInputs<'a> {
    /// `U(P)` — the referenced-blob universe (§10.5).
    pub universe: &'a BTreeSet<String>,
    /// Blobs embedded in the source package.
    pub embedded: &'a BTreeMap<String, Vec<u8>>,
    pub cache: Option<&'a CacheStore>,
    pub descriptors: &'a [AssetDescriptor],
}

/// Decides a conversion, or refuses it with a complete omission report.
///
/// Writes nothing and mutates nothing.
pub fn plan_pack(profile: Profile, inputs: &PackInputs<'_>) -> Result<PackPlan, OmissionReport> {
    let by_digest: BTreeMap<&str, &AssetDescriptor> = inputs
        .descriptors
        .iter()
        .map(|d| (d.digest.as_str(), d))
        .collect();

    let mut omissions = Vec::new();
    let mut embed = Vec::new();
    let mut external = Vec::new();
    let mut totals = SizeTotals::default();

    for digest in inputs.universe {
        match profile {
            Profile::Portable => match inputs.embedded.get(digest) {
                Some(bytes) => {
                    // §10.4: every embedded blob is digest-verified. This is
                    // unreachable through `read_package`, which verifies each
                    // blob against the digest its §10.3 name encodes and
                    // refuses the whole package otherwise. It stays because it
                    // is the stated contract, and a future path that builds a
                    // package some other way would need it.
                    let actual = domowik_container::digest_of(bytes);
                    if &actual != digest {
                        omissions.push(Omission {
                            digest: digest.clone(),
                            reason: format!("stored bytes hash to {actual}"),
                        });
                    } else {
                        totals.package_bytes += bytes.len() as u64;
                        embed.push(digest.clone());
                    }
                }
                None => match cached_intact(inputs.cache, digest) {
                    // A blob the cache holds CAN be embedded: §10.1 makes
                    // embedding a byte copy, and the cache copy has just been
                    // re-hashed. That is the whole point of having a cache
                    // when converting a thin package to a portable one.
                    Some(bytes_len) => {
                        totals.package_bytes += bytes_len;
                        embed.push(digest.clone());
                    }
                    None => omissions.push(Omission {
                        digest: digest.clone(),
                        reason: unresolved_reason(inputs, digest),
                    }),
                },
            },
            Profile::Thin => {
                // Thin embeds no member of U(P) — §10.4 reserves embedding for
                // preview-class blobs, and §10.5 makes previews non-members.
                // So the only question is whether the blob survives outside.
                if let Some(bytes_len) = cached_intact(inputs.cache, digest) {
                    totals.cache_bytes += bytes_len;
                    external.push(digest.clone());
                } else if has_hint(&by_digest, digest) {
                    totals.remote_bytes +=
                        by_digest.get(digest.as_str()).map_or(0, |d| d.byte_length);
                    external.push(digest.clone());
                } else {
                    omissions.push(Omission {
                        digest: digest.clone(),
                        reason: thin_reason(inputs, digest),
                    });
                }
            }
        }
    }

    if !omissions.is_empty() {
        omissions.sort_by(|a, b| a.digest.cmp(&b.digest));
        return Err(OmissionReport {
            profile: profile.as_str(),
            universe: inputs.universe.len(),
            omissions,
        });
    }

    let mut carried: Vec<String> = inputs
        .embedded
        .keys()
        .filter(|d| !inputs.universe.contains(*d))
        .cloned()
        .collect();
    carried.sort();
    // Carried blobs are still in the package, so they still weigh something.
    for digest in &carried {
        totals.package_bytes += inputs.embedded[digest].len() as u64;
    }

    embed.sort();
    external.sort();
    Ok(PackPlan {
        profile,
        embed,
        external,
        carried,
        totals,
    })
}

/// The cache copy's size, if it is present AND still hashes to its name.
fn cached_intact(cache: Option<&CacheStore>, digest: &str) -> Option<u64> {
    let cache = cache?;
    match cache.verify(digest) {
        Ok(Some(true)) => cache.stored_bytes(digest),
        _ => None,
    }
}

fn has_hint(by_digest: &BTreeMap<&str, &AssetDescriptor>, digest: &str) -> bool {
    by_digest.get(digest).is_some_and(|d| !d.hints.is_empty())
}

fn unresolved_reason(inputs: &PackInputs<'_>, digest: &str) -> String {
    match inputs.cache.map(|c| c.verify(digest)) {
        Some(Ok(Some(false))) => {
            "the cache holds it, but its bytes no longer match the digest".to_owned()
        }
        _ => "not present in this package or the cache".to_owned(),
    }
}

fn thin_reason(inputs: &PackInputs<'_>, digest: &str) -> String {
    match inputs.cache.map(|c| c.verify(digest)) {
        Some(Ok(Some(false))) => {
            "the cache holds it, but its bytes no longer match the digest, so a thin package \
             would dangle"
                .to_owned()
        }
        _ if inputs.embedded.contains_key(digest) => {
            "embedded here but absent from the cache, and no hint names it: admit it to the \
             cache first, or pack portable"
                .to_owned()
        }
        _ => {
            "absent from the cache and named by no hint, so a thin package would dangle".to_owned()
        }
    }
}

/// Size totals for a package as it stands, without planning a conversion.
///
/// The same three classes, so `asset list` and a pack preflight cannot
/// disagree about what a package weighs.
pub fn size_totals(inputs: &PackInputs<'_>) -> SizeTotals {
    let by_digest: BTreeMap<&str, &AssetDescriptor> = inputs
        .descriptors
        .iter()
        .map(|d| (d.digest.as_str(), d))
        .collect();

    let mut totals = SizeTotals::default();
    for bytes in inputs.embedded.values() {
        totals.package_bytes += bytes.len() as u64;
    }
    // An embedded blob is never ALSO counted as cache or remote bytes. The
    // three classes partition the assets; double counting would make them sum
    // to more than the user has.
    for digest in inputs.universe {
        if inputs.embedded.contains_key(digest) {
            continue;
        }
        if let Some(len) = cached_intact(inputs.cache, digest) {
            totals.cache_bytes += len;
        } else if let Some(descriptor) = by_digest.get(digest.as_str()) {
            totals.remote_bytes += descriptor.byte_length;
        }
    }
    totals
}
