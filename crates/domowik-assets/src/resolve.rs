//! Per-instance asset resolution — BEHAVIOR-SPEC §9.6, ASSET-STRATEGY §10.6.
//!
//! This is the answer to "is this document degraded, and which instances are
//! why". It is deliberately a SNAPSHOT and deliberately GEOMETRY-FREE.
//!
//! A snapshot rather than a standing assumption, because the alternative that
//! was in place — treat every asset instance as permanently degraded — is
//! unfalsifiable: it renders a placeholder for an asset that is sitting in the
//! package, and no test can distinguish it from the case where resolution is
//! genuinely broken. Resolution is computed per open, from the real §10.6
//! lookup order, and reports `Resolved` when the blob is there.
//!
//! Geometry-free, because the placeholder BOX belongs to whoever draws and
//! hit-tests it (ED-0106). This module reports the extent a descriptor
//! DECLARED, or `None`; §9.6's "fixed 1000 mm box" fallback is applied at the
//! rendering end. Synthesising the fallback here would put a number into a
//! data structure that no descriptor ever said, and every consumer downstream
//! would then be unable to tell a declared 1000 mm box from an absent one.
//!
//! Nothing here enforces anything. It is a query: it reports `requirement` as
//! data, and whether a command is permitted while degraded is the command
//! layer's decision.

use std::collections::{BTreeMap, BTreeSet};

use uuid::Uuid;

use crate::cache::CacheStore;
use crate::descriptor::{AssetDescriptor, Extent, Requirement};
use crate::policy::RetrievalPolicy;

/// The three fields of an asset instance that resolution needs.
///
/// Not `domowik_format::AssetInstance` itself: that type is owned by the
/// format crate and frozen at FORMAT-SPEC §2.3's Tier B shape, and resolution
/// has no business depending on its transform, its display name or its unknown
/// map. Callers project into this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceRef {
    pub instance_id: Uuid,
    pub level_id: Uuid,
    pub digest: String,
}

/// Why an instance is degraded. §9.6 treats every one of these identically for
/// the purpose of the open-mode decision; they differ only in what the
/// diagnostic tells the user to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DegradedReason {
    /// No blob under that digest, anywhere in the lookup order.
    Missing,
    /// A blob is present and its bytes do not match the digest. §9.6: "digest
    /// mismatch is treated exactly like a missing blob."
    DigestMismatch,
    /// Present and valid, but this build cannot decode the media type.
    UnsupportedMediaType { media_type: String },
    /// Present, valid, of a supported type, and still undecodable — truncated
    /// or malformed content inside a container this build understands.
    Undecodable,
    /// Not present locally, and retrieval is off or the host is offline.
    /// §9.6 counts these in the offline summary rather than as per-asset noise.
    RetrievalDisabledOffline,
    /// The instance names a digest no descriptor describes. §10.6: well-formed
    /// but unresolved ⇒ treated as REQUIRED-missing.
    NoDescriptor,
}

/// Whether an instance's asset resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionState {
    Resolved,
    Degraded(DegradedReason),
}

impl ResolutionState {
    pub fn is_degraded(&self) -> bool {
        matches!(self, Self::Degraded(_))
    }
}

/// One instance's resolution, independent of geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceResolution {
    pub instance_id: Uuid,
    pub level_id: Uuid,
    pub digest: String,
    /// `Required` when no descriptor names the digest. §10.6 makes a
    /// well-formed but UNRESOLVED undescribed reference required-missing, so
    /// an instance cannot escape the blocking branch by having no descriptor;
    /// an undescribed reference whose blob IS present resolves, and stays
    /// `Required` because nothing has said it is optional.
    pub requirement: Requirement,
    pub state: ResolutionState,
    /// The descriptor's `placeholderDimensions`, verbatim. `None` means the
    /// descriptor declared none — NOT that the extent is zero, and not a
    /// substitute for §9.6's fallback box.
    pub declared_extent: Option<Extent>,
}

