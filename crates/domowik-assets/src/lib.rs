//! Asset descriptors, the local content cache, and package profiles.
//!
//! This crate implements ASSET-STRATEGY §10 — the parts of it that describe
//! assets rather than the container that carries them. It sits ABOVE
//! `domowik-container`, which owns the package file, §10.1 digest identity and
//! the §10.3 blob path mapping, and it adds nothing to it.
//!
//! Three things live here that live nowhere else:
//!
//! - [`AssetDescriptor`] (§10.2). BEHAVIOR-SPEC §9.6's condition matrix keys
//!   on a descriptor's `requirement`, and its placeholder rule reads a
//!   descriptor's `placeholderDimensions`. Without descriptors, "is this
//!   document degraded" has no answer and every instance has to be ASSUMED
//!   degraded, which is both wrong and unfalsifiable.
//! - [`CacheStore`] (§10.6). The second step of the lookup order, and where a
//!   thin package's blobs live.
//! - The §10.7 collector ([`gc`]), whose pins and leases are what make the
//!   cache seam crash-safe. The package's own §9.7 atomic replace covers one
//!   file and cannot cover a second store (issue 0029), so the cache carries
//!   its own guarantee: over-retention on a crash, never premature deletion.
//!
//! **Nothing in this crate performs network I/O.** §10.8's retrieval is
//! consent-gated and disabled by default, and the hint machinery here parses
//! and validates; it does not fetch.

pub mod cache;
pub mod descriptor;
pub mod gc;
pub mod pack;
pub mod policy;
pub mod resolve;
pub mod time;
pub mod uri;

pub use cache::{Admission, AdmissionLimits, CacheError, CacheStore};
pub use descriptor::{
    sniff_media_type, AssetDescriptor, DescriptorError, DescriptorTable, Extent, Hint, License,
    Provenance, Requirement,
};
pub use pack::{
    plan_pack, size_totals, Omission, OmissionReport, PackInputs, PackPlan, Profile, SizeTotals,
};
pub use policy::{Destination, IpNetwork, RetrievalPolicy};
pub use resolve::{
    availability, resolve, DegradedReason, Diagnostic, InstanceRef, InstanceResolution, Lookup,
    MediaSupport, OpenMode, ResolutionSnapshot, ResolutionState,
};
pub use uri::{parse_hint_uri, HintUri, UriError};

impl AdmissionLimits {
    /// FORMAT-SPEC §9.3 row **L5** — per-blob logical bytes `len(B)`, the
    /// asset-admission ceiling and the hash domain of ASSET-STRATEGY §10.1.
    ///
    /// One number across the cache, embedded and retrieval paths: §9.3's own
    /// rationale for the row. L24, the retrieval transfer cap, is set equal to
    /// it so retrieval never fails where local admission would have succeeded.
    pub const L5_BLOB_BYTES: u64 = 536_870_912;
}

impl Default for AdmissionLimits {
    fn default() -> Self {
        Self {
            max_blob_bytes: Self::L5_BLOB_BYTES,
        }
    }
}