/// What this build can decode.
///
/// Supplied by the caller rather than hardcoded, because the set is a property
/// of the renderer and the enabled features, not of the asset layer. The CLI's
/// PNG export and a future GUI do not necessarily agree, and a list baked in
/// here would make one of them wrong.
#[derive(Debug, Clone, Default)]
pub struct MediaSupport {
    decodable: BTreeSet<String>,
}

impl MediaSupport {
    /// A build that decodes nothing. Every present blob then resolves as
    /// `UnsupportedMediaType`, which is §9.6's row for exactly that case.
    pub fn none() -> Self {
        Self::default()
    }

    pub fn new(types: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            decodable: types
                .into_iter()
                .map(|t| t.into().to_ascii_lowercase())
                .collect(),
        }
    }

    pub fn supports(&self, media_type: &str) -> bool {
        self.decodable.contains(&media_type.to_ascii_lowercase())
    }
}

/// Where blobs may be found, in §10.6's order.
pub struct Lookup<'a> {
    /// Blobs embedded in the package. `domowik_container::Package::assets`.
    pub embedded: &'a BTreeMap<String, Vec<u8>>,
    pub cache: Option<&'a CacheStore>,
    pub retrieval: &'a RetrievalPolicy,
    pub media: &'a MediaSupport,
}

/// Where a blob was found, if anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Embedded,
    Cache,
}

impl Lookup<'_> {
    /// §10.6's lookup order. Returns where the blob is, and whether its bytes
    /// still match its digest.
    ///
    /// The embedded step does not re-hash: `read_package` already verified
    /// every embedded blob against the digest its §10.3 name encodes and
    /// refused the package otherwise, so a second hash here would be work
    /// proportional to the package on every open, proving something already
    /// proven. The CACHE step DOES verify, because nothing verified it on the
    /// way in from an earlier session and a cache file is an ordinary file
    /// anybody can damage.
    fn find(&self, digest: &str) -> Option<(Origin, bool)> {
        if self.embedded.contains_key(digest) {
            return Some((Origin::Embedded, true));
        }
        if let Some(cache) = self.cache {
            // A cache the process cannot read is not evidence that the blob
            // is absent, but it is not evidence that it is present either, and
            // §9.6 has one safe direction: fall through to missing.
            if let Ok(Some(intact)) = cache.verify(digest) {
                return Some((Origin::Cache, intact));
            }
        }
        None
    }
}

/// Whether a digest is available AND usable on this device.
///
/// This is the predicate the container's open-mode gate consumes: it decides
/// the BEHAVIOR-SPEC §9.6 matrix, and this crate answers only the availability
/// column. One boolean covers every failure row on purpose — §9.6 says digest
/// mismatch is treated exactly like a missing blob, and undecodable and
/// unsupported-media-type land in the same column — so there is nothing for a
/// richer return type to say that the matrix would act on differently.
///
/// Embedded blobs are the CALLER's business and are not consulted here: the
/// container counts them as resolved before reaching this predicate, so a
/// portable package never opens degraded on a machine with no cache.
pub fn availability<'a>(lookup: &'a Lookup<'a>) -> impl Fn(&str) -> bool + 'a {
    move |digest: &str| matches!(lookup.find(digest), Some((_, true)))
}

/// Every instance's resolution, plus what §9.6 derives from the set.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolutionSnapshot {
    entries: Vec<InstanceResolution>,
}

/// §9.6's open mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    Editable,
    /// Pan, zoom, viewing and diagnostics work, and `info` and `validate`
    /// operate; interactive editing and normal saving do not.
    ReadOnlyDegraded,
}

/// One actionable diagnostic (§9.6: identity, purpose, remediation).
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub digest: String,
    /// The descriptor's original display name, when it had one. Display only.
    pub name: Option<String>,
    pub requirement: Requirement,
    pub reason: DegradedReason,
    /// Instances referencing this digest, ascending.
    pub instances: Vec<Uuid>,
    /// Levels those instances sit on, ascending.
    pub levels: Vec<Uuid>,
    pub remediation: String,
    /// §9.6 calls a required failure blocking and an optional one not.
    pub blocking: bool,
}

/// Computes the snapshot.
///
/// Deterministic: the same instances, descriptors, package and policy produce
/// the same snapshot, which is what §9.6's reopen rule requires ("identical
/// conditions produce identical open modes, diagnostics, and placeholder
/// geometry"). Entries are ordered by instance id.
pub fn resolve(
    instances: &[InstanceRef],
    descriptors: &[AssetDescriptor],
    lookup: &Lookup<'_>,
) -> ResolutionSnapshot {
    let by_digest: BTreeMap<&str, &AssetDescriptor> =
        descriptors.iter().map(|d| (d.digest.as_str(), d)).collect();

    let mut entries: Vec<InstanceResolution> = instances
        .iter()
        .map(|instance| {
            let descriptor = by_digest.get(instance.digest.as_str()).copied();
            let requirement = descriptor.map_or(Requirement::Required, |d| d.requirement.clone());
            let declared_extent = descriptor.and_then(|d| d.placeholder_dimensions);
            let state = state_for(instance, descriptor, lookup);
            InstanceResolution {
                instance_id: instance.instance_id,
                level_id: instance.level_id,
                digest: instance.digest.clone(),
                requirement,
                state,
                declared_extent,
            }
        })
        .collect();
    entries.sort_by_key(|e| e.instance_id);
    ResolutionSnapshot { entries }
}

fn state_for(
    instance: &InstanceRef,
    descriptor: Option<&AssetDescriptor>,
    lookup: &Lookup<'_>,
) -> ResolutionState {
    let Some(descriptor) = descriptor else {
        // §10.6's class is a "DANGLING instance→digest reference with no
        // descriptor", and its required-missing rule is scoped to the
        // sub-case "well-formed but UNRESOLVED". Both qualifiers matter. A
        // reference whose blob is present and digest-verified is neither
        // dangling nor unresolved, so it never enters this class — and
        // calling it missing would contradict §9.6, which defines missing as
        // the blob not being there.
        //
        // Reading it the other way has a consequence worth naming: descriptors
        // do not persist yet, so every asset instance would be undescribed,
        // and a single instance would put the whole document into read-only
        // degraded inspection. That is not what makes the narrow reading
        // right, but it is what made the breadth of the wide one visible.
        //
        // A MALFORMED digest never reaches here: FORMAT-SPEC §9 rejects it as
        // hostile input at parse time, which is a louder failure than this.
        return match lookup.find(&instance.digest) {
            Some((_, true)) => ResolutionState::Resolved,
            Some((_, false)) => ResolutionState::Degraded(DegradedReason::DigestMismatch),
            None => ResolutionState::Degraded(DegradedReason::NoDescriptor),
        };
    };
    match lookup.find(&instance.digest) {
        Some((_, false)) => ResolutionState::Degraded(DegradedReason::DigestMismatch),
        Some((_, true)) => {
            if !lookup.media.supports(&descriptor.media_type) {
                ResolutionState::Degraded(DegradedReason::UnsupportedMediaType {
                    media_type: descriptor.media_type.clone(),
                })
            } else {
                ResolutionState::Resolved
            }
        }
        None => {
            // Nothing was fetched, because nothing here fetches. With consent
            // granted the honest report is still that the blob is absent
            // locally; a retrieval path would replace this arm, and until one
            // exists saying "offline" when consent is off is the accurate
            // half of §9.6's offline summary.
            if lookup.retrieval.consent {
                ResolutionState::Degraded(DegradedReason::Missing)
            } else {
                ResolutionState::Degraded(DegradedReason::RetrievalDisabledOffline)
            }
        }
    }
}

impl ResolutionSnapshot {
    /// One instance's resolution.
    pub fn get(&self, instance_id: Uuid) -> Option<&InstanceResolution> {
        self.entries
            .binary_search_by_key(&instance_id, |e| e.instance_id)
            .ok()
            .map(|i| &self.entries[i])
    }

    /// Every instance, ascending by id — RESOLVED ONES INCLUDED.
    ///
    /// Listing the resolved instances too is deliberate: a consumer that hit-
    /// tests placeholders should not have to ask a second source what the
    /// snapshot left out, and a snapshot that silently filtered would make
    /// this module a hidden participant in selection.
    pub fn iter(&self) -> impl Iterator<Item = &InstanceResolution> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// §9.6's open mode: blocked only by a REQUIRED asset that did not
    /// resolve. An optional one opens editable with a non-blocking diagnostic.
    pub fn open_mode(&self) -> OpenMode {
        // `Undeclared` blocks with `Required`: §9.6 has no row for a level it
        // does not define, and the safe reading of a missing row refuses to
        // edit. This matches what the container's own gate does with the same
        // field, which is the point — disagreeing on malformed values is the
        // divergence that would be hardest to notice.
        if self
            .entries
            .iter()
            .any(|e| e.requirement.blocks_editing() && e.state.is_degraded())
        {
            OpenMode::ReadOnlyDegraded
        } else {
            OpenMode::Editable
        }
    }

    /// How many DISTINCT assets are unavailable offline.
    ///
    /// §9.6: "the interface shows one summary of how many assets are
    /// unavailable rather than per-asset noise." Counted by digest, not by
    /// instance — twenty chairs from one missing model are one unavailable
    /// asset, and reporting twenty is the noise the rule exists to prevent.
    pub fn offline_unavailable(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| {
                matches!(
                    e.state,
                    ResolutionState::Degraded(DegradedReason::RetrievalDisabledOffline)
                )
            })
            .map(|e| e.digest.as_str())
            .collect::<BTreeSet<_>>()
            .len()
    }

    /// One diagnostic per affected asset, ascending by digest (§9.6).
    pub fn diagnostics(&self, descriptors: &[AssetDescriptor]) -> Vec<Diagnostic> {
        let by_digest: BTreeMap<&str, &AssetDescriptor> =
            descriptors.iter().map(|d| (d.digest.as_str(), d)).collect();
        let mut grouped: BTreeMap<&str, Diagnostic> = BTreeMap::new();
        for entry in &self.entries {
            let ResolutionState::Degraded(reason) = &entry.state else {
                continue;
            };
            let slot = grouped
                .entry(entry.digest.as_str())
                .or_insert_with(|| Diagnostic {
                    digest: entry.digest.clone(),
                    name: by_digest
                        .get(entry.digest.as_str())
                        .and_then(|d| d.provenance.display_name.clone()),
                    requirement: entry.requirement.clone(),
                    reason: reason.clone(),
                    instances: Vec::new(),
                    levels: Vec::new(),
                    remediation: remediation_for(reason),
                    blocking: entry.requirement.blocks_editing(),
                });
            slot.instances.push(entry.instance_id);
            if !slot.levels.contains(&entry.level_id) {
                slot.levels.push(entry.level_id);
            }
        }
        let mut out: Vec<Diagnostic> = grouped.into_values().collect();
        for d in &mut out {
            d.instances.sort();
            d.levels.sort();
        }
        out
    }
}

/// §9.6's remediation list: restore the blob into the cache, re-link the file,
/// repack portable, or remove the referencing instance.
fn remediation_for(reason: &DegradedReason) -> String {
    match reason {
        DegradedReason::Missing | DegradedReason::RetrievalDisabledOffline => {
            "restore the blob into the cache, re-link the file, repack the package as portable, \
             or remove the referencing instance"
                .to_owned()
        }
        DegradedReason::DigestMismatch => {
            "the stored bytes do not match the digest: restore an intact copy into the cache or \
             repack the package as portable; removing the referencing instance also clears it"
                .to_owned()
        }
        DegradedReason::UnsupportedMediaType { media_type } => format!(
            "this build cannot decode {media_type}: the bytes are intact and preserved, so \
             re-import the asset in a supported encoding or open it on a build that decodes it"
        ),
        DegradedReason::Undecodable => {
            "the bytes are of a supported type but will not decode: re-import the asset from an \
             intact source, or remove the referencing instance"
                .to_owned()
        }
        DegradedReason::NoDescriptor => {
            "no descriptor describes this digest: re-import the asset so a descriptor is written, \
             or remove the referencing instance"
                .to_owned()
        }
    }
}
