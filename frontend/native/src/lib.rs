//! A C ABI over the Domowik core's stateful `Session` seam (issue 0024).
//!
//! **This is a spike.** Nothing here is a product contract, nothing is
//! distributed, and the ABI is expected to change. Its job is to answer ADR
//! 0003's Phase 2 gate against a *Rust* core: can Flutter drive this boundary,
//! and what does the boundary cost and oblige?
//!
//! # What changed by moving the core from Go to Rust
//!
//! Two obligations disappeared. There is no managed runtime to initialise on a
//! foreign thread or to shut down, so the Go slice's finding F3 ("no shutdown
//! story exists for c-shared") is simply absent. And there is no garbage
//! collector, so the whole no-Go-pointer rule — the largest single source of
//! complexity in the Go bridge — has no analogue.
//!
//! Two obligations got harder, and this crate exists mostly to discharge them:
//!
//! - **A panic must not cross `extern "C"`.** Rust aborts the process rather
//!   than unwinding into foreign frames. For a CLI that is merely bad; for a
//!   GUI it means one malformed request kills the user's editor with their
//!   work in it. Every entry point below is wrapped in [`guard`].
//! - **Ownership must be transported explicitly.** Rust's allocator is not
//!   libc's, so a buffer this library returns can only be freed by this
//!   library. There is no runtime to outlive the mistake.
//!
//! # Boundary rules, as implemented
//!
//! - **No Rust reference escapes.** Plans are addressed by opaque `u64`
//!   handles resolved through a registry. Handing out `Box::into_raw` would be
//!   faster and would make a forged or stale handle undefined behaviour —
//!   across a boundary where the caller is Dart, and any integer can arrive,
//!   that is not a trade worth taking. A forged handle here is *detected*.
//! - **Buffers transfer to the caller** and are freed only by
//!   [`domowik_buffer_free`], which takes the buffer's monotonic release
//!   TOKEN rather than its pointer. Tokens are never reused, so an unknown or
//!   already-reclaimed one is refused exactly, with
//!   [`status::UNKNOWN_BUFFER`]. A double free cannot reach the allocator, and
//!   two buffers can never share a key — which a pointer-keyed registry could
//!   not promise; see [`DomowikBuf::token`].
//! - **Request buffers are borrowed read-only** for the duration of the call
//!   and copied before anything retains them.
//! - **Threading:** calls that touch a plan's SESSION serialise on that
//!   plan's lock; distinct handles proceed in parallel, and the registry lock
//!   is held only for the lookup, never across core work.
//!
//!   Two exports deliberately do NOT take the plan lock, and saying "every
//!   call on one handle serialises" would be false: [`domowik_cancel`] must
//!   not wait for the work it is cancelling, and [`domowik_busy`] is the probe
//!   it cancels. Both touch only the cancellation flag, which lives outside
//!   the mutex for exactly this reason.
//! - **Errors:** a sticky per-handle last-error string, valid from a failing
//!   return until the next call on that handle. Retrieving it does not clear
//!   it.
//! - **Cancellation:** a per-handle atomic flag, observed by long operations.
//!
//! # Platform support
//!
//! Executed on macOS/arm64 only. See the spike README for what is
//! assessed-and-deferred and why. Nothing is simulated.

use domowik_container::Plan as ContainerPlan;
use domowik_document::operation::Operation;
use domowik_document::session::Session;
use domowik_format::{Document, Entity, OpeningKind, DERIVED_COORD_BOUND_MM};
use domowik_geometry::hit::{
    placeholder_geometry, AssetFootprint, HitClass, HitIndex, Placeholder, PlaceholderGeometry,
};
use domowik_geometry::incremental::LevelGeometry;
use domowik_geometry::polygon::Polygon;
use domowik_geometry::rooms;
use domowik_geometry::snap::{SnapClass, SnapError, SnapFeature, SnapIndex, SnapQuery};
use domowik_geometry::spatial::Aabb;
use domowik_geometry::walls;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::{c_char, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard};
use uuid::Uuid;

mod asset_library;
#[cfg(test)]
#[path = "../../../crates/domowik-models/tests/support/mod.rs"]
mod texture_fixture;
pub use asset_library::{domowik_plan_assets, domowik_plan_import_model, domowik_plan_scene_frame};
#[cfg(target_os = "macos")]
mod render3d;
#[cfg(target_os = "wasi")]
mod web;

/// Status codes. Zero is success; everything else is a failure the caller can
/// distinguish without parsing a string.
pub mod status {
    /// The call succeeded.
    pub const OK: i32 = 0;
    /// A null pointer, a length that cannot be represented, or a
    /// non-UTF-8 request where UTF-8 is required.
    pub const INVALID_ARGUMENT: i32 = 1;
    /// The handle was never issued, or has been closed. NOT undefined
    /// behaviour: forged and stale handles are detected, which is the reason
    /// handles are registry tokens rather than pointers.
    pub const UNKNOWN_HANDLE: i32 = 2;
    /// The pointer was not issued by this library, or has already been freed.
    pub const UNKNOWN_BUFFER: i32 = 3;
    /// The request was not valid JSON, or not the shape this ABI expects.
    pub const MALFORMED_REQUEST: i32 = 4;
    /// The core refused the command. The reason is in the last-error string.
    pub const REJECTED: i32 = 5;
    /// A panic was contained at the boundary. The library remains usable.
    pub const PANIC: i32 = 6;
    /// The operation observed a cancellation request and stopped.
    pub const CANCELED: i32 = 7;
    /// A monotonic id space is exhausted. Unreachable in practice — it takes
    /// 2^64 issues — but the alternative is wrapping, which would silently
    /// break the never-reused guarantee handles and buffer tokens rest on.
    pub const EXHAUSTED: i32 = 8;
}

/// The ABI revision. A caller that reads a different number must not proceed.
///
/// Moved twice already, which is the point of having it. **2** when keying
/// buffer release by token changed `DomowikBuf`'s layout and
/// `domowik_buffer_free`'s signature; **3** when the review's fixes changed
/// close's observable semantics, added `EXHAUSTED`, and added two debug
/// exports. A version field nobody ever bumps is decoration, and this is a
/// spike — the one place where practising the discipline costs nothing.
///
/// **9** adds [`domowik_plan_hit_test`] and its debug probe. Adding exports is
/// backward-compatible for a caller that never looks them up, and the number
/// still moves: a caller that resolves symbols eagerly fails at load rather
/// than at the first click, which is the failure worth having.
///
/// **10** is NOT backward-compatible, unlike every bump before it that only
/// added exports or fields: [`domowik_plan_edit`]'s SUCCESS response changes
/// shape, from the bare `batch_id` bytes it always wrote to the same JSON
/// object [`domowik_plan_execute`]/`undo`/`redo` return (so a warning a
/// gesture's own batch attaches is visible without a second call through
/// [`domowik_plan_state`]). A caller parsing the old bytes as a UUID would
/// misparse a JSON object; the version must move so that caller fails at
/// `domowik_abi_version()` rather than on its first parsed gesture. Also
/// adds the [`domowik_debug_warnings_computations`] debug export.
///
/// **11** adds [`domowik_plan_marquee`] (issue 0059): a caller that never
/// looks it up is unaffected, so this bump is additive like **9**'s, not
/// breaking like **10**'s — but the number still moves, for the same reason
/// **9**'s did.
///
/// **12** adds [`domowik_plan_snap`] and its debug build counter. It is
/// additive, but eager symbol resolution should still fail at load time for a
/// caller that expects the snapping boundary and receives an older library.
///
/// **13** adds room records (issue 0060): three new [`domowik_plan_edit`]
/// gesture kinds — `addRoom`, `setRoom`, `deleteRoom` — and four new fields on
/// [`domowik_plan_derived`], `recordId`, `label` and `displayAreaM2` on each
/// room plus a `roomRecords` array beside them. It adds NO export, so a caller
/// that resolves symbols eagerly still loads. The number moves anyway, and for
/// a sharper reason than **9**'s: a caller on **12** that sends
/// `{"kind":"addRoom"}` gets a parse rejection rather than a missing symbol,
/// and one that reads `rooms[i]` without `label` silently draws an unnamed
/// plan. Checking the version is how either is caught at startup instead of
/// being mistaken for a document with no room records in it.
///
/// **14** adds an optional `lengthMm` to the `setWall` gesture (issue 0071),
/// resolved bridge-side about the stored Start. The shape is additive, but the
/// failure it prevents is not a missing symbol: an ABI 13 library deserializes
/// the form, ignores the field it does not know, applies the thickness/height
/// half, and leaves the length unchanged — a silently partial edit. The
/// number moves so that pairing is refused at [`domowik_abi_version`]
/// instead of being discovered by re-reading geometry.
/// **15** adds plan-local directory libraries and the `plan_assets` JSON call.
///
/// **16** adds the browser editor's byte boundary (issue 0088):
/// [`domowik_plan_open_bytes`], [`domowik_plan_encode`],
/// [`domowik_plan_mark_saved`], [`domowik_plan_import_model`] and
/// [`domowik_plan_scene_frame`]. Additive, and the number moves for **9**'s
/// reason; the `wasm32-wasip1` build also exports `domowik_web_alloc` and
/// `domowik_web_free`.
/// **17** adds `setWalls`: one atomic height/thickness edit of same-level walls.
/// Older libraries reject the new gesture; version checking catches that at load.
const ABI_VERSION: i32 = 17;

/// An owned byte buffer handed to the caller.
///
/// `data` is freed by [`domowik_buffer_free`] and by nothing else. `len` is
/// the exact byte count; the allocation's capacity equals it, because every
/// buffer is shrunk to fit before it is released.
#[repr(C)]
pub struct DomowikBuf {
    pub data: *mut u8,
    pub len: u64,
    /// Opaque release token. Pass THIS to [`domowik_buffer_free`], not `data`.
    ///
    /// # Why the pointer is not the key
    ///
    /// The first version keyed the outstanding-buffer registry by the POINTER,
    /// which is what the Go spike did. Two addresses are not two buffers:
    ///
    /// - **Demonstrated:** every EMPTY buffer has the same address. Rust gives
    ///   a zero-length allocation a dangling well-aligned pointer — `0x1` for
    ///   `u8` — so two outstanding empty buffers (two `domowik_last_error`
    ///   calls on handles with no error) collided in the map. The second
    ///   insert overwrote the first, `domowik_outstanding_buffers` reported 1
    ///   for 2, and freeing the second returned `UNKNOWN_BUFFER` for a buffer
    ///   this library had legitimately issued.
    /// - **Theoretical, NOT reproduced here:** after a buffer is freed its
    ///   address may be handed out again. A stale-pointer free would then find
    ///   the recycled address in the map and free a LIVE buffer. Sixty-four
    ///   attempts on this host's allocator never reused the address, so this
    ///   is recorded as a hazard the design removes rather than a bug
    ///   demonstrated.
    ///
    /// A monotonic token is never reused, so a stale or forged token is always
    /// detected and no two buffers can ever share a key. It is the same
    /// argument that makes handles registry tokens instead of pointers.
    pub token: u64,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

struct Plan {
    scene_models: std::collections::BTreeMap<String, std::sync::Arc<domowik_models::ModelMesh>>,
    scene_model_decodes: u64,
    scene_model_errors: std::collections::BTreeMap<String, String>,
    resolutions: Option<(
        ContentRevision,
        Vec<domowik_assets::resolve::InstanceResolution>,
    )>,
    /// Embedded asset content changes that no batch records: an uploaded
    /// model can supply bytes a placed instance was missing, which changes
    /// its resolution, footprint and 3D mesh without moving
    /// [`document_revision`]. See [`Plan::content_revision`].
    asset_revision: u64,
    metadata_dirty: bool,
    /// Accepted manifest-metadata changes, which never enter the log. With
    /// [`document_revision`] it forms the [`save_revision`] a browser save
    /// confirms against.
    metadata_changes: u64,
    asset_hits: Option<asset_library::FootprintIndex>,
    /// View-only pose, valid only for the captured document revision. Never
    /// serialized, saved, or applied to the authoritative session.
    asset_pose_preview: Option<(usize, domowik_format::AssetInstance)>,
    /// Plan-session-local furniture clipboard (issue 0113): captured asset
    /// instances and the anchor the next paste places at the pointer. View
    /// state, like `asset_pose_preview`: never persisted, never in the
    /// operation log, never restored by undo, and discarded when the plan
    /// handle is closed.
    asset_clipboard: Option<asset_library::AssetClipboard>,
    /// Geometry-free baseline for transform-only presenter updates.
    view_scene: Option<asset_library::CachedScene>,
    scene_builds: u64,
    pose_scene_updates: u64,
    /// The file-backed plan, once this plan has one.
    ///
    /// A plan created with `domowik_plan_create` starts in memory; opening or
    /// saving gives it a container. The SESSION lives inside the container
    /// plan once there is one, so there is never a second copy of the
    /// document to keep in step.
    file: Option<ContainerPlan>,
    /// The session for a plan that has no file yet.
    session: Session,
    /// Sticky last error (see the module docs). Not cleared by retrieval.
    last_error: String,
    /// Derived query state per level. See [`LevelIndexes`].
    levels: HashMap<Uuid, LevelIndexes>,
    /// How many times this plan has REFRESHED a hit index (built or synced
    /// to a new revision), for [`domowik_debug_hit_index_builds`].
    hit_index_builds: u64,
    snap_index_builds: u64,
    /// Apertures built (not reused from a level's cache) by derived reads.
    aperture_builds: u64,
    /// One read-only warning projection per accepted document revision.
    /// Save/refusal/state reads retain it; it never enters persisted history.
    warnings: Option<(usize, Vec<WarningResponse>)>,
    /// How many times this plan's warning cache has actually RECOMPUTED
    /// (a revision miss), for [`domowik_debug_warnings_computations`].
    /// Mirrors `hit_index_builds`'s role: proof that a mutation and the
    /// state read immediately following it share one computation per
    /// accepted revision, not two.
    warnings_computations: u64,
}

/// What the asset-dependent caches are keyed on: the document revision and
/// the plan's embedded-content revision.
type ContentRevision = (usize, u64);

impl Plan {
    fn display_asset<'a>(
        &'a self,
        asset: &'a domowik_format::AssetInstance,
    ) -> &'a domowik_format::AssetInstance {
        match &self.asset_pose_preview {
            Some((revision, preview))
                if *revision == document_revision(self.session()) && preview.id == asset.id =>
            {
                preview
            }
            _ => asset,
        }
    }
    /// The key for caches that depend on asset resolution as well as on the
    /// document: hit placeholders, footprints and resolutions.
    fn content_revision(&self) -> ContentRevision {
        (document_revision(self.session()), self.asset_revision)
    }

    /// `level`'s hit index, synced to the current document revision and its
    /// placeholder snapshot. `Err` when placeholder resolution fails.
    fn synced_hit(&mut self, level: Uuid) -> Result<&HitIndex, String> {
        let revision = self.content_revision();
        let fresh = self
            .levels
            .get(&level)
            .is_some_and(|l| l.hit_revision == Some(revision));
        if !fresh {
            let placeholders = self.placeholders(level)?;
            let session = match &self.file {
                Some(f) => f.session_ref(),
                None => &self.session,
            };
            let entry = self
                .levels
                .entry(level)
                .or_insert_with(|| LevelIndexes::new(level));
            entry.hit.sync(session.document(), &placeholders);
            entry.hit_revision = Some(revision);
            self.hit_index_builds = self.hit_index_builds.saturating_add(1);
        }
        Ok(&self.levels[&level].hit)
    }

    /// Resolved asset footprints on `level` as marquee candidates
    /// (issue 0111): the same conservative bounds, revision key and
    /// degradation exclusion as the asset store's point hits. Degraded
    /// instances stay out — they remain `placeholder-box` candidates inside
    /// the geometry marquee, never both, so no instance is double-counted.
    fn resolved_footprints(&mut self, level: Uuid) -> Vec<AssetFootprint> {
        asset_library::ensure_footprint_index(self, level);
        self.asset_hits.as_ref().expect("just ensured").footprints()
    }

    /// `level`'s snapping index (and plan geometry), synced to the current
    /// document revision.
    fn synced_snap(&mut self, level: Uuid) -> &SnapIndex {
        let session = match &self.file {
            Some(f) => f.session_ref(),
            None => &self.session,
        };
        let revision = document_revision(session);
        let entry = self
            .levels
            .entry(level)
            .or_insert_with(|| LevelIndexes::new(level));
        if entry.snap_revision != Some(revision) {
            entry.snap.sync(session.document());
            entry.snap_revision = Some(revision);
            self.snap_index_builds = self.snap_index_builds.saturating_add(1);
        }
        &entry.snap
    }

    /// `level`'s drawn region with opening cuts (R4.2.2), synced to the
    /// current revision; `None` when the level hosts no opening, in which
    /// case the drawn region IS the unamended occupied region and no second
    /// engine is kept (removing the last opening frees it). The engine is
    /// occupied-only and built on first demand.
    fn synced_drawn(&mut self, level: Uuid) -> Option<Vec<Polygon>> {
        let session = match &self.file {
            Some(f) => f.session_ref(),
            None => &self.session,
        };
        let revision = document_revision(session);
        let doc = session.document();
        let entry = self
            .levels
            .entry(level)
            .or_insert_with(|| LevelIndexes::new(level));
        if entry.drawn_revision == Some(revision) {
            return entry.drawn.as_ref().map(|g| g.occupied());
        }
        entry.drawn_revision = Some(revision);
        if !domowik_geometry::cuts::level_has_openings(doc, level) {
            entry.drawn = None;
            return None;
        }
        let solids = walls::level_solids(doc, level);
        let apertures = domowik_geometry::cuts::level_apertures(doc, &solids);
        if apertures.is_empty() {
            entry.drawn = None;
            return None;
        }
        let engine = entry.drawn.get_or_insert_with(LevelGeometry::occupied_only);
        engine.sync_cut(&solids, &apertures);
        Some(engine.occupied())
    }

    fn ensure_file(&mut self) -> &mut ContainerPlan {
        if self.file.is_none() {
            let session = std::mem::replace(&mut self.session, Session::new());
            self.file = Some(ContainerPlan::unsaved(session));
        }
        self.file.as_mut().expect("just ensured")
    }
    /// The session, wherever it currently lives.
    fn session(&self) -> &Session {
        match &self.file {
            Some(f) => f.session_ref(),
            None => &self.session,
        }
    }

    /// Resolution is local and revision-cached: no network or external cache is
    /// configured by this editor. Decoding is checked once per digest per revision.
    fn resolutions(&mut self) -> &[domowik_assets::resolve::InstanceResolution] {
        use domowik_assets::{
            descriptor::DescriptorTable,
            policy::RetrievalPolicy,
            resolve::{self, DegradedReason, InstanceRef, Lookup, MediaSupport, ResolutionState},
        };
        let revision = self.content_revision();
        if !matches!(&self.resolutions, Some((r, _)) if *r == revision) {
            let empty = Default::default();
            let descriptors = self
                .file
                .as_ref()
                .map(|f| f.descriptors())
                .unwrap_or(&empty);
            let (table, _) = DescriptorTable::from_object(descriptors);
            let empty_assets = Default::default();
            let embedded = self
                .file
                .as_ref()
                .map(|f| &f.assets)
                .unwrap_or(&empty_assets);
            let instances: Vec<_> = self
                .session()
                .document()
                .asset_instances
                .iter()
                .map(|a| InstanceRef {
                    instance_id: a.id,
                    level_id: a.level_id,
                    digest: a.digest.clone(),
                })
                .collect();
            let media = MediaSupport::new(["model/gltf-binary", "model/obj", "application/zip"]);
            let policy = RetrievalPolicy::default();
            let snapshot = resolve::resolve(
                &instances,
                &table.as_vec(),
                &Lookup {
                    embedded,
                    cache: None,
                    retrieval: &policy,
                    media: &media,
                },
            );
            let mut decoded = HashMap::new();
            let entries = snapshot
                .iter()
                .cloned()
                .map(|mut entry| {
                    if !entry.state.is_degraded() {
                        if let Some(bytes) = embedded.get(&entry.digest) {
                            let usable =
                                *decoded.entry(entry.digest.clone()).or_insert_with(|| {
                                    domowik_models::mesh_with_budget(bytes, 5000).is_ok()
                                });
                            if !usable {
                                entry.state =
                                    ResolutionState::Degraded(DegradedReason::Undecodable);
                            }
                        }
                    }
                    entry
                })
                .collect();
            self.resolutions = Some((revision, entries));
        }
        &self.resolutions.as_ref().expect("just ensured").1
    }

    fn placeholders(&mut self, level: Uuid) -> Result<Vec<Placeholder>, String> {
        let entries: Vec<_> = self
            .resolutions()
            .iter()
            .filter(|e| e.level_id == level && e.state.is_degraded())
            .map(|e| (e.instance_id, e.declared_extent))
            .collect();
        entries
            .into_iter()
            .map(|(id, extent)| {
                let instance = self
                    .session()
                    .document()
                    .asset_instance(id)
                    .ok_or("Unknown placeholder instance")?;
                let declared = extent.map(|e| (e.width_mm, e.height_mm, e.depth_mm));
                let bounds = match placeholder_geometry(instance, declared)
                    .ok_or("Placeholder bounds exceed the supported coordinate range")?
                {
                    PlaceholderGeometry::Fallback(b) | PlaceholderGeometry::Declared(b) => b,
                };
                Ok(Placeholder {
                    instance: id,
                    bounds,
                })
            })
            .collect()
    }

    /// The session, mutably, or the refusal that stops the edit.
    ///
    /// BEHAVIOR-SPEC §9.6 blocks editing while a required asset is unresolved,
    /// and the core enforces it on this accessor. It is surfaced across the
    /// ABI as `REJECTED` rather than unwrapped: a panic here would unwind into
    /// a foreign frame, which is the one thing the C harness exists to prove
    /// does not happen, and a caller that asked to edit a read-only plan has
    /// made an ordinary refusable mistake rather than an ABI violation.
    ///
    /// This editor also checks per-instance decodability: an embedded but
    /// unusable required model blocks mutation and normal saving.
    fn session_mut(&mut self) -> Result<&mut Session, domowik_container::ContainerError> {
        let blocking: Vec<_> = self
            .resolutions()
            .iter()
            .filter(|e| e.state.is_degraded() && e.requirement.blocks_editing())
            .map(|e| e.digest.clone())
            .collect();
        if !blocking.is_empty() {
            return Err(domowik_container::ContainerError::ReadOnlyDegraded {
                path: self
                    .file
                    .as_ref()
                    .and_then(|f| f.path())
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<unsaved>".into()),
                assets: blocking.join("; "),
            });
        }
        match &mut self.file {
            Some(f) => f.session(),
            None => Ok(&mut self.session),
        }
    }
}

/// The derived query state of one level, kept across edits (issues 0012,
/// 0013).
///
/// # Why kept, and why keyed this way
///
/// A drag asks for a hit test and a snap on every pointer move, and every
/// accepted edit used to throw both indexes away: the snap index re-ran a
/// whole-level overlay (2.1 ms at 840 walls), so the first pointer move after
/// each edit paid for the level. Both indexes now SYNC instead — an identity
/// diff plus work proportional to what changed — and the snap index owns the
/// level's incremental plan geometry, which `domowik_plan_derived` reads too,
/// so one engine serves all three.
///
/// Each index is synced lazily, on the first query after the ACTIVE
/// session's operation-log length changed. Every mutation this ABI can
/// perform — `execute`, `edit`, `undo`, `redo` — appends exactly one batch to
/// that log on success and changes nothing on failure, so a stale index
/// cannot survive an edit and a refused edit costs nothing. Saving appends
/// nothing (`Plan::save` writes the package and clears dirty), and no read
/// appends anything, so the indexes are RETAINED across both.
///
/// "ACTIVE session" is not a formality. A plan created in memory keeps its
/// session in [`Plan::session`]; saving moves that session into a container,
/// and opening starts with one already there. Reading the log from the wrong
/// one would key every saved plan's cache off a permanently-zero length — the
/// same live-session confusion issue 0042 fixed in the summary counts.
///
/// Kept per level rather than for the active level only: switching back to
/// a level at the same revision costs nothing, and the plan geometry has to
/// exist per displayed level anyway.
struct LevelIndexes {
    /// The level's snapping index; also the owner of its plan geometry.
    snap: SnapIndex,
    /// The revision `snap` was last synced to.
    snap_revision: Option<usize>,
    hit: HitIndex,
    /// The revision `hit` and its placeholder snapshot were last synced to.
    hit_revision: Option<ContentRevision>,
    /// Each hosted opening's derived aperture, keyed by opening id and valid
    /// while the opening and its host's geometry are unchanged.
    apertures: HashMap<Uuid, ApertureEntry>,
    /// The drawn region's occupied-only engine (R4.2.2), present only while
    /// the level hosts an opening, and the revision it was synced to.
    drawn: Option<LevelGeometry>,
    drawn_revision: Option<usize>,
}

/// One cached aperture: the inputs it was built from and its JSON.
struct ApertureEntry {
    opening: domowik_format::Opening,
    host: (domowik_format::Point, domowik_format::Point, f64),
    json: Option<serde_json::Value>,
}

impl LevelIndexes {
    fn new(level: Uuid) -> Self {
        Self {
            snap: SnapIndex::new(level),
            snap_revision: None,
            hit: HitIndex::new(level),
            hit_revision: None,
            apertures: HashMap::new(),
            drawn: None,
            drawn_revision: None,
        }
    }
}

/// A registered plan: its state, and the two things that must be reachable
/// WITHOUT its lock.
struct Entry {
    plan: Arc<Mutex<Plan>>,
    /// Cancellation EPOCH, bumped by [`domowik_cancel`].
    ///
    /// Deliberately OUTSIDE the plan's mutex. The operation being cancelled
    /// holds that mutex, so a flag stored inside it could only be set after
    /// the work finished — which is not a cancel, it is a wait.
    ///
    /// An epoch rather than a bool, because a bool loses cancellations. The
    /// long operation used to clear the flag on entry; a `cancel` landing
    /// between the operation's lookup and its clear was overwritten, and the
    /// caller got `OK` from `domowik_cancel` for a cancellation that never
    /// happened. Nothing ever clears an epoch: the operation records the value
    /// it started at and stops as soon as it differs, so a cancel arriving at
    /// any point during the operation is observed, and one arriving BEFORE it
    /// still does not leak into it. Found by adversarial review (Codex,
    /// gpt-5.6-sol, 2026-09-01).
    cancel_epoch: Arc<AtomicU64>,
    /// In-flight ABI calls against this plan, for [`domowik_plan_close`].
    gate: Arc<Gate>,
}

/// Tracks calls that are inside the ABI on one plan's behalf.
///
/// # Why the plan's mutex is not enough
///
/// `domowik_plan_close` used to take the plan lock briefly and call that
/// quiescence. It is not, and the header promised something the code did not
/// deliver — found by adversarial review (Codex, gpt-5.6-sol, 2026-09-01).
/// Three windows escaped it:
///
/// 1. A call resolves its handle and parses its request BEFORE taking the plan
///    lock. Close saw a free mutex and returned while that thread was about to
///    enter the core.
/// 2. A call drops the plan lock before serialising its response, so close
///    could return while the other thread was still inside the library.
/// 3. [`domowik_busy`] never takes the plan lock at all, so close never waited
///    for it.
///
/// A LEASE is taken when a call resolves its handle and released when that
/// call returns, which brackets the whole export rather than the part that
/// touches the session. Close removes the registry entry — so no new lease can
/// be acquired, because no later call can find the plan — then waits for the
/// outstanding count to reach zero.
///
/// What "brackets the whole export" means precisely (issue 0063): the lease
/// is dropped as the export's own guarded closure returns, which is after
/// that call has published its output — but a handful of the export
/// function's own epilogue instructions, and the FFI return sequence back
/// into the caller's native code, still run AFTER the drop. `close` waiting
/// for every lease to reach zero is therefore a guarantee about admitted
/// WORK and its output, not a guarantee that every calling thread has
/// finished executing library text — see [`domowik_plan_close`]'s doc for
/// what that distinction means for a caller planning to unload the library.
#[derive(Default)]
struct Gate {
    active: Mutex<usize>,
    idle: Condvar,
}

impl Gate {
    #[cfg_attr(not(test), allow(unused_variables))]
    fn enter(self: &Arc<Self>, handle: u64) -> Lease {
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        *active += 1;
        drop(active);
        Lease {
            gate: Arc::clone(self),
            #[cfg(test)]
            handle,
        }
    }

    /// Blocks until every lease has been released.
    #[cfg_attr(not(test), allow(unused_variables))]
    fn wait_for_quiescence(&self, handle: u64) {
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        while *active > 0 {
            // Fires WHILE HOLDING `active`, so this must be notify-only and
            // never block: `Lease::drop` needs this same mutex to decrement
            // the count, and a blocking hook here would deadlock with no
            // timeout to break it (issue 0063, peer-reviewed). Once this
            // point is reached with `*active > 0`, the loop has no way out
            // except `idle.wait` — it cannot return until some `Lease::drop`
            // brings the count to zero — which is what makes this signal
            // proof that `close` is genuinely blocked, not merely that its
            // thread has not been scheduled yet.
            #[cfg(test)]
            test_hooks::on_quiescence_wait(handle);
            active = self.idle.wait(active).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// Held for the duration of one ABI call. Dropping it is what makes
/// [`domowik_plan_close`] able to return.
struct Lease {
    gate: Arc<Gate>,
    /// Test-only (issue 0063): lets `Drop` offer a `PreLeaseDrop` hold point
    /// for the exact handle this lease belongs to. Absent in non-test
    /// builds, where dropping a lease does only what it always did.
    #[cfg(test)]
    handle: u64,
}

impl Drop for Lease {
    fn drop(&mut self) {
        // The LAST thing that happens before this lease is released: every
        // bit of this call's own work, including publishing its output, is
        // already done. A hold armed here proves `close` waits for that —
        // not merely for the call to have been admitted. See `test_hooks`.
        #[cfg(test)]
        test_hooks::hold_at(self.handle, test_hooks::Phase::PreLeaseDrop);
        let mut active = self.gate.active.lock().unwrap_or_else(|e| e.into_inner());
        *active -= 1;
        if *active == 0 {
            self.gate.idle.notify_all();
        }
    }
}

struct Registry {
    plans: HashMap<u64, Entry>,
    /// Token -> (pointer, length) for every buffer issued and not yet freed.
    /// See [`DomowikBuf::token`] for why the key is not the pointer.
    buffers: HashMap<u64, (usize, usize)>,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| {
    Mutex::new(Registry {
        plans: HashMap::new(),
        buffers: HashMap::new(),
    })
});

/// Handles are never reused: a closed handle stays unknown for the life of the
/// process, so a stale handle can never be mistaken for a live one.
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// Buffer release tokens, never reused, for the same reason.
static NEXT_BUFFER_TOKEN: AtomicU64 = AtomicU64::new(1);

/// The next id from `counter`, or `None` once it is exhausted.
///
/// `fetch_add` WRAPS, so "never reused" was not literally implemented: after
/// 2^64 issues the counter returns to values still in the map, and a stale
/// token would then free a live buffer or address someone else's document.
/// At a billion per second that is 584 years, so the severity is low and the
/// review said so — but the header states the guarantee absolutely, and a
/// guarantee that holds "almost always" is the kind that gets relied on.
/// This saturates instead: once exhausted the counter stays exhausted and
/// every further request is refused, which is a dead library rather than a
/// silently unsound one. Found by adversarial review (Codex, gpt-5.6-sol,
/// 2026-09-01).
fn next_id(counter: &AtomicU64) -> Option<u64> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
        .ok()
}

/// Takes the registry lock, recovering from poisoning.
///
/// Every entry point contains its panics, so a poisoned lock means a panic
/// happened while the lock was held — the registry may be missing an update,
/// but it is not torn: every mutation under it is a single map operation.
/// Bricking the library for the rest of the process is a worse outcome than
/// continuing.
fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}

/// Test-only instrumentation for the quiescence seam (issue 0063).
///
/// The tests that prove [`domowik_plan_close`] waits for an admitted call
/// used to guess at that call's timing with a fixed `sleep`, then sample a
/// flag the worker thread set AFTER its export call returned — but that flag
/// store is ordered after the lease drop only in the WORKER's own program
/// order, not relative to the thread that observes it, so `close` could
/// legitimately return (correctly: the lease really was gone) before the
/// worker got around to setting the flag. Neither problem was about
/// `close`'s actual behaviour; both were about how the tests watched it.
///
/// This gives a test a way to be notified the instant a lease is actually
/// taken (replacing the guessed sleep) and to hold that lease open on
/// demand (so "close is still waiting" can be checked directly instead of
/// inferred from a race). It compiles to nothing outside `cfg(test)` and
/// touches no production path.
#[cfg(test)]
mod test_hooks {
    use std::collections::HashMap;
    use std::sync::mpsc::{Receiver, Sender};
    use std::sync::Mutex;

    /// Which point in an ABI call's lifetime a hold pauses at.
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    pub(super) enum Phase {
        /// Lease taken, still outside the plan lock — window 1 of the
        /// original finding: proves `close` waits for a call that has not
        /// touched the session yet.
        Admitted,
        /// Immediately before the lease actually drops: every bit of the
        /// call's own work, including publishing its output, is already
        /// done — window 2 of the original finding: proves `close` waits
        /// for output already produced, not just for admission.
        PreLeaseDrop,
    }

    struct Hold {
        admitted: Sender<()>,
        release: Receiver<()>,
    }

    static HOLDS: Mutex<Option<HashMap<(u64, Phase), Hold>>> = Mutex::new(None);
    static REGISTRY_REMOVED: Mutex<Option<HashMap<u64, Sender<()>>>> = Mutex::new(None);

    /// A one-shot release for a hold armed by [`arm`]. Its `Drop` ALSO
    /// removes the `(handle, phase)` entry from `HOLDS` if it is still
    /// sitting there unconsumed — an arm whose target call never happens
    /// (a test assertion fails first, or the call gets refused before it
    /// reaches the hold) must not leak an entry for the rest of the
    /// process. That remove is a harmless no-op once `hold_at` has already
    /// taken the entry; the `send` that follows is a harmless no-op if
    /// nothing is parked to receive it.
    pub(super) struct ReleaseGuard {
        handle: u64,
        phase: Phase,
        release: Option<Sender<()>>,
    }

    impl ReleaseGuard {
        pub(super) fn release(mut self) {
            self.fire();
        }

        fn fire(&mut self) {
            let mut holds = HOLDS.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(map) = holds.as_mut() {
                map.remove(&(self.handle, self.phase));
            }
            drop(holds);
            if let Some(tx) = self.release.take() {
                let _ = tx.send(());
            }
        }
    }

    impl Drop for ReleaseGuard {
        fn drop(&mut self) {
            self.fire();
        }
    }

    /// Arms a one-shot hold for the NEXT call that reaches `phase` on
    /// `handle`. Returns the receiver that fires the moment it does, and a
    /// guard that releases it to proceed.
    ///
    /// This captures the next matching call from ANY thread, not
    /// specifically a worker the test is about to spawn — if the test's own
    /// code touches `(handle, phase)` between arming and spawning, IT gets
    /// parked instead. That is an ordering precondition for the caller to
    /// respect, not something this type enforces. Handles are never reused
    /// (see `NEXT_HANDLE`), so a handle-keyed global cannot collide across
    /// tests even though `HOLDS` itself is process-wide.
    pub(super) fn arm(handle: u64, phase: Phase) -> (Receiver<()>, ReleaseGuard) {
        let (admitted_tx, admitted_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut holds = HOLDS.lock().unwrap_or_else(|e| e.into_inner());
        holds.get_or_insert_with(HashMap::new).insert(
            (handle, phase),
            Hold {
                admitted: admitted_tx,
                release: release_rx,
            },
        );
        drop(holds);
        (
            admitted_rx,
            ReleaseGuard {
                handle,
                phase,
                release: Some(release_tx),
            },
        )
    }

    /// Called from the real seam for `phase` (see call sites). A no-op
    /// unless a hold is armed for this exact `(handle, phase)`; otherwise it
    /// signals arrival and parks — still holding whatever the caller was
    /// holding — until the test releases it.
    ///
    /// Bounded, not `recv()`: the test that owns this hold may itself be
    /// blocked (for instance inside `domowik_plan_close`, waiting on the
    /// very lease this call is holding), in which case it can never reach
    /// the call that would release us — the sender stays alive on a stuck
    /// stack frame, so a plain `recv()` would hang forever with no per-test
    /// timeout to save it. Timing out here does not decide the test either
    /// way; it just stops holding the lease, so whatever the test asserted
    /// (or failed to assert before its own deadlock) is what fails, not
    /// this plumbing.
    pub(super) fn hold_at(handle: u64, phase: Phase) {
        let hold = {
            let mut holds = HOLDS.lock().unwrap_or_else(|e| e.into_inner());
            holds.as_mut().and_then(|map| map.remove(&(handle, phase)))
        };
        if let Some(hold) = hold {
            let _ = hold.admitted.send(());
            let _ = hold
                .release
                .recv_timeout(std::time::Duration::from_secs(30));
        }
    }

    /// Arms a one-shot notification for the moment [`domowik_plan_close`]
    /// removes `handle` from the registry — visible before it waits for
    /// quiescence, so a test can probe the "removed but not yet quiescent"
    /// window (the entry is gone, but an admitted call may still be draining).
    pub(super) fn watch_registry_removal(handle: u64) -> Receiver<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watchers = REGISTRY_REMOVED.lock().unwrap_or_else(|e| e.into_inner());
        watchers.get_or_insert_with(HashMap::new).insert(handle, tx);
        rx
    }

    /// Called from `domowik_plan_close` right after it removes the registry
    /// entry. A no-op unless a watcher is armed for this exact handle.
    pub(super) fn on_registry_removed(handle: u64) {
        let watcher = {
            let mut watchers = REGISTRY_REMOVED.lock().unwrap_or_else(|e| e.into_inner());
            watchers.as_mut().and_then(|map| map.remove(&handle))
        };
        if let Some(tx) = watcher {
            let _ = tx.send(());
        }
    }

    static QUIESCENCE_WAIT: Mutex<Option<HashMap<u64, Sender<()>>>> = Mutex::new(None);

    /// Arms a one-shot notification for the moment `Gate::wait_for_quiescence`
    /// observes an outstanding lease on `handle` and is about to actually
    /// block on it. Unlike [`watch_registry_removal`], receiving this is
    /// proof `close` is genuinely parked, not merely that it has started
    /// running: once the seam that fires it is reached with the count still
    /// above zero, the loop cannot return except via `Lease::drop` bringing
    /// that count to zero — there is no path out in between. A mutant
    /// `close` that skips the wait entirely never reaches this seam at all.
    pub(super) fn watch_quiescence_wait(handle: u64) -> Receiver<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watchers = QUIESCENCE_WAIT.lock().unwrap_or_else(|e| e.into_inner());
        watchers.get_or_insert_with(HashMap::new).insert(handle, tx);
        rx
    }

    /// Called from `Gate::wait_for_quiescence` WHILE HOLDING `Gate::active`.
    /// Must stay notify-only and non-blocking: `Lease::drop` needs that same
    /// mutex to decrement the count, so anything here that blocked would
    /// deadlock the very thing it is trying to observe, with no timeout to
    /// break it. Plain `mpsc::Sender::send` on an unbounded channel — never
    /// a `SyncSender` or any bounded channel — is what keeps this safe.
    pub(super) fn on_quiescence_wait(handle: u64) {
        let watcher = {
            let mut watchers = QUIESCENCE_WAIT.lock().unwrap_or_else(|e| e.into_inner());
            watchers.as_mut().and_then(|map| map.remove(&handle))
        };
        if let Some(tx) = watcher {
            let _ = tx.send(());
        }
    }
}

/// Resolves `handle` and takes a call lease on it, atomically under the
/// registry lock.
///
/// Every export that names a handle goes through this, and holds the returned
/// [`Lease`] until it returns. That is what makes
/// [`domowik_plan_close`]'s quiescence promise true: the lease is taken
/// BEFORE the request is parsed and released AFTER the response is written,
/// so it brackets the whole call rather than the part that touches the
/// session.
fn resolve(handle: u64) -> Option<(Arc<Mutex<Plan>>, Lease)> {
    let (plan, lease) = {
        let registry = registry();
        let entry = registry.plans.get(&handle)?;
        let lease = entry.gate.enter(handle);
        (entry.plan.clone(), lease)
    };
    #[cfg(test)]
    test_hooks::hold_at(handle, test_hooks::Phase::Admitted);
    Some((plan, lease))
}

/// Resolves `handle` for a call that needs only the cancellation epoch.
fn resolve_epoch(handle: u64) -> Option<(Arc<AtomicU64>, Lease)> {
    let (epoch, lease) = {
        let registry = registry();
        let entry = registry.plans.get(&handle)?;
        let lease = entry.gate.enter(handle);
        (entry.cancel_epoch.clone(), lease)
    };
    #[cfg(test)]
    test_hooks::hold_at(handle, test_hooks::Phase::Admitted);
    Some((epoch, lease))
}

/// Runs `body` with panics contained, mapping one to [`status::PANIC`].
///
/// A panic unwinding across `extern "C"` aborts the process. This is the only
/// thing standing between a bug in the core and the user's editor vanishing,
/// so every exported function goes through it — including the ones that
/// "cannot" panic.
fn guard(body: impl FnOnce() -> i32) -> i32 {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(code) => code,
        Err(_) => status::PANIC,
    }
}

/// Records `message` as `handle`'s sticky last error and returns `code`.
fn fail(plan: &Arc<Mutex<Plan>>, code: i32, message: impl Into<String>) -> i32 {
    let mut plan = plan.lock().unwrap_or_else(|e| e.into_inner());
    plan.last_error = message.into();
    code
}

/// Moves `bytes` into a caller-owned buffer, recording it as outstanding.
fn release(bytes: Vec<u8>, out: *mut DomowikBuf) -> i32 {
    // `into_boxed_slice` shrinks capacity to length, so the free path needs
    // only the pointer and the length it already records.
    let boxed = bytes.into_boxed_slice();
    let len = boxed.len();
    let ptr = Box::into_raw(boxed) as *mut u8;
    let Some(token) = next_id(&NEXT_BUFFER_TOKEN) else {
        // Reclaim rather than leak the allocation we just made.
        // SAFETY: `ptr` came from `Box::into_raw` on a boxed slice of `len`.
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
        }
        return status::EXHAUSTED;
    };
    registry().buffers.insert(token, (ptr as usize, len));
    // SAFETY: `out` is checked non-null by every caller before this runs.
    unsafe {
        *out = DomowikBuf {
            data: ptr,
            len: len as u64,
            token,
        };
    }
    status::OK
}

// ---------------------------------------------------------------------------
// Wire protocol
// ---------------------------------------------------------------------------

/// A request to [`domowik_plan_execute`].
///
/// JSON rather than a struct-per-call C surface: the operations are already
/// `serde` types with a normative serialized form (FORMAT-SPEC §4.2), so a
/// second hand-maintained encoding of the same thing is a second place for the
/// two to disagree. The cost is measured — see the spike README's batching
/// numbers.
#[derive(Deserialize)]
struct ExecuteRequest {
    operations: Vec<Operation>,
}

#[derive(Serialize)]
struct ExecuteResponse {
    batch_id: String,
    /// Entity count after the command, so a caller can tell a no-op from an
    /// edit without a second round trip.
    entities: usize,
    undo_depth: usize,
    redo_depth: usize,
    dirty: bool,
}

/// [`domowik_plan_state`]'s response: the shared summary plus the revision.
///
/// A separate type rather than more fields on [`ExecuteResponse`], so the
/// execute/undo/redo payloads keep their exact current shape. This is an
/// ADDITIVE revision and warning fields on one call, not an ABI change.
#[derive(Serialize)]
struct StateResponse<'a> {
    #[serde(flatten)]
    summary: ExecuteResponse,
    /// The ACTIVE session's accepted-batch count. See [`document_revision`].
    document_revision: usize,
    /// ABI 16: grows when embedded asset content changes without a batch (an
    /// uploaded model supplying missing bytes). Footprints, hit candidates
    /// and the 3D scene can change with it; the document cannot.
    asset_revision: u64,
    warnings: &'a [WarningResponse],
}

#[derive(Serialize)]
struct WarningResponse {
    /// Either a §2.1 level advisory or an R3.1.2 `wall-area-overlap` pair —
    /// [`domowik_document::warnings::Warning`] is untagged, so this field's
    /// JSON shape is unchanged from when it only ever held a
    /// [`domowik_format::ValidationWarning`]: a `{code, ...}` object.
    #[serde(flatten)]
    warning: domowik_document::warnings::Warning,
    message: String,
}

/// [`domowik_plan_execute`]/`edit`/`undo`/`redo`'s response: the existing
/// summary plus the warnings attached to the batch that just committed.
///
/// A wrapper, not new fields on [`ExecuteResponse`], so this object's OWN
/// shape and field names never change across the callers that share it.
/// What each ENTRY POINT does with that shape is not uniform, and ABI_VERSION
/// **10** exists because of the difference: [`domowik_plan_execute`], `undo`
/// and `redo` already returned exactly [`ExecuteResponse`]'s fields as JSON,
/// so gaining `warnings` here is additive for them. [`domowik_plan_edit`]
/// previously wrote only the bare `batch_id` bytes — no JSON at all — so its
/// response adopting this shape is a BREAKING change for that one entry
/// point, not an additive one; see `ABI_VERSION`'s doc for why the version
/// had to move for it. BEHAVIOR-SPEC R3.1.7: a warning attaches to the batch
/// result itself, so a caller must not need a second round trip through
/// [`domowik_plan_state`] to see one.
///
/// `warnings` BORROWS [`Plan::warnings`]'s revision-keyed cache rather than
/// recomputing its own copy: a mutation ends its own revision, and the
/// caller's very next read is overwhelmingly [`domowik_plan_state`] asking
/// about that SAME revision (a GUI refresh after every edit), so computing it
/// twice for one accepted batch would be avoidable duplicate work — confirmed
/// by [`domowik_debug_warnings_computations`] in this crate's own tests, not
/// a timed measurement.
#[derive(Serialize)]
struct MutationResponse<'a> {
    #[serde(flatten)]
    summary: ExecuteResponse,
    warnings: &'a [WarningResponse],
}

/// Ensures `plan.warnings` holds the CURRENT accepted revision's warnings,
/// recomputing only when the cached revision has fallen behind, and returns
/// that revision. The one place either [`mutation_response`] or
/// [`domowik_plan_state`] may write [`Plan::warnings`], so a mutating call
/// and the state read that immediately follows it never both pay to compute
/// the same accepted revision's warnings.
fn ensure_warnings_cached(plan: &mut Plan) -> usize {
    let revision = document_revision(plan.session());
    if plan.warnings.as_ref().map(|(rev, _)| *rev) != Some(revision) {
        let warnings = domowik_document::warnings::validation_warnings(plan.session().document())
            .into_iter()
            .map(|warning| {
                let message = warning.to_string();
                WarningResponse { warning, message }
            })
            .collect();
        plan.warnings = Some((revision, warnings));
        plan.warnings_computations = plan.warnings_computations.saturating_add(1);
    }
    revision
}

fn mutation_response(plan: &mut Plan, batch_id: String) -> MutationResponse<'_> {
    ensure_warnings_cached(plan);
    let mut summary = response_for(plan.session(), batch_id);
    summary.dirty |= plan.metadata_dirty;
    let warnings = &plan.warnings.as_ref().expect("warning cache populated").1;
    MutationResponse { summary, warnings }
}

/// A monotonic counter of ACCEPTED DOCUMENT CHANGES on the active session.
///
/// # What it is, and what it deliberately is not
///
/// It is the number of batches in the active session's operation log. Every
/// accepted `execute`, `edit`, `undo` and `redo` appends exactly one — an undo
/// appends a COMPENSATING batch rather than removing one, so history moves
/// forward in both directions and the number only ever grows. A rejected
/// command appends nothing and a read appends nothing, so neither moves it.
/// Saving appends nothing either: `Plan::save` writes the package and clears
/// dirty without touching the log.
///
/// It is NOT the undo depth, which falls when you undo and cannot distinguish
/// "undone" from "never done". It is NOT the dirty flag, which is a boolean
/// and is cleared by saving. R7.2.3 resets the click cycle on "any accepted
/// document change", and neither of those two answers that question: undo
/// depth is equal before and after an undo-then-redo pair that changed the
/// document twice, and dirty is false immediately after a save that changed
/// nothing about the candidates.
///
/// This is the same quantity [`LevelIndexes`] uses for its DOCUMENT half, and
/// deliberately so: a caller resetting on this number covers document
/// invalidation exactly, with no second definition to drift.
///
/// It covers only that half. Indexes are kept per level, keyed on the
/// revision each was last synced to, so querying a level for the first time
/// builds its index with the revision unchanged, and this number says
/// nothing about it. That is not a disagreement — the
/// level is one of R7.2.3's own reset triggers, tracked by the caller
/// alongside the view transform, modifiers and pointer kind. Do not read a
/// stable revision as "the index was reused".
///
/// # Comparable only within one handle
///
/// The baseline belongs to the session, so a NEW handle starts from its own.
/// `domowik_plan_create` and `domowik_plan_create_default` begin at 0;
/// `domowik_plan_open` begins at the file's retained batch count, because
/// opening CONTINUES the log rather than starting a new stream. Comparing the
/// number across two handles is meaningless, and callers must not — R7.4
/// clears the selection on reopen anyway, so there is nothing to preserve
/// across that boundary.
fn document_revision(session: &Session) -> usize {
    session.log().batches().len()
}

fn response_for(session: &Session, batch_id: String) -> ExecuteResponse {
    ExecuteResponse {
        batch_id,
        entities: session.document().entity_count(),
        undo_depth: session.undo_depth(),
        redo_depth: session.redo_depth(),
        dirty: session.is_dirty(),
    }
}

/// Borrows `len` bytes at `ptr` for the duration of the call.
///
/// # Safety
///
/// The caller guarantees `ptr` addresses at least `len` readable bytes that
/// stay valid and unmutated until this function returns. That is the ABI's
/// borrow rule, and it is the caller's half of it.
unsafe fn borrow<'a>(ptr: *const u8, len: u64) -> Option<&'a [u8]> {
    if ptr.is_null() {
        // Rejected regardless of `len`. `slice::from_raw_parts` requires a
        // non-null, aligned pointer even for a zero length, so a null pointer
        // is never turned into an empty slice here.
        return None;
    }
    let len = usize::try_from(len).ok()?;
    Some(std::slice::from_raw_parts(ptr, len))
}

// ---------------------------------------------------------------------------
// Exports
// ---------------------------------------------------------------------------

/// The ABI revision this library implements.
#[no_mangle]
pub extern "C" fn domowik_abi_version() -> i32 {
    ABI_VERSION
}

/// Creates an empty internal builder and writes its handle to `out_handle`.
/// Add a level before saving; user-facing New uses [`domowik_plan_create_default`].
///
/// # Safety
///
/// `out_handle` must be a valid, writable `uint64_t*`.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_create(out_handle: *mut u64) -> i32 {
    guard(|| register_session(Session::new(), out_handle))
}

/// Creates an editor plan with a default ground level as its clean baseline.
/// No operation or undo entry is emitted for this initial document state.
///
/// # Safety
/// `out_handle` must be a writable `uint64_t*`; no write occurs on failure.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_create_default(out_handle: *mut u64) -> i32 {
    guard(|| {
        if out_handle.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let baseline = Document::with_ground_floor();
        match Session::open_snapshot(Uuid::now_v7(), baseline, &Default::default(), &[], vec![]) {
            Ok(session) => register_session(session, out_handle),
            Err(_) => status::REJECTED,
        }
    })
}

unsafe fn register_session(session: Session, out_handle: *mut u64) -> i32 {
    register_plan(None, session, out_handle)
}

/// Registers a plan, file-backed or not, and writes its handle.
unsafe fn register_plan(
    file: Option<ContainerPlan>,
    session: Session,
    out_handle: *mut u64,
) -> i32 {
    if out_handle.is_null() {
        return status::INVALID_ARGUMENT;
    }
    let Some(handle) = next_id(&NEXT_HANDLE) else {
        return status::EXHAUSTED;
    };
    let entry = Entry {
        plan: Arc::new(Mutex::new(Plan {
            resolutions: None,
            asset_revision: 0,
            metadata_dirty: false,
            metadata_changes: 0,
            scene_models: Default::default(),
            scene_model_decodes: 0,
            scene_model_errors: Default::default(),
            asset_hits: None,
            asset_pose_preview: None,
            asset_clipboard: None,
            view_scene: None,
            scene_builds: 0,
            pose_scene_updates: 0,
            file,
            session,
            last_error: String::new(),
            levels: HashMap::new(),
            warnings: None,
            warnings_computations: 0,
            hit_index_builds: 0,
            snap_index_builds: 0,
            aperture_builds: 0,
        })),
        cancel_epoch: Arc::new(AtomicU64::new(0)),
        gate: Arc::new(Gate::default()),
    };
    registry().plans.insert(handle, entry);
    *out_handle = handle;
    status::OK
}

/// Executes a JSON `{"operations":[...]}` request as one atomic command.
///
/// On success `out` receives a JSON response the caller must free with
/// [`domowik_buffer_free`]. On failure `out` is left untouched and the reason
/// is retrievable with [`domowik_last_error`].
///
/// # Safety
///
/// `req` must address `req_len` readable bytes for the duration of the call,
/// and `out` must be a valid, writable `domowik_buf*`.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_execute(
    handle: u64,
    req: *const u8,
    req_len: u64,
    out: *mut DomowikBuf,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        // The lease is taken BEFORE the request is parsed, so `close` cannot
        // return while this call is still on its way into the core.
        let Some((plan, _lease)) = resolve(handle) else {
            // No last error is recorded: there is no handle to record it on,
            // and inventing one would let a forged handle allocate.
            return status::UNKNOWN_HANDLE;
        };
        let Some(bytes) = borrow(req, req_len) else {
            return fail(&plan, status::INVALID_ARGUMENT, "request pointer is null");
        };

        let request: ExecuteRequest = match serde_json::from_slice(bytes) {
            Ok(r) => r,
            Err(e) => {
                // §4.4 at raw ingress: a created opening whose enum names no
                // variant is a refusal WITH its code, chosen by R4.4 across
                // every failure of the batch; anything else stays malformed.
                return match raw_opening_refusal(&plan, bytes) {
                    Some(message) => fail(&plan, status::REJECTED, message),
                    None => fail(&plan, status::MALFORMED_REQUEST, e.to_string()),
                };
            }
        };

        let mut plan = plan.lock().unwrap_or_else(|e| e.into_inner());
        // The §9.6 gate, reported through the last-error string like any other
        // refusal. `fail` locks the plan itself, so it cannot be used while
        // this guard is held; the error is recorded on the guard directly.
        let committed = match plan.session_mut() {
            Ok(session) => session.commit(request.operations),
            Err(e) => {
                plan.last_error = e.to_string();
                return status::REJECTED;
            }
        };
        match committed {
            Ok(batch) => {
                let body = mutation_response(&mut plan, batch.to_string());
                match serde_json::to_vec(&body) {
                    Ok(bytes) => {
                        drop(plan);
                        release(bytes, out)
                    }
                    Err(e) => {
                        plan.last_error = e.to_string();
                        status::REJECTED
                    }
                }
            }
            Err(e) => {
                plan.last_error = format!("{e}");
                status::REJECTED
            }
        }
    })
}

/// Undoes the most recent command. `out` receives the same JSON shape as
/// [`domowik_plan_execute`], with an empty `batch_id` when there was nothing to
/// undo.
///
/// # Safety
///
/// `out` must be a valid, writable `domowik_buf*`.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_undo(handle: u64, out: *mut DomowikBuf) -> i32 {
    change_history(handle, out, false)
}

/// Redoes the most recently undone command through the core's restoration
/// rules. Returns the same response as undo, including an empty `batch_id`
/// when the stack is empty. Saves do not invalidate the redo stack.
///
/// # Safety
/// `out` must be writable; success transfers one caller-owned buffer.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_redo(handle: u64, out: *mut DomowikBuf) -> i32 {
    change_history(handle, out, true)
}

unsafe fn change_history(handle: u64, out: *mut DomowikBuf, redo: bool) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let mut plan = plan.lock().unwrap_or_else(|e| e.into_inner());
        let result = match plan.session_mut() {
            Ok(session) => {
                if redo {
                    session.redo()
                } else {
                    session.undo()
                }
            }
            Err(e) => {
                plan.last_error = e.to_string();
                return status::REJECTED;
            }
        };
        match result {
            Ok(entry) => {
                let id = entry.map(|b| b.to_string()).unwrap_or_default();
                let body = mutation_response(&mut plan, id);
                match serde_json::to_vec(&body) {
                    Ok(bytes) => {
                        drop(plan);
                        release(bytes, out)
                    }
                    Err(e) => {
                        plan.last_error = e.to_string();
                        status::REJECTED
                    }
                }
            }
            Err(e) => {
                plan.last_error = format!("{e}");
                status::REJECTED
            }
        }
    })
}

/// Reads entity/history counts, dirty state and the document revision without
/// changing the session. All of them come from the active session, including
/// after save/open.
///
/// Carries revision and read-only warnings absent from execute/undo/redo:
/// `document_revision`, the active session's accepted-batch count. Reads and
/// saves leave it alone; every accepted `execute`, `edit`, `undo` and `redo`
/// advances it, including edits nowhere near whatever the caller last hit
/// tested — which is what R7.2.3's "any accepted document change" requires and
/// what an entity-id comparison cannot see. See [`document_revision`] for why
/// it is neither the undo depth nor the dirty flag, and for the rule that it
/// is comparable only within one handle.
/// `warnings` contains stable diagnostic codes, affected level UUIDs and a
/// human message. Warnings never make a valid command fail. Computation is
/// cached by revision; repeated reads only serialize the retained diagnostics.
///
/// # Safety
/// `out` must be writable. Success transfers one caller-owned JSON buffer;
/// errors leave it untouched. Calls serialise on this handle and are covered
/// by the same close/drain lease as edits.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_state(handle: u64, out: *mut DomowikBuf) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let mut plan = plan.lock().unwrap_or_else(|e| e.into_inner());
        let revision = ensure_warnings_cached(&mut plan);
        let body = StateResponse {
            summary: {
                let mut summary = response_for(plan.session(), String::new());
                summary.dirty |= plan.metadata_dirty;
                summary
            },
            document_revision: revision,
            asset_revision: plan.asset_revision,
            warnings: &plan.warnings.as_ref().expect("warning cache populated").1,
        };
        match serde_json::to_vec(&body) {
            Ok(bytes) => {
                drop(plan);
                release(bytes, out)
            }
            Err(error) => {
                plan.last_error = error.to_string();
                status::REJECTED
            }
        }
    })
}

/// Writes the plan's current document as JSON to `out`.
///
/// # Safety
///
/// `out` must be a valid, writable `domowik_buf*`.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_snapshot(handle: u64, out: *mut DomowikBuf) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let plan = plan.lock().unwrap_or_else(|e| e.into_inner());
        match serde_json::to_vec(plan.session().document()) {
            Ok(bytes) => {
                drop(plan);
                release(bytes, out)
            }
            Err(_) => status::REJECTED,
        }
    })
}

/// Performs one EDITING GESTURE, with every effect the specification attaches
/// to it, and writes the same [`MutationResponse`] JSON shape as
/// [`domowik_plan_execute`]/`undo`/`redo` to `out` — including any warning
/// the resulting batch attaches (BEHAVIOR-SPEC R3.1.7). A properties-form
/// gesture submitted unchanged commits nothing and writes an EMPTY buffer,
/// not this shape: there is no batch, and so nothing to report.
///
/// ABI_VERSION **10**: this response shape is a BREAKING change from what
/// this entry point wrote before — the bare `batch_id` bytes, no JSON — not
/// an additive one. A caller built against ABI 9 must not parse this call's
/// output; that is exactly what checking `domowik_abi_version()` before use
/// is for.
///
/// # Why this is not just `domowik_plan_execute`
///
/// `execute` takes operations the caller already decided on. A gesture is not
/// a list of operations — it is an intention whose consequences are the
/// specification's to determine. Drawing a wall across another wall SPLITS it
/// (R3.1.2), the split lands in the SAME batch (R3.1.1), the openings on the
/// wall it crossed are remapped or the whole edit is refused (R4.3.3), and any
/// endpoint landing on an occupied node joins it (R3.3.5). A caller building
/// operations by hand gets none of that, and the document it produces is
/// quietly different from the one the CLI would produce for the same gesture.
///
/// The GUI did exactly that until this call existed.
///
/// # Requests
///
/// ```json
/// {"kind":"addWall","levelId":"…","start":[x,y],"end":[x,y],
///  "thicknessMm":200,"heightMm":2500}
/// {"kind":"moveWall","wallId":"…","start":[x,y],"end":[x,y]}   // either may be null
/// {"kind":"translateSelection","entityIds":["…","…"],"delta":[dx,dy]}
/// {"kind":"setWall","wallId":"…","thicknessMm":150}
/// {"kind":"setWall","wallId":"…","lengthMm":3000,"heightMm":2700}
///                                    // lengthMm keeps start, moves end
/// {"kind":"setWalls","wallIds":["…","…"],"heightMm":2700,"thicknessMm":200}
/// {"kind":"deleteWall","wallId":"…"}
/// {"kind":"reverseWall","wallId":"…"}
/// {"kind":"orderLevels","levelIds":["…","…"]}     // top of the list first
/// {"kind":"moveOpening","openingId":"…","u":0.6}
/// {"kind":"setOpening","openingId":"…","widthMm":800,"heightMm":2000}
/// {"kind":"rehostOpening","openingId":"…","hostWallId":"…","newId":"…","u":0.6}
/// {"kind":"deleteOpening","openingId":"…"}
/// {"kind":"addRoom","levelId":"…","seed":[x,y],"name":"Salon"}   // id optional
/// {"kind":"setRoom","roomId":"…","name":"Kuchnia"}               // fields optional
/// {"kind":"deleteRoom","roomId":"…"}
/// ```
///
/// # Safety
///
/// `req` must point to `req_len` readable bytes; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_edit(
    handle: u64,
    req: *const u8,
    req_len: u64,
    out: *mut DomowikBuf,
) -> i32 {
    guard(|| {
        if out.is_null() || (req.is_null() && req_len != 0) {
            return status::INVALID_ARGUMENT;
        }
        let bytes = if req_len == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(req, req_len as usize) }
        };
        // §4.4 before typed decoding: an opening request writing a field it
        // may not (`hostWallId`, `levelId`) or naming no enum variant is a
        // refusal with R4.4's primary code — serde would otherwise drop the
        // forbidden fields silently or call the request malformed.
        if let Some(outcome) = opening_request_refusal(handle, bytes) {
            return match outcome {
                RequestRefusal::Malformed => status::MALFORMED_REQUEST,
                RequestRefusal::UnknownHandle => status::UNKNOWN_HANDLE,
                RequestRefusal::Refused(message) => {
                    if let Some((plan, _lease)) = resolve(handle) {
                        plan.lock().unwrap_or_else(|e| e.into_inner()).last_error = message;
                    }
                    status::REJECTED
                }
            };
        }
        let request: EditRequest = match serde_json::from_slice(bytes) {
            Ok(r) => r,
            Err(_) => return status::MALFORMED_REQUEST,
        };
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let mut plan = plan.lock().unwrap_or_else(|e| e.into_inner());

        if let EditRequest::TranslateSelection { entity_ids, .. } = &request {
            if plan
                .resolutions()
                .iter()
                .any(|entry| entity_ids.contains(&entry.instance_id) && entry.state.is_degraded())
            {
                plan.last_error = "group movement does not support degraded placeholders".into();
                return status::REJECTED;
            }
        }
        let built = build_edit(plan.session().document(), &request);
        let operations = match built {
            Ok(ops) => ops,
            Err(message) => {
                plan.last_error = message;
                return status::REJECTED;
            }
        };
        if operations.is_empty()
            && matches!(
                request,
                EditRequest::SetLevel { .. }
                    | EditRequest::SetWalls { .. }
                    | EditRequest::SetOpening { .. }
                    // Issue 0115: a reorder that finds the list already in
                    // this order commits nothing either.
                    | EditRequest::OrderLevels { .. }
            )
        {
            // A properties form submitted unchanged is not a document command.
            drop(plan);
            return release(Vec::new(), out);
        }
        let committed = match plan.session_mut() {
            Ok(session) => session.commit(operations),
            Err(e) => {
                plan.last_error = e.to_string();
                return status::REJECTED;
            }
        };
        match committed {
            Ok(batch) => {
                let body = mutation_response(&mut plan, batch.to_string());
                match serde_json::to_vec(&body) {
                    Ok(bytes) => {
                        drop(plan);
                        release(bytes, out)
                    }
                    Err(e) => {
                        plan.last_error = e.to_string();
                        status::REJECTED
                    }
                }
            }
            Err(e) => {
                let detail = e.to_string();
                let detail = detail.trim_start_matches("command rejected: ");
                plan.last_error = format!("command rejected: {detail}");
                status::REJECTED
            }
        }
    })
}

/// The refusal message of an execute request that failed typed decoding
/// only because created openings carry invalid enum values: the enums are
/// replaced by placeholders, the batch is evaluated on a scratch copy of
/// the session, and R4.4's precedence picks the primary code across the
/// enum codes and whatever the scratch commit refuses. `None` when the
/// request is malformed for any other reason.
fn raw_opening_refusal(plan: &Arc<Mutex<Plan>>, bytes: &[u8]) -> Option<String> {
    let mut value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let codes = domowik_document::ingress::sanitize_opening_enums(value.get_mut("operations")?);
    if codes.is_empty() {
        return None;
    }
    let request: ExecuteRequest = serde_json::from_value(value).ok()?;
    let mut scratch = plan
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .session()
        .clone();
    let trial = scratch.commit(request.operations).err();
    let primary = codes
        .iter()
        .map(|(c, _)| *c)
        .chain(trial.as_ref().and_then(|e| e.primary_opening_code()))
        .min()?;
    Some(format!("command rejected: {primary}"))
}

/// What [`opening_request_refusal`] decided.
enum RequestRefusal {
    Malformed,
    UnknownHandle,
    Refused(String),
}

/// R4.4 for an `addOpening` / `setOpening` request, before typed decoding:
/// `None` when it writes no forbidden field and names no invalid enum (the
/// ordinary path handles it). Otherwise the offending fields are removed or
/// replaced by placeholders, the request must still decode (else it is
/// malformed: framing failures stay framing), and its edit is evaluated on
/// a scratch copy of the session so R4.4's precedence picks the primary
/// code across the request's own failures and the candidate state's.
fn opening_request_refusal(handle: u64, bytes: &[u8]) -> Option<RequestRefusal> {
    use domowik_format::OpeningCode as C;
    let mut value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let kind = value.get("kind")?.as_str()?.to_owned();
    if kind != "addOpening" && kind != "setOpening" {
        return None;
    }
    let object = value.as_object_mut()?;
    let mut codes = Vec::new();
    if kind == "setOpening" && object.remove("hostWallId").is_some() {
        codes.push(C::OpeningDirectRehost);
    }
    if object.remove("levelId").is_some() {
        codes.push(C::OpeningLevelImmutable);
    }
    let mut check = |key: &str, code: C, valid: &[&str], placeholder: Option<&str>| {
        if let Some(v) = object.get(key) {
            if !v.is_null() && !v.as_str().is_some_and(|s| valid.contains(&s)) {
                codes.push(code);
                match placeholder {
                    Some(p) => {
                        object.insert(key.to_owned(), serde_json::json!(p));
                    }
                    None => {
                        object.remove(key);
                    }
                }
            }
        }
    };
    if kind == "addOpening" {
        check(
            "opening",
            C::InvalidOpeningKind,
            &["door", "window"],
            Some("door"),
        );
    }
    check("hinge", C::InvalidHinge, &["nearStart", "nearEnd"], None);
    check(
        "opensToward",
        C::InvalidOpensToward,
        &["Left", "Right"],
        None,
    );
    if codes.is_empty() {
        return None;
    }
    let Ok(request) = serde_json::from_value::<EditRequest>(value) else {
        return Some(RequestRefusal::Malformed);
    };
    let Some((plan, _lease)) = resolve(handle) else {
        return Some(RequestRefusal::UnknownHandle);
    };
    let plan = plan.lock().unwrap_or_else(|e| e.into_inner());
    let doc = plan.session().document();
    let trial = match build_edit(doc, &request) {
        Ok(ops) => {
            let mut scratch = plan.session().clone();
            scratch
                .commit(ops)
                .err()
                .and_then(|e| e.primary_opening_code())
        }
        // The builder names a missing host before any operation exists.
        Err(message) if message.starts_with("no wall") => Some(C::UnknownHost),
        Err(_) => None,
    };
    let primary = codes.into_iter().chain(trial).min()?;
    Some(RequestRefusal::Refused(format!(
        "command rejected: {primary}"
    )))
}

/// One editing gesture, as the caller describes it.
///
/// Every variant ends in `Wall` and clippy would rather they did not. These
/// names ARE the wire format — `{"kind":"addWall"}` is what a caller sends —
/// so shortening them either breaks the contract or hides it behind serde
/// renames, which puts the thing a reader most needs to see one indirection
/// away. Openings and levels will land here as `addOpening`, `setLevel` and
/// so on, at which point the postfix stops being uniform anyway.
#[derive(Debug, Deserialize)]
#[allow(clippy::enum_variant_names)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum EditRequest {
    #[serde(rename_all = "camelCase")]
    TranslateSelection {
        entity_ids: Vec<Uuid>,
        delta: [f64; 2],
    },
    #[serde(rename_all = "camelCase")]
    AddWall {
        level_id: Uuid,
        start: [f64; 2],
        end: [f64; 2],
        thickness_mm: f64,
        #[serde(default)]
        height_mm: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    MoveWall {
        wall_id: Uuid,
        #[serde(default)]
        start: Option<[f64; 2]>,
        #[serde(default)]
        end: Option<[f64; 2]>,
    },
    #[serde(rename_all = "camelCase")]
    SetWall {
        wall_id: Uuid,
        /// Keeps the stored Start fixed and moves End along the existing
        /// Start→End direction to this centreline length. Resolved here, on
        /// the Rust side of the boundary, so the view never derives geometry
        /// (ED-0101). Mutually exclusive with the coordinate gestures by
        /// construction: this gesture has no start/end fields.
        #[serde(default)]
        length_mm: Option<f64>,
        #[serde(default)]
        thickness_mm: Option<f64>,
        #[serde(default)]
        height_mm: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    SetWalls {
        wall_ids: Vec<Uuid>,
        #[serde(default)]
        thickness_mm: Option<f64>,
        #[serde(default)]
        height_mm: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    DeleteWall { wall_id: Uuid },
    #[serde(rename_all = "camelCase")]
    DeleteLevel { level_id: Uuid },
    #[serde(rename_all = "camelCase")]
    SetLevel {
        level_id: Uuid,
        name: Option<String>,
        elevation_mm: Option<f64>,
        floor_thickness_mm: Option<f64>,
        default_wall_height_mm: Option<f64>,
        default_ceiling_height_mm: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    ReverseWall { wall_id: Uuid },
    #[serde(rename_all = "camelCase")]
    OrderLevels {
        /// Every level in the requested list order, top of the list first
        /// (issue 0115). A whole-list write: the bridge refuses a partial
        /// list rather than synthesizing one.
        level_ids: Vec<Uuid>,
    },
    #[serde(rename_all = "camelCase")]
    AddOpening {
        host_wall_id: Uuid,
        /// `"door"` or `"window"`.
        opening: OpeningKind,
        u: f64,
        width_mm: f64,
        height_mm: f64,
        #[serde(default)]
        sill_mm: f64,
        #[serde(default)]
        mechanism: Option<domowik_format::OpeningMechanism>,
        #[serde(default)]
        hinge: Option<domowik_format::Hinge>,
        #[serde(default)]
        opens_toward: Option<domowik_format::Side>,
    },
    #[serde(rename_all = "camelCase")]
    MoveOpening { opening_id: Uuid, u: f64 },
    #[serde(rename_all = "camelCase")]
    RehostOpening {
        opening_id: Uuid,
        host_wall_id: Uuid,
        new_id: Uuid,
        u: f64,
    },
    #[serde(rename_all = "camelCase")]
    SetOpening {
        opening_id: Uuid,
        #[serde(default)]
        u: Option<f64>,
        #[serde(default)]
        width_mm: Option<f64>,
        #[serde(default)]
        height_mm: Option<f64>,
        #[serde(default)]
        sill_mm: Option<f64>,
        #[serde(default)]
        depth_mm: Option<f64>,
        #[serde(default)]
        opens_toward: Option<domowik_format::Side>,
        #[serde(default)]
        hinge: Option<domowik_format::Hinge>,
        #[serde(default)]
        mechanism: Option<domowik_format::OpeningMechanism>,
    },
    #[serde(rename_all = "camelCase")]
    DeleteOpening { opening_id: Uuid },

    // Room records (BEHAVIOR-SPEC R5.2.2). Exactly three kinds, because the
    // rule says exactly three; the editor gets no fourth way to touch a record.
    #[serde(rename_all = "camelCase")]
    AddRoom {
        level_id: Uuid,
        seed: [f64; 2],
        /// R5.2.2 forbids AUTOMATIC id assignment, so a caller that has an
        /// identifier in mind supplies it and a collision rejects. A caller
        /// that has none leaves this out and the bridge supplies one on its
        /// behalf — the CORE still mints nothing.
        #[serde(default)]
        id: Option<Uuid>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        floor_style_ref: Option<String>,
        #[serde(default)]
        ceiling_style_ref: Option<String>,
    },
    /// An absent field is left alone; an explicit `null` CLEARS an optional
    /// one. `Option<Option<T>>` is what tells those apart, and F37d is why
    /// they must be told apart: setting a name to `""` and clearing it are
    /// different edits reaching different states.
    #[serde(rename_all = "camelCase")]
    SetRoom {
        room_id: Uuid,
        #[serde(default)]
        seed: Option<[f64; 2]>,
        #[serde(default, deserialize_with = "some_option")]
        name: Option<Option<String>>,
        #[serde(default, deserialize_with = "some_option")]
        floor_style_ref: Option<Option<String>>,
        #[serde(default, deserialize_with = "some_option")]
        ceiling_style_ref: Option<Option<String>>,
        #[serde(default)]
        level_id: Option<Uuid>,
    },
    #[serde(rename_all = "camelCase")]
    DeleteRoom { room_id: Uuid },
}

/// Deserializes a present-but-null field as `Some(None)`.
///
/// `#[serde(default)]` on an `Option<Option<T>>` gives `None` for an ABSENT
/// field; serde's own impl would give `None` for an explicit `null` too, which
/// collapses "leave alone" and "clear" into one. This keeps them apart.
fn some_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Turns the wire form of an optional field into the core's [`FieldEdit`].
fn field_edit(value: &Option<Option<String>>) -> domowik_edit::FieldEdit<String> {
    match value {
        None => domowik_edit::FieldEdit::Keep,
        Some(None) => domowik_edit::FieldEdit::Clear,
        Some(Some(v)) => domowik_edit::FieldEdit::Set(v.clone()),
    }
}

/// Turns a gesture into the batch the specification says it commits.
fn build_edit(doc: &Document, request: &EditRequest) -> Result<Vec<Operation>, String> {
    match request {
        EditRequest::TranslateSelection { entity_ids, delta } => {
            domowik_edit::translate_selection_operations(
                doc,
                entity_ids,
                domowik_format::Point::new(delta[0], delta[1]),
            )
        }
        EditRequest::AddWall {
            level_id,
            start,
            end,
            thickness_mm,
            height_mm,
        } => {
            let wall = domowik_format::Wall::new(
                *level_id,
                domowik_format::Point::new(start[0], start[1]),
                domowik_format::Point::new(end[0], end[1]),
                *thickness_mm,
                domowik_edit::resolve_wall_height(doc, *level_id, *height_mm)
                    .map_err(|error| error.to_string())?,
            );
            domowik_edit::add_wall_operations(doc, *level_id, wall)
                .map(|e| e.operations)
                .map_err(|e| e.to_string())
        }
        EditRequest::MoveWall {
            wall_id,
            start,
            end,
        } => domowik_edit::set_wall_operations(
            doc,
            *wall_id,
            start.map(|p| domowik_format::Point::new(p[0], p[1])),
            end.map(|p| domowik_format::Point::new(p[0], p[1])),
            None,
            None,
        )
        .map(|edit| edit.operations)
        .map_err(|error| error.to_string()),
        EditRequest::SetWall {
            wall_id,
            length_mm,
            thickness_mm,
            height_mm,
        } => {
            // R7.1.6: a length edit keeps the stored Start and moves End along
            // the SAME direction. The arithmetic lives here, on the Rust side
            // of the boundary, so the view only chooses the intent.
            let end = match (*length_mm, doc.wall(*wall_id)) {
                (Some(length), Some(wall)) => {
                    let (sx, sy) = (wall.start_mm[0], wall.start_mm[1]);
                    let (ex, ey) = (wall.end_mm[0], wall.end_mm[1]);
                    let (dx, dy) = (ex - sx, ey - sy);
                    let current = dx.hypot(dy);
                    if !length.is_finite() || length <= 0.0 {
                        return Err("setWall lengthMm must be finite and positive".to_owned());
                    }
                    if !current.is_finite() || current <= 0.0 {
                        return Err("setWall lengthMm needs a nonzero stored axis".to_owned());
                    }
                    let scale = length / current;
                    Some(domowik_format::Point::new(sx + dx * scale, sy + dy * scale))
                }
                // An unknown wall has no axis to measure: fall through without
                // an end, and the resulting empty batch is refused downstream
                // exactly as it already was for an unknown `setWall` target.
                _ => None,
            };
            domowik_edit::set_wall_operations(doc, *wall_id, None, end, *thickness_mm, *height_mm)
                .map(|edit| edit.operations)
                .map_err(|error| error.to_string())
        }
        EditRequest::SetWalls {
            wall_ids,
            thickness_mm,
            height_mm,
        } => domowik_edit::set_walls_operations(doc, wall_ids, *thickness_mm, *height_mm),
        EditRequest::DeleteWall { wall_id } => {
            Ok(domowik_edit::remove_wall_operations(doc, *wall_id).operations)
        }
        EditRequest::DeleteLevel { level_id } => {
            if doc.level(*level_id).is_none() {
                return Err(format!("no level {level_id} to delete"));
            }
            // Session admission owns last/nonempty/referenced-level guards,
            // equally for this gesture and raw operations/compensations.
            Ok(vec![Operation::delete(*level_id)])
        }
        EditRequest::SetLevel {
            level_id,
            name,
            elevation_mm,
            floor_thickness_mm,
            default_wall_height_mm,
            default_ceiling_height_mm,
        } => domowik_edit::set_level_operations(
            doc,
            *level_id,
            &domowik_edit::LevelFields {
                name: name.clone(),
                elevation_mm: *elevation_mm,
                floor_thickness_mm: *floor_thickness_mm,
                default_wall_height_mm: *default_wall_height_mm,
                default_ceiling_height_mm: *default_ceiling_height_mm,
            },
        )
        .map_err(|error| error.to_string()),
        EditRequest::OrderLevels { level_ids } => {
            // Issue 0115: one gesture, one document-carried list order. An
            // empty result means the list already runs this way and nothing
            // is committed, exactly like a no-op setLevel.
            domowik_edit::reorder_levels_operations(doc, level_ids)
                .map_err(|error| error.to_string())
        }
        EditRequest::ReverseWall { wall_id } => {
            Ok(domowik_edit::reverse_wall_operations(doc, *wall_id).operations)
        }
        EditRequest::AddOpening {
            host_wall_id,
            opening,
            u,
            width_mm,
            height_mm,
            sill_mm,
            mechanism,
            hinge,
            opens_toward,
        } => {
            // The host's thickness IS the default depth: a door cuts the whole
            // wall, and R4.0.4 caps `depthMm` at the host's thickness, so a
            // fixed default would fail validation on thin walls.
            let host = doc
                .wall(*host_wall_id)
                .ok_or_else(|| format!("no wall {host_wall_id} to host the opening"))?;
            let mut o = match opening {
                OpeningKind::Door => domowik_format::Opening::door(
                    *host_wall_id,
                    *u,
                    *width_mm,
                    *height_mm,
                    host.thickness_mm,
                ),
                OpeningKind::Window => domowik_format::Opening::window(
                    *host_wall_id,
                    *u,
                    *width_mm,
                    *height_mm,
                    *sill_mm,
                    host.thickness_mm,
                ),
            };
            if let Some(value) = mechanism {
                if !value.accepts(*opening) {
                    return Err("mechanism incompatible with opening kind".to_owned());
                }
                o.mechanism = Some(*value);
            }
            if let Some(value) = hinge {
                o.hinge = *value;
            }
            if let Some(value) = opens_toward {
                o.opens_toward = *value;
            }
            Ok(vec![Operation::create(Entity::Opening(o))])
        }
        EditRequest::MoveOpening { opening_id, u } => {
            let mut fields = serde_json::Map::new();
            fields.insert("u".to_owned(), serde_json::json!(u));
            Ok(vec![Operation::set_fields(*opening_id, fields)])
        }
        EditRequest::RehostOpening {
            opening_id,
            host_wall_id,
            new_id,
            u,
        } => {
            let old = doc
                .openings
                .iter()
                .find(|o| o.id == *opening_id)
                .ok_or_else(|| format!("no opening {opening_id} to move"))?;
            let old_host = doc.wall(old.host_wall_id).ok_or("missing original host")?;
            let host = doc.wall(*host_wall_id).ok_or("missing target host")?;
            if old_host.level_id != host.level_id || old.host_wall_id == *host_wall_id {
                return Err("target host must be a different wall on the same level".to_owned());
            }
            if doc.entity(*new_id).is_some() || *new_id == *opening_id {
                return Err("new opening ID is already in use".to_owned());
            }
            let mut replacement = old.clone();
            replacement.id = *new_id;
            replacement.host_wall_id = *host_wall_id;
            replacement.u = *u;
            Ok(vec![
                Operation::delete(*opening_id),
                Operation::create(Entity::Opening(replacement)),
            ])
        }
        EditRequest::SetOpening {
            opening_id,
            u,
            width_mm,
            height_mm,
            sill_mm,
            depth_mm,
            opens_toward,
            hinge,
            mechanism,
        } => {
            let opening = doc
                .openings
                .iter()
                .find(|o| o.id == *opening_id)
                .ok_or_else(|| format!("no opening {opening_id} to edit"))?;
            let mut fields = serde_json::Map::new();
            if let Some(value) = mechanism {
                if !value.accepts(opening.kind) {
                    return Err("mechanism incompatible with opening kind".to_owned());
                }
                if opening.effective_mechanism() != *value {
                    fields.insert("mechanism".to_owned(), serde_json::json!(value));
                }
            }
            if let Some(value) = hinge.filter(|v| *v != opening.hinge) {
                fields.insert("hinge".to_owned(), serde_json::json!(value));
            }
            if let Some(value) = u.filter(|v| *v != opening.u) {
                fields.insert("u".to_owned(), serde_json::json!(value));
            }
            if let Some(value) = opens_toward.filter(|v| *v != opening.opens_toward) {
                fields.insert("opensToward".to_owned(), serde_json::json!(value));
            }
            if let Some(value) = width_mm.filter(|v| *v != opening.width_mm) {
                fields.insert("widthMm".to_owned(), serde_json::json!(value));
            }
            if let Some(value) = height_mm.filter(|v| *v != opening.height_mm) {
                fields.insert("heightMm".to_owned(), serde_json::json!(value));
            }
            if let Some(value) = sill_mm.filter(|v| *v != opening.sill_mm) {
                fields.insert("sillMm".to_owned(), serde_json::json!(value));
            }
            if let Some(value) = depth_mm.filter(|v| *v != opening.depth_mm) {
                fields.insert("depthMm".to_owned(), serde_json::json!(value));
            }
            Ok(if fields.is_empty() {
                vec![]
            } else {
                vec![Operation::set_fields(*opening_id, fields)]
            })
        }
        // R4.3.4 STANDALONE deletion: exactly that entity, nothing cascaded.
        EditRequest::DeleteOpening { opening_id } => Ok(vec![Operation::delete(*opening_id)]),

        // The three room commands go straight to the core builders. ED-0101:
        // the front end chooses what the user is trying to do and never
        // decides what it means — so candidacy, the lowest-UUID tie-break and
        // every status stay behind this boundary, and the editor holds no room
        // identity rules of its own.
        EditRequest::AddRoom {
            level_id,
            seed,
            id,
            name,
            floor_style_ref,
            ceiling_style_ref,
        } => {
            let mut record = domowik_format::RoomRecord::new(
                id.unwrap_or_else(Uuid::now_v7),
                *level_id,
                domowik_format::Point::new(seed[0], seed[1]),
            );
            record.name = name.clone();
            record.floor_style_ref = floor_style_ref.clone();
            record.ceiling_style_ref = ceiling_style_ref.clone();
            domowik_edit::create_room_operations(doc, record).map_err(|e| e.to_string())
        }
        EditRequest::SetRoom {
            room_id,
            seed,
            name,
            floor_style_ref,
            ceiling_style_ref,
            level_id,
        } => domowik_edit::update_room_operations(
            doc,
            *room_id,
            &domowik_edit::RoomFields {
                seed: seed.map(|p| domowik_format::Point::new(p[0], p[1])),
                name: field_edit(name),
                floor_style_ref: field_edit(floor_style_ref),
                ceiling_style_ref: field_edit(ceiling_style_ref),
                level_id: *level_id,
            },
        )
        .map_err(|e| e.to_string()),
        EditRequest::DeleteRoom { room_id } => {
            domowik_edit::delete_room_operations(doc, *room_id).map_err(|e| e.to_string())
        }
    }
}

/// Opens the package at `path` and registers it as a new plan.
///
/// The handle is written to `out_handle` on success and nothing is written on
/// failure. Opening CONTINUES the file's operation log rather than starting a
/// new stream, so a plan edited by the CLI and then by the GUI stays one
/// history.
/// A materialized file with no levels is refused without a handle; raw
/// container/CLI inspection remains available for explicit repair.
///
/// # Safety
///
/// `path` must be a NUL-terminated UTF-8 string; `out_handle` must be
/// writable.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_open(path: *const c_char, out_handle: *mut u64) -> i32 {
    guard(|| {
        if path.is_null() || out_handle.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Ok(text) = (unsafe { CStr::from_ptr(path) }).to_str() else {
            return status::INVALID_ARGUMENT;
        };
        let opened = match ContainerPlan::open(std::path::Path::new(text)) {
            Ok(p) => p,
            Err(_) => return status::REJECTED,
        };
        if opened.document().levels.is_empty() {
            // A legacy empty plan can still be inspected/repaired headlessly,
            // but cannot replace the editor's required active-level document.
            return status::REJECTED;
        }
        unsafe { register_plan(Some(opened), Session::new(), out_handle) }
    })
}

/// Writes the plan to `path`, adopting it as the plan's file.
///
/// # Safety
///
/// `path` must be a NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_save(handle: u64, path: *const c_char) -> i32 {
    guard(|| {
        if path.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Ok(text) = (unsafe { CStr::from_ptr(path) }).to_str() else {
            return status::INVALID_ARGUMENT;
        };
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let mut plan = plan.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = plan.session_mut() {
            plan.last_error = e.to_string();
            return status::REJECTED;
        }
        // A plan created in memory has no container behind it yet, so give it
        // one carrying the session it has been accumulating.
        if plan.file.is_none() {
            let session = std::mem::replace(&mut plan.session, Session::new());
            plan.file = Some(ContainerPlan::unsaved(session));
        }
        let file = plan.file.as_mut().expect("just ensured");
        match file.save_as(std::path::Path::new(text)) {
            Ok(()) => {
                plan.metadata_dirty = false;
                status::OK
            }
            Err(e) => {
                plan.last_error = e.to_string();
                status::REJECTED
            }
        }
    })
}

/// Opens a `.domo` package held in memory and registers it as a plan.
///
/// The byte-level twin of [`domowik_plan_open`] for hosts without a
/// filesystem (the browser editor, issue 0088): the same reader, checks and
/// refusals, and the same handle semantics. The plan has no path, so
/// [`domowik_plan_save`] needs a path before it can write; a browser saves
/// with [`domowik_plan_encode`] instead. Nothing is written on failure.
///
/// # Safety
///
/// `bytes` must address `len` readable bytes for the duration of the call;
/// `out_handle` must be writable.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_open_bytes(
    bytes: *const u8,
    len: u64,
    out_handle: *mut u64,
) -> i32 {
    guard(|| {
        if out_handle.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some(bytes) = (unsafe { borrow(bytes, len) }) else {
            return status::INVALID_ARGUMENT;
        };
        let opened = match ContainerPlan::open_bytes("plan.domo", bytes) {
            Ok(p) => p,
            Err(_) => return status::REJECTED,
        };
        if opened.document().levels.is_empty() {
            return status::REJECTED;
        }
        unsafe { register_plan(Some(opened), Session::new(), out_handle) }
    })
}

/// What a browser save confirms against: it grows with every accepted
/// document batch and every manifest-metadata change, so equal values mean
/// nothing a save writes has changed.
fn save_revision(plan: &Plan) -> u64 {
    document_revision(plan.session()) as u64 + plan.metadata_changes
}

/// Encodes the plan into the package bytes [`domowik_plan_save`] would write,
/// without writing them or changing any state.
///
/// Refuses exactly what saving refuses. On success `out` receives the bytes
/// (release with [`domowik_buffer_free`]) and `out_revision` the plan's save
/// revision; once the host has stored the bytes it reports so with
/// [`domowik_plan_mark_saved`] and that revision. Until then the plan stays
/// dirty, because a download the user cancels is not a save.
///
/// # Safety
///
/// `out_revision` must be a writable `uint64_t*` and `out` a writable
/// `domowik_buf*`; neither is written on failure.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_encode(
    handle: u64,
    out_revision: *mut u64,
    out: *mut DomowikBuf,
) -> i32 {
    guard(|| {
        if out_revision.is_null() || out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let mut plan = plan.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = plan.session_mut() {
            plan.last_error = e.to_string();
            return status::REJECTED;
        }
        let revision = save_revision(&plan);
        match plan.ensure_file().encode() {
            Ok(bytes) => {
                unsafe { *out_revision = revision };
                release(bytes, out)
            }
            Err(e) => {
                plan.last_error = e.to_string();
                status::REJECTED
            }
        }
    })
}

/// Records that the bytes [`domowik_plan_encode`] returned at `revision`
/// were stored, clearing dirty.
///
/// Refuses with `REJECTED`, changing nothing, when the plan has changed since
/// that encode: the stored bytes no longer describe it, so it stays dirty.
/// Undo and redo depths are unaffected either way.
#[no_mangle]
pub extern "C" fn domowik_plan_mark_saved(handle: u64, revision: u64) -> i32 {
    guard(|| {
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let mut plan = plan.lock().unwrap_or_else(|e| e.into_inner());
        if save_revision(&plan) != revision {
            plan.last_error = "the plan changed after it was encoded".to_owned();
            return status::REJECTED;
        }
        plan.ensure_file().mark_saved();
        plan.metadata_dirty = false;
        status::OK
    })
}

/// Writes the DERIVED geometry of one level to `out` as JSON.
///
/// # Why this exists
///
/// `domowik_plan_snapshot` returns the stored document: wall centre lines and
/// nothing else. An editor that drew only that would be drawing sticks. The
/// occupied region (R3.2.1) and the room faces with their areas (R5.1.1) are
/// COMPUTED, never stored, and computing them in the UI would put a second
/// geometry implementation in the view — which is exactly what the
/// architecture forbids, and what would drift from the core the first time a
/// join rule changed.
///
/// # Shape
///
/// ```json
/// {
///   "levelId": "…",
///   "occupied": [ { "outer": [[x,y],…], "holes": [[[x,y],…],…] }, … ],
///   "rooms":    [ { "outer": […], "holes": […], "areaMm2": 10.64e6,
///                   "recordId": "…"|null, "label": "Salon"|null,
///                   "displayAreaM2": "10.64",
///                   "labelAnchorMm": [x,y]|null, "seedCandidateMm": [x,y] }, … ],
///   "roomRecords": [ { "id": "…", "seedMm": [x,y], "name": "Salon"|null,
///                      "floorStyleRef": …, "ceilingStyleRef": …,
///                      "status": "associated" }, … ],
///   "floorAreaMm2": 10640000.0
/// }
/// ```
///
/// `rooms` are detected FACES and `roomRecords` are authoritative entities;
/// they are separate arrays because they are separate things. A face exists
/// with no record and shows no label (R5.3.3); a record persists with no face
/// and carries a non-associating status. `recordId` and `label` are non-null
/// exactly when the face has an effective record (R5.2.4), which is the one
/// question a renderer needs answered.
///
/// `displayAreaM2` is the R5.3.2 string — square metres, two decimals, exact
/// halves to even. It travels beside the raw `areaMm2` so that a front end
/// showing an area and the core computing one cannot disagree about 30.245;
/// rounding it in Dart would be a second implementation of a normative rule.
///
/// `labelAnchorMm` is where R5.3.3 puts the label: the effective record's
/// seed, which is strictly inside the face and so never needs clamping. It is
/// null exactly when `label` is.
///
/// `seedCandidateMm` is a point strictly inside the face that a caller can
/// hand to `addRoom`. It exists so that "name this room" needs no containment
/// test in the front end (ED-0101): an L-shaped room's centroid lies outside
/// it, and an editor picking its own point would seed a record into a wall.
///
/// Coordinates are millimetres. Both lists are in the R3.2.4/R5.1.3 canonical
/// order, so the same document always yields the same sequence and a caller
/// may diff two responses meaningfully.
///
/// `level_id` is a NUL-terminated UTF-8 UUID. An unknown level is not an
/// error: it yields empty lists, because a level with no walls has no derived
/// geometry and refusing would make the empty document a special case.
///
/// # Safety
///
/// `level_id` must point to a NUL-terminated string and `out` to a writable
/// `DomowikBuf`. The buffer is the CALLER's and must be released with
/// `domowik_buffer_free`.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_derived(
    handle: u64,
    level_id: *const c_char,
    out: *mut DomowikBuf,
) -> i32 {
    guard(|| {
        if out.is_null() || level_id.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Ok(text) = (unsafe { CStr::from_ptr(level_id) }).to_str() else {
            return status::INVALID_ARGUMENT;
        };
        let Ok(level) = Uuid::parse_str(text) else {
            return status::INVALID_ARGUMENT;
        };
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let mut plan = plan.lock().unwrap_or_else(|e| e.into_inner());
        plan.synced_snap(level);
        let drawn = plan.synced_drawn(level);
        let mut cache =
            std::mem::take(&mut plan.levels.get_mut(&level).expect("synced above").apertures);
        let geometry = plan.levels[&level].snap.geometry();
        let document = plan.session().document();
        // Seeds are located through the retained rings, not against every
        // polygon of the level (issue 0013).
        let projection = rooms::project_retained(document, level, geometry);
        // Apertures, in host-wall then document order, each reused from the
        // level's cache while its opening and host geometry are unchanged:
        // an edit elsewhere on the level rebuilds none of them (R4.3.7).
        // Openings are grouped by host in one pass — not filtered per wall,
        // which was O(walls × openings) per read.
        let mut hosted: HashMap<Uuid, Vec<&domowik_format::Opening>> = HashMap::new();
        for opening in &document.openings {
            hosted
                .entry(opening.host_wall_id)
                .or_default()
                .push(opening);
        }
        let mut apertures = Vec::new();
        let mut built = 0u64;
        let mut seen: HashMap<Uuid, ()> = HashMap::new();
        for wall in document.walls_on_level(level) {
            let host = (wall.start(), wall.end(), wall.thickness_mm);
            for &opening in hosted.get(&wall.id).into_iter().flatten() {
                seen.insert(opening.id, ());
                let fresh = cache
                    .get(&opening.id)
                    .is_some_and(|c| c.opening == *opening && c.host == host);
                if !fresh {
                    built += 1;
                    cache.insert(
                        opening.id,
                        ApertureEntry {
                            opening: opening.clone(),
                            host,
                            json: aperture_json(wall, opening),
                        },
                    );
                }
                if let Some(json) = &cache[&opening.id].json {
                    apertures.push(json.clone());
                }
            }
        }
        cache.retain(|id, _| seen.contains_key(id));
        // Room records are read HERE, under the lock, with the projection they
        // were classified against. Reading them afterwards would pair a record
        // with a status computed from a document it no longer matches.
        let records: Vec<(domowik_format::RoomRecord, &'static str)> = projection
            .records
            .iter()
            .filter_map(|p| Some((document.room_record(p.id)?.clone(), p.status.token())))
            .collect();
        let labels: Vec<Option<(Uuid, String, [f64; 2])>> = (0..projection.faces.len())
            .map(|i| {
                let id = projection.effective_record(i)?;
                let record = document.room_record(id)?;
                Some((id, record.display_name().to_owned(), record.seed_mm))
            })
            .collect();
        let (occupied, rooms) = (projection.occupied, projection.faces);
        plan.levels.get_mut(&level).expect("synced above").apertures = cache;
        plan.aperture_builds = plan.aperture_builds.saturating_add(built);
        drop(plan);

        let to_json = |p: &Polygon| {
            serde_json::json!({
                "outer": p.outer.iter().map(|v| [v.x, v.y]).collect::<Vec<_>>(),
                "holes": p.holes.iter()
                    .map(|h| h.iter().map(|v| [v.x, v.y]).collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
            })
        };
        // `fold` from an explicit +0.0, not `sum()`. Rust's `Sum for f64`
        // folds from NEGATIVE zero so that summing `[-0.0]` stays negative,
        // which means an empty room list yields `-0.0` — and a UI formatting
        // that to two places renders "-0.00 m²", which looks like a bug to
        // everyone who sees it.
        let floor_area: f64 = rooms.iter().map(|r| r.area_mm2()).fold(0.0, |a, b| a + b);
        // Apertures and door leaves, computed HERE for the same reason as the
        // rooms: an editor that reconstructed them would be a second
        // implementation of R4.2.1 and R4.0.4 living in the view.

        let payload = serde_json::json!({
            "levelId": level.to_string(),
            "openings": apertures,
            "occupied": occupied.iter().map(to_json).collect::<Vec<_>>(),
            // What to DRAW: the occupied region with opening cuts (R4.2.2).
            // `occupied` stays unamended (rooms, snapping and hits use it).
            "drawn": drawn.as_ref().unwrap_or(&occupied).iter().map(to_json).collect::<Vec<_>>(),
            "rooms": rooms.iter().enumerate().map(|(i, r)| {
                let mut v = to_json(r);
                v["areaMm2"] = serde_json::json!(r.area_mm2());
                // Exact R5.3.2 display; `null` (never a made-up "0.00") when
                // the exact kernel cannot represent it.
                v["displayAreaM2"] = serde_json::json!(rooms::display_polygon_area_m2(r));
                let label = labels.get(i).and_then(|l| l.as_ref());
                v["recordId"] = serde_json::json!(label.map(|(id, _, _)| id.to_string()));
                v["label"] = serde_json::json!(label.map(|(_, text, _)| text.clone()));
                v["labelAnchorMm"] = serde_json::json!(label.map(|(_, _, seed)| *seed));
                v["seedCandidateMm"] =
                    serde_json::json!(rooms::interior_point(r).map(|p| [p.x, p.y]));
                v
            }).collect::<Vec<_>>(),
            "roomRecords": records.iter().map(|(record, status)| serde_json::json!({
                "id": record.id.to_string(),
                "seedMm": record.seed_mm,
                // `null` and `""` stay different here: they are different
                // authoritative values and only the DISPLAY collapses them.
                "name": record.name,
                "floorStyleRef": record.floor_style_ref,
                "ceilingStyleRef": record.ceiling_style_ref,
                "status": status,
            })).collect::<Vec<_>>(),
            "floorAreaMm2": floor_area,
        });
        match serde_json::to_vec(&payload) {
            Ok(bytes) => release(bytes, out),
            Err(_) => status::REJECTED,
        }
    })
}

/// One opening's derived aperture (R4.2.1 cut, R4.0.4 leaf and glyph) on
/// `wall`, as the derived response carries it; `None` when its span does not
/// fit the host or the cut is degenerate.
fn aperture_json(
    wall: &domowik_format::Wall,
    opening: &domowik_format::Opening,
) -> Option<serde_json::Value> {
    let (s0, s1) = opening.span(wall.length_mm())?;
    let along = |t: f64| {
        domowik_format::Point::new(
            wall.start().x + (wall.end().x - wall.start().x) * t,
            wall.start().y + (wall.end().y - wall.start().y) * t,
        )
    };
    let (a, b) = (along(s0), along(s1));
    // R4.2.1: the footprint spans only the cut depth from the named face —
    // a partial-depth window is a notch, not a full-thickness hole.
    let host = walls::WallSolid::new(wall.start(), wall.end(), wall.thickness_mm);
    let rect = domowik_geometry::cuts::Aperture::of(opening, 0, &host)?.footprint(&host)?;
    let (hinge, tip) = walls::leaf_endpoints(opening, a, b);
    use domowik_format::OpeningMechanism as Mechanism;
    let axis = (b.x - a.x, b.y - a.y);
    let length = axis.0.hypot(axis.1);
    let normal = (-axis.1 / length, axis.0 / length);
    let offset = |p: domowik_format::Point, distance: f64| {
        domowik_format::Point::new(p.x + normal.0 * distance, p.y + normal.1 * distance)
    };
    let middle = domowik_format::Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let glyph: Vec<(domowik_format::Point, domowik_format::Point)> =
        match opening.effective_mechanism() {
            Mechanism::DoorHinged => vec![(hinge, tip)],
            Mechanism::DoorSliding => vec![(
                offset(a, wall.thickness_mm / 2.0 + 30.0),
                offset(b, wall.thickness_mm / 2.0 + 30.0),
            )],
            Mechanism::WindowSideHung => vec![(
                if opening.hinge == domowik_format::Hinge::NearStart {
                    a
                } else {
                    b
                },
                offset(middle, opening.width_mm / 5.0),
            )],
            Mechanism::WindowDoubleSwing => vec![
                (a, offset(middle, opening.width_mm / 5.0)),
                (b, offset(middle, opening.width_mm / 5.0)),
            ],
            Mechanism::WindowLift => vec![
                (offset(a, 30.0), offset(b, 30.0)),
                (offset(middle, 30.0), offset(middle, 90.0)),
            ],
        };
    Some(serde_json::json!({
        "id": opening.id.to_string(),
        "hostWallId": wall.id.to_string(),
        "kind": match opening.kind {
            domowik_format::OpeningKind::Door => "door",
            domowik_format::OpeningKind::Window => "window",
        },
        "u": opening.u,
        "widthMm": opening.width_mm,
        "heightMm": opening.height_mm,
        "sillMm": opening.sill_mm,
        "depthMm": opening.depth_mm,
        "side": opening.side,
        "opensToward": opening.opens_toward,
        "hingeEnd": opening.hinge,
        "mechanism": opening.effective_mechanism(),
        "cut": rect.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>(),
        "leaf": [[hinge.x, hinge.y], [tip.x, tip.y]],
        "glyphSegments": glyph.iter().map(|(x,y)| [[x.x,x.y],[y.x,y.y]]).collect::<Vec<_>>(),
        "swingDeg": opening.swing_deg,
    }))
}

// ---------------------------------------------------------------------------
// Hit testing
// ---------------------------------------------------------------------------

/// The widest hit radius this ABI accepts, in plan millimetres.
///
/// R7.1.3's largest logical-pixel radius is 24 (touch) and §1.3 clamps zoom to
/// a minimum of 0.0001 px/mm, so 24 / 0.0001 = 240,000 mm is the widest radius
/// any legal viewport can convert to. Anything beyond it is a caller defect —
/// an unclamped zoom, or millimetres confused with pixels — and is refused
/// rather than answered, because answering would quietly turn one click into a
/// document-wide scan.
const MAX_HIT_RADIUS_MM: f64 = 240_000.0;

/// The empty ordered result. One encoding, so the two paths that produce it
/// cannot drift.
const NO_HITS: &[u8] = b"[]";

/// A request to [`domowik_plan_hit_test`].
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HitTestRequest {
    level_id: Uuid,
    point_mm: [f64; 2],
    radius_mm: f64,
    /// Optional exact pick radius: logical pixels over `zoom` (logical pixels
    /// per millimetre). When both are present they REPLACE `radiusMm` for
    /// admission, so `T/z` is never rounded (ED-0112).
    radius_px: Option<f64>,
    zoom: Option<f64>,
}

/// The validated `(radiusPx, zoom)` pair of a request, if it has one: both or
/// neither, `radiusPx` finite and non-negative, `zoom` finite and positive,
/// and their quotient within [`MAX_HIT_RADIUS_MM`].
fn screen_radius(px: Option<f64>, zoom: Option<f64>) -> Result<Option<(f64, f64)>, String> {
    match (px, zoom) {
        (None, None) => Ok(None),
        (Some(px), Some(zoom))
            if px.is_finite()
                && px >= 0.0
                && zoom.is_finite()
                && zoom > 0.0
                && px / zoom <= MAX_HIT_RADIUS_MM =>
        {
            Ok(Some((px, zoom)))
        }
        _ => Err(format!(
            "radiusPx and zoom must be given together, radiusPx finite and >= 0, zoom finite \
             and > 0, and radiusPx/zoom <= {MAX_HIT_RADIUS_MM}"
        )),
    }
}

/// Ranked hit candidates at one point on one level, as JSON.
///
/// # Why this is not a snapshot plus geometry in the caller
///
/// The same reason as [`domowik_plan_derived`]. R7.1.3's classes, R6.3.1's
/// total order and R6.3.2's join-node dedupe are specification, and a Dart
/// reimplementation of them would be a second answer to "what did the user
/// click", drifting from the CLI the first time a rule changed. The core's
/// `HitIndex` is used as it stands; no geometry is copied here.
///
/// # Request
///
/// ```json
/// {"levelId":"…","pointMm":[x,y],"radiusMm":120.0}
/// ```
///
/// Optionally `"radiusPx":12,"zoom":0.1` (logical pixels per millimetre):
/// admission is then EXACTLY `distance ≤ radiusPx / zoom`, never a rounded
/// quotient, and `radiusMm` only bounds the search (ED-0112).
///
/// `radiusMm` is R7.1.3's logical-pixel radius (12 for mouse and stylus, 24
/// for touch) ALREADY converted to plan millimetres through the §1.2 inverse
/// transform. That conversion belongs to the viewport; the core has no view
/// transform and this call does not invent one either. It must be finite and
/// within `[0, 240000]` — see [`MAX_HIT_RADIUS_MM`].
///
/// # Response
///
/// An ORDERED array, top-ranked first, in R7.1.3 class precedence and then
/// R6.3.1's total key:
///
/// ```json
/// [{"class":"join-node","entityId":"…","closestMm":[x,y],
///   "distanceSquaredMm2":0.0}]
/// ```
///
/// `class` is one of `join-node`, `free-endpoint`, `region`. `entityId` is the
/// JOIN at a node and the WALL at an endpoint or a region. Distances are
/// SQUARED and never rooted, because R6.3.1 compares exact squared distances
/// and a square root makes two candidates that differ compare equal.
///
/// Returned distances and closest points are binary64 display values within
/// the certified enclosure of the exact ones. Admission and ordering of every
/// class — including wall and opening regions, which are the IDEAL
/// rectangles — use exact comparisons over the supplied inputs (ED-0112).
///
/// Nothing beyond that order is applied. Click cycling (R7.2.2), selection and
/// the primary item are the caller's; putting any of them behind this call
/// would put one policy in two places.
///
/// # Refusals and empties, which are not the same thing
///
/// - A malformed request, a non-finite coordinate or radius, or a radius
///   outside `[0, 240000]` is refused with [`status::MALFORMED_REQUEST`], and
///   `out` is not written.
/// - A FINITE point outside the plan domain — either component beyond
///   [`DERIVED_COORD_BOUND_MM`] widened by the radius — returns an EMPTY
///   array. Nothing indexable can be within reach of it, so the answer is
///   known without building an index or doing arithmetic on one, and the
///   query returns before either happens. Panning a long way is an ordinary
///   thing to do, not an error.
///
///   The DERIVED bound, not `COORD_BOUND_MM`: a wall's region is its
///   rectangle, which extends half a thickness beyond its stored centre line,
///   so a wall lying on the stored bound has candidate geometry outside it.
///   Culling at the stored bound would drop real hits on exactly those walls.
/// - An unknown level returns an empty array, for the reason
///   [`domowik_plan_derived`] gives: a level with nothing on it has no
///   candidates, and refusing would make the empty document a special case.
///
/// # Cost
///
/// Each level keeps its index, synced on the first query after the document
/// version changed: work proportional to what the edit changed, not a
/// rebuild (issue 0012). Alternating between levels at one version costs
/// nothing. See [`LevelIndexes`], and [`domowik_debug_hit_index_builds`] for
/// how this is asserted.
///
/// # Safety
///
/// `req` must address `req_len` readable bytes that stay valid and unmutated
/// for the duration of the call, and `out` must be a valid, writable
/// `domowik_buf*`. The buffer written on success is the CALLER's and must be
/// released with [`domowik_buffer_free`]; a non-zero return writes nothing and
/// transfers nothing.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_hit_test(
    handle: u64,
    req: *const u8,
    req_len: u64,
    out: *mut DomowikBuf,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        // Resolved BEFORE the request is parsed, as in `domowik_plan_execute`,
        // so the lease brackets the whole call and `close` cannot return while
        // this one is still on its way into the core.
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let Some(bytes) = borrow(req, req_len) else {
            return fail(&plan, status::INVALID_ARGUMENT, "request pointer is null");
        };
        let request: HitTestRequest = match serde_json::from_slice(bytes) {
            Ok(r) => r,
            Err(e) => return fail(&plan, status::MALFORMED_REQUEST, e.to_string()),
        };

        // Checked here rather than left to the core. §1.1 rejects non-finite
        // coordinates wherever they occur, and a NaN reaching the index would
        // not be refused by it — every comparison against NaN is false, so it
        // would return an empty list that looks exactly like a legitimate
        // miss. A caller cannot tell those apart; this makes it a status code.
        let [x, y] = request.point_mm;
        if !x.is_finite() || !y.is_finite() {
            return fail(
                &plan,
                status::MALFORMED_REQUEST,
                "pointMm components must be finite",
            );
        }
        if !request.radius_mm.is_finite()
            || request.radius_mm < 0.0
            || request.radius_mm > MAX_HIT_RADIUS_MM
        {
            return fail(
                &plan,
                status::MALFORMED_REQUEST,
                format!(
                    "radiusMm must be finite and within [0, {MAX_HIT_RADIUS_MM}]; got {}",
                    request.radius_mm
                ),
            );
        }
        let screen = match screen_radius(request.radius_px, request.zoom) {
            Ok(screen) => screen,
            Err(message) => return fail(&plan, status::MALFORMED_REQUEST, message),
        };
        // The search reach: the exact screen radius rounded up, or radiusMm.
        let radius = screen.map_or(request.radius_mm, |(px, zoom)| (px / zoom).next_up());

        // Both operands are finite and bounded, so the sum cannot overflow.
        // Rounding cannot discard an in-range binary64 query coordinate:
        // no representable value lies strictly between the exact sum and its
        // nearest rounded value. This argument needs no topology tolerance.
        let reach = DERIVED_COORD_BOUND_MM + radius;
        if x.abs() > reach || y.abs() > reach {
            return release(NO_HITS.to_vec(), out);
        }
        let point = domowik_format::Point::new(x, y);

        let hits = {
            let mut locked = plan.lock().unwrap_or_else(|e| e.into_inner());
            // The SAME quantity `domowik_plan_state` reports as
            // `document_revision`, read through the same helper so the two
            // cannot drift. It is only the DOCUMENT half of this key: the
            // level below is the other half, and switching level rebuilds
            // with the revision unchanged.
            match locked.synced_hit(request.level_id) {
                Ok(index) => match screen {
                    Some((px, zoom)) => index.hit_test_screen(point, px, zoom),
                    None => index.hit_test(point, radius),
                },
                Err(e) => {
                    locked.last_error = e;
                    return status::REJECTED;
                }
            }
        };

        let payload: Vec<serde_json::Value> = hits
            .iter()
            .map(|hit| {
                serde_json::json!({
                    "class": match hit.class {
                        HitClass::JoinNode => "join-node",
                        HitClass::FreeEndpoint => "free-endpoint",
                        HitClass::Opening => "opening",
                        HitClass::Region => "region",
                        HitClass::PlaceholderBox => "placeholder-box",
                        // Marquee-only (issue 0111): a point hit never carries
                        // it, because resolved instances are point-hit by the
                        // asset store's own query, not by HitIndex.
                        HitClass::AssetFootprint => "asset-footprint",
                    },
                    "entityId": hit.entity.to_string(),
                    "closestMm": [hit.closest.x, hit.closest.y],
                    "distanceSquaredMm2": hit.distance_squared,
                })
            })
            .collect();
        match serde_json::to_vec(&payload) {
            Ok(bytes) => release(bytes, out),
            Err(e) => fail(&plan, status::REJECTED, e.to_string()),
        }
    })
}

// ---------------------------------------------------------------------------
// Snapping
// ---------------------------------------------------------------------------

/// The fixed wire shape for [`domowik_plan_snap`].  Preferences default to
/// enabled exactly as a fresh local view state does (R6.4.3).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapRequest {
    level_id: Uuid,
    point_mm: [f64; 2],
    radius_mm: f64,
    visible_rect_mm: [f64; 4],
    anchor_mm: Option<[f64; 2]>,
    previous_direction: Option<[f64; 2]>,
    /// The previous accepted segment's stored endpoints, preferred over
    /// `previousDirection`: the relative angle family is then exact over the
    /// stored coordinates (ED-0112).
    previous_segment_mm: Option<[[f64; 2]; 2]>,
    /// See [`HitTestRequest::radius_px`].
    radius_px: Option<f64>,
    zoom: Option<f64>,
    #[serde(default)]
    excluded_wall_ids: Vec<Uuid>,
    #[serde(default)]
    excluded_join_ids: Vec<Uuid>,
    grid_spacing_mm: Option<f64>,
    #[serde(default = "enabled_by_default")]
    grid_enabled: bool,
    #[serde(default = "enabled_by_default")]
    object_enabled: bool,
    #[serde(default = "enabled_by_default")]
    angle_enabled: bool,
    #[serde(default)]
    alt_disabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

fn snap_class_name(class: SnapClass) -> &'static str {
    match class {
        SnapClass::JoinNode => "join-node",
        SnapClass::FreeEndpoint => "free-endpoint",
        SnapClass::Edge => "edge",
        SnapClass::Axis => "axis",
        SnapClass::Grid => "grid",
        SnapClass::Angle => "angle",
    }
}

fn snap_feature_json(feature: SnapFeature) -> serde_json::Value {
    match feature {
        SnapFeature::Entity { point } => serde_json::json!({
            "kind": "entity", "pointMm": [point.x, point.y],
        }),
        // `span` is deliberately NOT serialized yet. The core carries the
        // constant-owner sub-span index as part of the edge identity
        // (ED-0103 Part 2), but adding a field here changes an externally
        // observable response shape, and ABI 13 is allocated to another
        // issue. Held for ABI coordination; no current consumer reads the
        // feature key, so withholding it costs nothing today.
        SnapFeature::Edge {
            component,
            ring,
            segment,
            span: _,
            discriminator,
        } => serde_json::json!({
            "kind": "edge", "component": component, "ring": ring,
            "segment": segment, "discriminator": discriminator,
        }),
        SnapFeature::Axis { on_segment } => serde_json::json!({
            "kind": "axis", "onSegment": on_segment,
        }),
        SnapFeature::Grid { i, j } => serde_json::json!({ "kind": "grid", "i": i, "j": j }),
        SnapFeature::Angle {
            direction: (dx, dy),
            degrees,
        } => serde_json::json!({
            "kind": "angle", "direction": [dx, dy], "degrees": degrees,
        }),
    }
}

/// Ranked snapping candidates for one active-level pointer query.
///
/// The request is JSON:
/// `{"levelId":…, "pointMm":[x,y], "radiusMm":r,
/// "visibleRectMm":[minX,minY,maxX,maxY], …}`, optionally with
/// `"radiusPx"`/`"zoom"` (exact pick radius, as for hit testing) and
/// `"previousSegmentMm":[[x,y],[x,y]]` (the accepted segment's endpoints,
/// preferred over `"previousDirection"`) — ED-0112.  It is deliberately a
/// bounded, read-only bridge to `SnapIndex`: document features are cached by
/// `(levelId, document_revision)`, while its view-dependent axis broad phase
/// remains inside the core index.  Unknown levels are empty for document
/// features, just like hit testing, but local grid/angle candidates remain
/// available without a document level.
///
/// On success writes `{ "candidates": [...] }`; a core `InvalidInput`,
/// `GridRange`, or unavailable edge ownership is a malformed request and
/// leaves `out` untouched.
///
/// # Safety
///
/// `req` must address `req_len` readable immutable bytes for this call, and
/// `out` must be a valid writable `domowik_buf*`. On success that buffer is
/// caller-owned and must be released by `domowik_buffer_free`.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_snap(
    handle: u64,
    req: *const u8,
    req_len: u64,
    out: *mut DomowikBuf,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let Some(bytes) = borrow(req, req_len) else {
            return fail(&plan, status::INVALID_ARGUMENT, "request pointer is null");
        };
        let request: SnapRequest = match serde_json::from_slice(bytes) {
            Ok(request) => request,
            Err(error) => return fail(&plan, status::MALFORMED_REQUEST, error.to_string()),
        };
        let finite = |p: [f64; 2]| p[0].is_finite() && p[1].is_finite();
        if !finite(request.point_mm)
            || !request.radius_mm.is_finite()
            || request.radius_mm < 0.0
            || request.radius_mm > MAX_HIT_RADIUS_MM
            || !request.visible_rect_mm.iter().all(|v| v.is_finite())
            || request.visible_rect_mm[0] > request.visible_rect_mm[2]
            || request.visible_rect_mm[1] > request.visible_rect_mm[3]
            || request.anchor_mm.is_some_and(|p| !finite(p))
            || request.previous_direction.is_some_and(|d| !finite(d))
            || request
                .previous_segment_mm
                .is_some_and(|[a, b]| !finite(a) || !finite(b))
            || request.grid_spacing_mm.is_some_and(|s| !s.is_finite())
        {
            return fail(
                &plan,
                status::MALFORMED_REQUEST,
                "snap request contains invalid coordinates",
            );
        }
        let screen = match screen_radius(request.radius_px, request.zoom) {
            Ok(screen) => screen,
            Err(message) => return fail(&plan, status::MALFORMED_REQUEST, message),
        };
        let point = domowik_format::Point::new(request.point_mm[0], request.point_mm[1]);
        let view = Aabb::from_corners(
            domowik_format::Point::new(request.visible_rect_mm[0], request.visible_rect_mm[1]),
            domowik_format::Point::new(request.visible_rect_mm[2], request.visible_rect_mm[3]),
        );
        let candidates = {
            let mut locked = plan.lock().unwrap_or_else(|e| e.into_inner());
            let query = SnapQuery {
                point,
                radius_mm: request.radius_mm,
                visible_rect_mm: view,
                anchor: request
                    .anchor_mm
                    .map(|p| domowik_format::Point::new(p[0], p[1])),
                previous_direction: request.previous_direction.map(|d| (d[0], d[1])),
                previous_segment: request.previous_segment_mm.map(|[a, b]| {
                    (
                        domowik_format::Point::new(a[0], a[1]),
                        domowik_format::Point::new(b[0], b[1]),
                    )
                }),
                screen_radius: screen,
                excluded_walls: &request.excluded_wall_ids,
                excluded_joins: &request.excluded_join_ids,
                grid_spacing_mm: request.grid_spacing_mm,
                grid_enabled: request.grid_enabled,
                object_enabled: request.object_enabled,
                angle_enabled: request.angle_enabled,
                alt_disabled: request.alt_disabled,
            };
            locked.synced_snap(request.level_id).query(&query)
        };
        let candidates = match candidates {
            Ok(candidates) => candidates,
            Err(error) => {
                let message = match error {
                    SnapError::InvalidInput => "snap core rejected invalid input",
                    SnapError::GridRange => "snap core refused oversized grid range",
                };
                return fail(&plan, status::MALFORMED_REQUEST, message);
            }
        };
        let payload = serde_json::json!({ "candidates": candidates.into_iter().map(|candidate| {
            serde_json::json!({
                "class": snap_class_name(candidate.class),
                "entityId": candidate.entity.map(|id| id.to_string()),
                "pointMm": [candidate.at.x, candidate.at.y],
                "feature": snap_feature_json(candidate.feature),
                "guideMm": candidate.guide.map(|(a, b)| [[a.x, a.y], [b.x, b.y]]),
            })
        }).collect::<Vec<_>>() });
        match serde_json::to_vec(&payload) {
            Ok(bytes) => release(bytes, out),
            Err(error) => fail(&plan, status::REJECTED, error.to_string()),
        }
    })
}

/// Number of snap-index refreshes (a level's index built or synced to a new
/// document revision, by a snap query or a derived-geometry read), or `-1`
/// for an unknown handle.
#[no_mangle]
pub extern "C" fn domowik_debug_snap_index_builds(handle: u64) -> i64 {
    match catch_unwind(AssertUnwindSafe(|| {
        let (plan, _lease) = resolve(handle)?;
        let locked = plan.lock().unwrap_or_else(|e| e.into_inner());
        Some(locked.snap_index_builds)
    })) {
        Ok(Some(builds)) => i64::try_from(builds).unwrap_or(i64::MAX),
        Ok(None) | Err(_) => -1,
    }
}

/// A request to [`domowik_plan_marquee`]. The two corners may be given in
/// EITHER order — any two opposite corners of the rectangle.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarqueeRequest {
    level_id: Uuid,
    corner1_mm: [f64; 2],
    corner2_mm: [f64; 2],
}

/// The R7.3 marquee query, READ-ONLY: entity IDs whose interaction shape
/// intersects the rectangle, as JSON, in ascending marquee-key order — the
/// SAME order R7.3.2 requires additive marquee to process candidates in, so
/// a caller finds the new PRIMARY by taking the last of these not already in
/// its selection, and does nothing at all when every one of them already is
/// (R7.3.2's "selection AND primary remain UNCHANGED").
///
/// # Why this is not geometry in the caller
///
/// [`domowik_geometry::hit::marquee_all`] is R7.3's answer, in the SAME sense
/// [`domowik_plan_hit_test`] is R7.1.3's: boundary-inclusive rectangle
/// intersection (R7.3.1), the marquee-key ordering, and the zero-area case
/// (R7.3.2) — which it resolves by delegating to the SAME point-hit index
/// `domowik_plan_hit_test` uses, returning only the top-ranked candidate
/// there, never the whole feature list. This call is a pass-through to that
/// function and adds no second definition of what a rectangle selects
/// (ED-0101); it does not even special-case the degenerate corner itself.
///
/// Candidates are the active level's wall regions and openings' hosts, the
/// degraded instances' §9.6 placeholder boxes, and — since issue 0111 — the
/// RESOLVED instances' conservative footprints, whose candidacy and bounds
/// the asset store supplies: one class list per content revision, ordered
/// together by the R7.3.2 key with the asset class ranked after every
/// geometry class. Degraded instances appear once, as placeholders, never
/// also as assets. A coincident-corner rectangle keeps point-hit semantics
/// and answers from the geometry classes alone: resolved instances are
/// point-hit by `domowik_plan_assets`, which a degenerate rectangle never
/// merges. Declared dimensions use §9.6's transformed width/depth bounds;
/// absent dimensions use the fixed fallback. One-dimensional rectangles keep
/// intersection semantics. Wall polygon construction still inherits issue
/// 0012's binary64 numerical limits.
///
/// # Request
///
/// ```json
/// {"levelId":"…","corner1Mm":[x,y],"corner2Mm":[x,y]}
/// ```
///
/// Both corners must be finite (R7.3.2, and the general section 1.4 rule
/// that non-finite input discards the whole event): a non-finite component
/// refuses the WHOLE call with `DOMOWIK_MALFORMED_REQUEST` rather than
/// answering from a NaN-laden or inverted box — the core's `Aabb` performs
/// no such check itself, so this boundary is where it is enforced, exactly
/// as `domowik_plan_hit_test` enforces `pointMm`'s finiteness before the
/// index ever sees it.
///
/// # Response
///
/// ```json
/// {"entityIds": ["…", …]}
/// ```
///
/// Only ACTIVE-level entities are eligible (R7.1.3, R7.5.1, R7.5.4): the
/// core's own `level_id` filter, not a second one here — non-active levels
/// are never marquee-selected regardless of XY overlap with the active one.
///
/// Read-only: this call never commits an operation, never advances
/// [`document_revision`], and never touches undo/redo history. Selection is
/// local view state (R7.1.2), never document history, and this query is how
/// that state is COMPUTED, not a document mutation of its own.
///
/// # Safety
///
/// `req` must address `req_len` readable bytes for the duration of the call;
/// `out` must be a valid, writable `domowik_buf*`.
#[no_mangle]
pub unsafe extern "C" fn domowik_plan_marquee(
    handle: u64,
    req: *const u8,
    req_len: u64,
    out: *mut DomowikBuf,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let Some(bytes) = borrow(req, req_len) else {
            return fail(&plan, status::INVALID_ARGUMENT, "request pointer is null");
        };
        let request: MarqueeRequest = match serde_json::from_slice(bytes) {
            Ok(r) => r,
            Err(e) => return fail(&plan, status::MALFORMED_REQUEST, e.to_string()),
        };

        let [x0, y0] = request.corner1_mm;
        let [x1, y1] = request.corner2_mm;
        if !x0.is_finite() || !y0.is_finite() || !x1.is_finite() || !y1.is_finite() {
            return fail(
                &plan,
                status::MALFORMED_REQUEST,
                "corner1Mm/corner2Mm components must be finite",
            );
        }
        let rect = Aabb::from_corners(
            domowik_format::Point::new(x0, y0),
            domowik_format::Point::new(x1, y1),
        );

        let ids = {
            let mut plan = plan.lock().unwrap_or_else(|e| e.into_inner());
            // One revision's candidacies for both halves of the merge: the
            // geometry classes from the synced index, resolved instances from
            // the footprint index (issue 0111).
            let assets = plan.resolved_footprints(request.level_id);
            match plan.synced_hit(request.level_id) {
                Ok(index) => index.marquee_with_assets(rect, &assets),
                Err(e) => {
                    plan.last_error = e;
                    return status::REJECTED;
                }
            }
        };

        let payload: Vec<String> = ids.into_iter().map(|id| id.to_string()).collect();
        match serde_json::to_vec(&serde_json::json!({ "entityIds": payload })) {
            Ok(bytes) => release(bytes, out),
            Err(e) => fail(&plan, status::REJECTED, e.to_string()),
        }
    })
}

/// How many times `handle` has REFRESHED a hit index — built one or synced
/// one to a new document revision (issue 0012) — or `-1` if the handle is
/// unknown or the call itself panicked.
///
/// A DEBUG export, like [`domowik_debug_cancel_epoch`], and for the same
/// reason: the property that matters is not observable from the rest of the
/// ABI. A cache that silently rebuilt on every query would return byte-
/// identical answers, so a test written against the RESULTS passes on the
/// broken implementation — and a test that passes on the bug is worse than no
/// test. This makes "reused across reads and saves, refreshed by an edit"
/// directly assertable.
///
/// Takes the plan lock and a call lease, so it serialises with hit testing and
/// `close` waits for it. That is deliberate: a probe that read the counter
/// without the lock could report a build that had not finished.
#[no_mangle]
pub extern "C" fn domowik_debug_hit_index_builds(handle: u64) -> i64 {
    match catch_unwind(AssertUnwindSafe(|| {
        let (plan, _lease) = resolve(handle)?;
        let locked = plan.lock().unwrap_or_else(|e| e.into_inner());
        Some(locked.hit_index_builds)
    })) {
        Ok(Some(builds)) => i64::try_from(builds).unwrap_or(i64::MAX),
        Ok(None) | Err(_) => -1,
    }
}

/// How many times `handle`'s warning cache has actually RECOMPUTED (a
/// revision miss), or `-1` if the handle is unknown or the call panicked.
///
/// A DEBUG export, for the same reason as [`domowik_debug_hit_index_builds`]:
/// a mutation and the [`domowik_plan_state`] read that immediately follows it
/// would return byte-identical `warnings` whether or not the second call
/// recomputed, so that property is not otherwise assertable from the ABI.
/// This makes "one computation per accepted revision, shared by both calls"
/// directly assertable rather than merely plausible.
#[no_mangle]
pub extern "C" fn domowik_debug_warnings_computations(handle: u64) -> i64 {
    match catch_unwind(AssertUnwindSafe(|| {
        let (plan, _lease) = resolve(handle)?;
        let locked = plan.lock().unwrap_or_else(|e| e.into_inner());
        Some(locked.warnings_computations)
    })) {
        Ok(Some(n)) => i64::try_from(n).unwrap_or(i64::MAX),
        Ok(None) | Err(_) => -1,
    }
}

/// Writes `handle`'s sticky last error to `out` as UTF-8, without clearing it.
///
/// An empty buffer means no error has been recorded. The string stays valid
/// from a failing return until the next call on that handle.
///
/// # Safety
///
/// `out` must be a valid, writable `domowik_buf*`.
#[no_mangle]
pub unsafe extern "C" fn domowik_last_error(handle: u64, out: *mut DomowikBuf) -> i32 {
    guard(|| {
        if out.is_null() {
            return status::INVALID_ARGUMENT;
        }
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let message = {
            let plan = plan.lock().unwrap_or_else(|e| e.into_inner());
            plan.last_error.clone()
        };
        release(message.into_bytes(), out)
    })
}

/// Frees a buffer this library issued, identified by its release TOKEN.
///
/// Refuses — with [`status::UNKNOWN_BUFFER`] and without freeing anything — a
/// token this library did not issue or has already reclaimed. Because tokens
/// are monotonic and never reused, that refusal is exact: a double free from
/// Dart or C is a returned error code and can never reach the allocator, and
/// no two live buffers can share a key. See [`DomowikBuf::token`] for the two
/// ways a pointer-keyed registry got this wrong.
///
/// This takes no pointer at all, so there is no pointer for a caller to get
/// wrong. Passing a value that is not a live token is safe and reported.
#[no_mangle]
pub extern "C" fn domowik_buffer_free(token: u64) -> i32 {
    guard(|| {
        let Some((ptr, len)) = registry().buffers.remove(&token) else {
            return status::UNKNOWN_BUFFER;
        };
        // SAFETY: the registry only ever holds pointers this library produced
        // with `Box::into_raw` on a boxed slice of exactly `len` bytes, and
        // removing the token above means no other thread can reach it.
        // `into_boxed_slice` made capacity equal length, so this reconstructs
        // the layout the allocator handed out.
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                ptr as *mut u8,
                len,
            )));
        }
        status::OK
    })
}

/// Closes a plan. The handle is never reissued, so later use of it is a
/// detected [`status::UNKNOWN_HANDLE`] rather than undefined behaviour.
///
/// **Returns only after every call already admitted on this plan — one that
/// has taken its [`Lease`], whether or not it had reached the plan lock yet —
/// has finished its lease-bracketed work, and every effect it publishes
/// THROUGH THIS LIBRARY is visible**: completed session mutations, a written
/// `out` buffer with its token registered, and any last-error update. The
/// call's own status CODE is a different thing — it is computed under the
/// lease, but delivered to its own caller only after the lease drops (the
/// closure returns, `guard` matches its result, the `extern "C"` return
/// sequence hands the value back), so a call whose only output is a status
/// code — [`domowik_busy`], [`domowik_cancel`] — publishes nothing this
/// promise covers; `close` guarantees its body ran to completion, not that
/// its caller has the result in hand. That matters to a caller tearing down:
/// the first version removed the registry entry and returned immediately, so
/// `domowik_plan_close` could come back while another thread was still
/// inside the core.
///
/// The registry lock is released BEFORE waiting, so a long-running commit on
/// one plan cannot stall every other handle; and the plan is dropped after
/// that, so a large `Session`'s deallocation does not happen under the global
/// lock either.
///
/// Lock order is registry-then-plan here, which is safe only because no path
/// ever takes them the other way round: every export that holds the plan lock
/// drops it before calling [`release`], which is the only thing that reaches
/// for the registry afterwards.
///
/// **What is NOT promised:** this is not permission to unload the library. A
/// calling thread can still be a few instructions past its lease drop,
/// unwinding that call's own epilogue, when this returns on another thread —
/// closing a plan is not the same claim as no thread executing library text.
/// A caller that wants to dlclose/FreeLibrary this library must still join
/// every thread of its own that could be inside an ABI call, and free every
/// buffer it still owns, before doing so. Separately: a call already admitted
/// still completes and returns its normal status, and its effects go away
/// with the plan. A caller that closes a plan whose result it still wants
/// gets a truthful `OK` for work it then discards. Closing is the caller's
/// statement that it is done with the PLAN, not a synchronization barrier for
/// its own threads.
#[no_mangle]
pub extern "C" fn domowik_plan_close(handle: u64) -> i32 {
    guard(|| {
        let Some(entry) = registry().plans.remove(&handle) else {
            return status::UNKNOWN_HANDLE;
        };
        #[cfg(test)]
        test_hooks::on_registry_removed(handle);
        // The registry lock is released above, so a long call on this plan
        // cannot stall every other handle while we wait — and no LATER call
        // can take a lease, because none can find the plan any more.
        entry.gate.wait_for_quiescence(handle);
        // Dropped after the wait and outside the registry lock, so a large
        // `Session` is not deallocated under the global lock either.
        drop(entry);
        status::OK
    })
}

/// Panics on purpose, to prove containment from the far side of the boundary.
///
/// A containment mechanism nobody has watched fail is a claim, not a
/// mechanism.
#[no_mangle]
pub extern "C" fn domowik_debug_panic() -> i32 {
    guard(|| panic!("deliberate panic from the bridge, to prove containment"))
}

/// Panics while HOLDING the plan's lock, poisoning it.
///
/// [`domowik_debug_panic`] panics holding nothing, so "the library still works
/// afterwards" proved containment but said nothing about poison recovery —
/// which is a separate claim this ABI makes and which nothing exercised.
/// Found by adversarial review (Codex, gpt-5.6-sol, 2026-09-01).
#[no_mangle]
pub extern "C" fn domowik_debug_panic_holding_lock(handle: u64) -> i32 {
    guard(|| {
        let Some((plan, _lease)) = resolve(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        let _held = plan.lock().unwrap_or_else(|e| e.into_inner());
        panic!("deliberate panic while holding the plan lock");
    })
}

/// Spins for up to `millis`, observing this handle's cancellation flag.
///
/// The core has no long-running operation yet, so cancellation is probed with
/// a stand-in rather than left untested until one exists. Returns
/// [`status::CANCELED`] if the flag was set, [`status::OK`] if it ran to
/// completion.
#[no_mangle]
pub extern "C" fn domowik_busy(handle: u64, millis: u64) -> i32 {
    guard(|| {
        let Some((epoch, _lease)) = resolve_epoch(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        // The epoch is READ at the start, never cleared. A cancel that arrives
        // any time from here on changes it and is observed; one that arrived
        // earlier is already folded into `started_at` and does not leak into
        // this operation. The previous shape cleared a flag here, so a cancel
        // landing between the lookup and the clear was silently erased and the
        // caller got OK from `domowik_cancel` for nothing.
        let started_at = epoch.load(Ordering::SeqCst);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(millis);
        while std::time::Instant::now() < deadline {
            if epoch.load(Ordering::SeqCst) != started_at {
                return status::CANCELED;
            }
            std::thread::yield_now();
        }
        status::OK
    })
}

/// The handle's cancellation epoch, or `-1` if the handle is unknown.
///
/// A DEBUG export, like [`domowik_debug_panic`]. It exists because the
/// property that matters — no operation ever writes to the cancellation state,
/// so a recorded cancel cannot be erased — is not observable from the rest of
/// the ABI. The losing interleaving of the old clearing-flag protocol is a few
/// instructions wide, and a stress test that races it passed on the broken
/// implementation too. A test that passes on the bug is worse than no test, so
/// the invariant is exposed and asserted directly instead.
#[no_mangle]
pub extern "C" fn domowik_debug_cancel_epoch(handle: u64) -> i64 {
    match catch_unwind(AssertUnwindSafe(|| {
        registry()
            .plans
            .get(&handle)
            .map(|e| e.cancel_epoch.load(Ordering::SeqCst))
    })) {
        Ok(Some(epoch)) => i64::try_from(epoch).unwrap_or(i64::MAX),
        // An unknown handle and a contained panic are both `-1` here; this is
        // a debug probe, and neither is a value a caller acts on.
        Ok(None) | Err(_) => -1,
    }
}

/// Requests cancellation of whatever `handle` is currently doing.
///
/// Deliberately does NOT take the per-handle lock: the operation being
/// cancelled is holding it. A cancel that waits for the work it is cancelling
/// is not a cancel.
#[no_mangle]
pub extern "C" fn domowik_cancel(handle: u64) -> i32 {
    guard(|| {
        // Only the REGISTRY lock is taken, and only for the lookup. The
        // operation being cancelled holds the PLAN lock, which this never
        // touches.
        let Some((epoch, _lease)) = resolve_epoch(handle) else {
            return status::UNKNOWN_HANDLE;
        };
        epoch.fetch_add(1, Ordering::SeqCst);
        status::OK
    })
}

/// Runs `body` with panics contained, mapping one to `-1`.
///
/// The COUNT functions cannot share [`guard`]: it returns status codes, so a
/// contained panic would come back as the number 6 and be indistinguishable
/// from "six outstanding buffers". A leak check that silently reports a panic
/// as a plausible count is worse than no leak check, because the harness would
/// pass. Found by re-reading this file adversarially before review.
fn guard_count(body: impl FnOnce() -> usize) -> i64 {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(count) => i64::try_from(count).unwrap_or(i64::MAX),
        Err(_) => -1,
    }
}

/// How many buffers this library has issued and not reclaimed. A leak check
/// for harnesses; zero at the end of a well-behaved run, `-1` if the call
/// itself panicked.
#[no_mangle]
pub extern "C" fn domowik_outstanding_buffers() -> i64 {
    guard_count(|| registry().buffers.len())
}

/// How many plans are open. `-1` if the call itself panicked.
#[no_mangle]
pub extern "C" fn domowik_open_plans() -> i64 {
    guard_count(|| registry().plans.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::{Entity, Level, COORD_BOUND_MM, DIMENSION_CAP_MM};

    /// A request creating one level, as JSON on the wire.
    fn create_level_request() -> Vec<u8> {
        let op = Operation::create(Entity::Level(Level::new("Ground")));
        serde_json::to_vec(&serde_json::json!({ "operations": [op] })).expect("encodes")
    }

    /// Calls `f` with an out-buffer, returns the status and the bytes, and
    /// frees the buffer through the ABI so every test also exercises the free
    /// path.
    fn call_returning(f: impl FnOnce(*mut DomowikBuf) -> i32) -> (i32, Vec<u8>) {
        let mut buf = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        let code = f(&mut buf);
        if code != status::OK {
            return (code, Vec::new());
        }
        let bytes = unsafe { std::slice::from_raw_parts(buf.data, buf.len as usize) }.to_vec();
        assert_eq!(domowik_buffer_free(buf.token), status::OK);
        (code, bytes)
    }

    fn open_plan() -> u64 {
        let mut handle = 0u64;
        assert_eq!(unsafe { domowik_plan_create(&mut handle) }, status::OK);
        handle
    }

    /// Builds a closed 4000x3000 room and returns (handle, levelId).
    fn plan_with_a_room() -> (u64, uuid::Uuid) {
        let h = open_plan();
        let level = domowik_format::Level::new("Ground");
        let level_id = level.id;
        let op = Operation::create(Entity::Level(level));
        let req = serde_json::to_vec(&serde_json::json!({ "operations": [op] })).unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        let corners = [(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)];
        for i in 0..4 {
            let (x0, y0) = corners[i];
            let (x1, y1) = corners[(i + 1) % 4];
            let wall = domowik_format::Wall::new(
                level_id,
                domowik_format::Point::new(x0, y0),
                domowik_format::Point::new(x1, y1),
                200.0,
                2500.0,
            );
            let op = Operation::create(Entity::Wall(wall));
            let req = serde_json::to_vec(&serde_json::json!({ "operations": [op] })).unwrap();
            let (code, _) = call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            });
            assert_eq!(code, status::OK, "wall {i}");
        }
        (h, level_id)
    }

    /// R3.1.7: a warning attaches to the batch RESULT itself. `execute`'s own
    /// response must carry it — a caller must not need a second round trip
    /// through `plan_state` to see one.
    #[test]
    fn execute_response_carries_the_wall_area_overlap_warning_it_creates() {
        let h = open_plan();
        let level = domowik_format::Level::new("Ground");
        let level_id = level.id;
        let req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Level(level))] }),
        )
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        let wall_a = domowik_format::Wall::new(
            level_id,
            domowik_format::Point::new(0.0, 0.0),
            domowik_format::Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Wall(wall_a))] }),
        )
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        // Parallel, 50mm apart, t=200 each: bands overlap with no axis
        // contact at all (R3.1.2's last row).
        let wall_b = domowik_format::Wall::new(
            level_id,
            domowik_format::Point::new(0.0, 50.0),
            domowik_format::Point::new(4000.0, 50.0),
            200.0,
            2500.0,
        );
        let req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Wall(wall_b))] }),
        )
        .unwrap();
        let (code, body) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK, "the warning does not block the batch");
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let warnings = response["warnings"].as_array().expect("warnings array");
        assert!(
            warnings.iter().any(|w| w["code"] == "wall-area-overlap"),
            "execute's OWN response must carry the warning, got {response}"
        );
        // Every existing field keeps its shape: additive, not an ABI change.
        assert_eq!(response["entities"], 3, "level + wall_a + wall_b");
        assert_eq!(response["undo_depth"], 3, "three committed batches");

        // `undo` compensates the overlapping wall away; ITS OWN response
        // must reflect the resulting warning-free state, not a stale one.
        let (code, body) = call_returning(|out| unsafe { domowik_plan_undo(h, out) });
        assert_eq!(code, status::OK);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            response["warnings"],
            serde_json::json!([]),
            "undo's own response must already show the warning cleared"
        );

        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// A mutation's own response and the `plan_state` read that immediately
    /// follows it — the overwhelmingly common GUI-refresh sequence — must
    /// share ONE warning computation per accepted revision, not pay for it
    /// twice. Asserted through `domowik_debug_warnings_computations`, not
    /// timing: two byte-identical `warnings` arrays prove nothing about
    /// whether the second call recomputed them.
    #[test]
    fn a_mutation_and_the_state_read_after_it_share_one_warning_computation() {
        let h = open_plan();
        let level = domowik_format::Level::new("Ground");
        let level_id = level.id;
        let req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Level(level))] }),
        )
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        let wall_a = domowik_format::Wall::new(
            level_id,
            domowik_format::Point::new(0.0, 0.0),
            domowik_format::Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Wall(wall_a))] }),
        )
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        // Every successful mutation computes its OWN `warnings` (R3.1.7
        // attaches one to every batch result, not only ones that find
        // something), so the two commits above already paid for it twice.
        // What matters from here is the DELTA, not an absolute count.
        let base = domowik_debug_warnings_computations(h);

        // Parallel, 50mm apart, t=200 each: bands overlap with no axis
        // contact at all.
        let wall_b = domowik_format::Wall::new(
            level_id,
            domowik_format::Point::new(0.0, 50.0),
            domowik_format::Point::new(4000.0, 50.0),
            200.0,
            2500.0,
        );
        let req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Wall(wall_b))] }),
        )
        .unwrap();
        let (code, body) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(response["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "wall-area-overlap"));
        assert_eq!(
            domowik_debug_warnings_computations(h),
            base + 1,
            "the mutation's OWN response computes the cache once"
        );

        // `plan_state`, immediately after, asks about the SAME accepted
        // revision: it must reuse what the mutation already computed.
        let (code, body) = call_returning(|out| unsafe { domowik_plan_state(h, out) });
        assert_eq!(code, status::OK);
        let state: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(state["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "wall-area-overlap"));
        assert_eq!(
            domowik_debug_warnings_computations(h),
            base + 1,
            "plan_state must reuse the mutation's cache for the same revision, not recompute"
        );

        // A repeated `plan_state` read changes nothing further either.
        let (code, _) = call_returning(|out| unsafe { domowik_plan_state(h, out) });
        assert_eq!(code, status::OK);
        assert_eq!(domowik_debug_warnings_computations(h), base + 1);

        // A later mutation ends a NEW revision and does pay to recompute —
        // exactly once, not merged with the stale entry.
        let (code, _) = call_returning(|out| unsafe { domowik_plan_undo(h, out) });
        assert_eq!(code, status::OK);
        assert_eq!(
            domowik_debug_warnings_computations(h),
            base + 2,
            "a new accepted revision recomputes once"
        );

        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// The gesture path (`domowik_plan_edit`) must carry the same additive
    /// `warnings` field as `execute`/`undo`/`redo` — one shape across every
    /// mutating entry point.
    #[test]
    fn edit_gesture_response_carries_the_wall_area_overlap_warning_it_creates() {
        let (h, level) = plan_with_a_room();
        // A wall parallel to and 50mm from the room's first wall
        // ((0,0)-(4000,0), t=200), far from every corner: no axis contact,
        // positive-area band overlap.
        let req = serde_json::to_vec(&serde_json::json!({
            "kind": "addWall", "levelId": level,
            "start": [500.0, 50.0], "end": [3500.0, 50.0],
            "thicknessMm": 200.0, "heightMm": 2500.0
        }))
        .unwrap();
        let (code, body) = call_returning(|out| unsafe {
            domowik_plan_edit(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK, "the warning does not block the gesture");
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let warnings = response["warnings"].as_array().expect("warnings array");
        assert!(
            warnings.iter().any(|w| w["code"] == "wall-area-overlap"),
            "the gesture path's own response must carry the warning too, got {response}"
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// A `setWall` gesture that changes ONLY `thicknessMm` — no `start`/`end`
    /// at all, the same axis before and after — must still be reclassified:
    /// the gesture's own response introduces the warning, `plan_state`
    /// immediately after reuses that same computation, and undo/redo
    /// restore/reapply both the thickness and the warning it causes, each
    /// paying for exactly one recomputation of its own (new) revision.
    #[test]
    fn edit_set_wall_thickness_only_toggles_the_overlap_warning_through_undo_redo() {
        let h = open_plan();
        let level = domowik_format::Level::new("Ground");
        let level_id = level.id;
        let req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Level(level))] }),
        )
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        let wall_a = domowik_format::Wall::new(
            level_id,
            domowik_format::Point::new(0.0, 0.0),
            domowik_format::Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Wall(wall_a))] }),
        )
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        // 250mm from wall_a, t=200 each: bands [-100,100] and [150,350], a
        // clear 50mm gap.
        let wall_b = domowik_format::Wall::new(
            level_id,
            domowik_format::Point::new(0.0, 250.0),
            domowik_format::Point::new(4000.0, 250.0),
            200.0,
            2500.0,
        );
        let wall_b_id = wall_b.id;
        let req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Wall(wall_b))] }),
        )
        .unwrap();
        let (code, body) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            response["warnings"].as_array().unwrap().is_empty(),
            "250mm apart at t=200 each is clear"
        );

        let base = domowik_debug_warnings_computations(h);

        // Widening wall_b to t=400 brings its band down to 50mm, overlapping
        // wall_a's — through `setWall`, thickness only, no axis write.
        let req = serde_json::to_vec(&serde_json::json!({
            "kind": "setWall", "wallId": wall_b_id, "thicknessMm": 400.0
        }))
        .unwrap();
        let (code, body) = call_returning(|out| unsafe {
            domowik_plan_edit(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK, "a thickness-only widening is not refused");
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            response["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["code"] == "wall-area-overlap"),
            "the edit gesture's OWN response must carry the newly introduced \
             warning, got {response}"
        );
        assert_eq!(
            domowik_debug_warnings_computations(h),
            base + 1,
            "one computation for this accepted revision"
        );

        // The immediate state read shares that SAME computation.
        let (code, body) = call_returning(|out| unsafe { domowik_plan_state(h, out) });
        assert_eq!(code, status::OK);
        let state: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(state["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "wall-area-overlap"));
        assert_eq!(
            domowik_debug_warnings_computations(h),
            base + 1,
            "plan_state must reuse it, not recompute"
        );

        // Undo restores the original thickness and clears the warning; ITS
        // OWN response reflects that immediately, at the cost of exactly one
        // recomputation for the new (restored) revision.
        let (code, body) = call_returning(|out| unsafe { domowik_plan_undo(h, out) });
        assert_eq!(code, status::OK);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            response["warnings"].as_array().unwrap().is_empty(),
            "undo restores the clear state"
        );
        assert_eq!(domowik_debug_warnings_computations(h), base + 2);

        // Redo reapplies the widening and the warning it causes.
        let (code, body) = call_returning(|out| unsafe { domowik_plan_redo(h, out) });
        assert_eq!(code, status::OK);
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            response["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["code"] == "wall-area-overlap"),
            "redo restores the warning along with the thickness, got {response}"
        );
        assert_eq!(domowik_debug_warnings_computations(h), base + 3);

        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// R7.1.6 through the gesture path: one `setWall` form carrying a length
    /// and both authoritative dimensions lands as ONE batch. `lengthMm` is
    /// resolved about the stored Start — End moves along the SAME direction —
    /// and one undo restores all three fields together.
    #[test]
    fn edit_set_wall_length_and_dimensions_are_one_batch() {
        let h = open_plan();
        let level = domowik_format::Level::new("Ground");
        let level_id = level.id;
        let req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Level(level))] }),
        )
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        // A 3-4-5 diagonal: a length edit must scale it, not re-aim it.
        let wall = domowik_format::Wall::new(
            level_id,
            domowik_format::Point::new(0.0, 0.0),
            domowik_format::Point::new(3000.0, 4000.0),
            200.0,
            2500.0,
        );
        let wall_id = wall.id;
        let req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Wall(wall))] }),
        )
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        let before = snapshot_json(h);
        let base = state_of(h)["undo_depth"].as_u64().unwrap();

        let (code, _) = edit_json(
            h,
            serde_json::json!({
                "kind": "setWall", "wallId": wall_id,
                "lengthMm": 10000.0, "thicknessMm": 150.0, "heightMm": 2700.0
            }),
        );
        assert_eq!(code, status::OK, "one form is one accepted command");
        assert_eq!(
            state_of(h)["undo_depth"].as_u64().unwrap(),
            base + 1,
            "one form mints exactly one undo entry"
        );

        let after = snapshot_json(h);
        let stored = after["walls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["id"] == wall_id.to_string())
            .expect("the edited wall");
        assert_eq!(stored["startMm"], serde_json::json!([0.0, 0.0]));
        let end = stored["endMm"].as_array().unwrap();
        assert!((end[0].as_f64().unwrap() - 6000.0).abs() < 1e-6);
        assert!((end[1].as_f64().unwrap() - 8000.0).abs() < 1e-6);
        assert_eq!(stored["thicknessMm"], 150.0);
        assert_eq!(stored["heightMm"], 2700.0);

        // A zero requested length would collapse the axis: the core refuses
        // the whole batch and the document stays exactly where it was.
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind": "setWall", "wallId": wall_id, "lengthMm": 0.0})
            )
            .0,
            status::REJECTED
        );
        assert_eq!(snapshot_json(h), after, "a refused form changes nothing");
        assert_eq!(state_of(h)["undo_depth"].as_u64().unwrap(), base + 1);

        let (code, _) = call_returning(|out| unsafe { domowik_plan_undo(h, out) });
        assert_eq!(code, status::OK);
        assert_eq!(
            snapshot_json(h),
            before,
            "one undo restores length, thickness and height together"
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn bulk_wall_dimensions_commit_undo_redo_and_refuse_atomically() {
        let (h, level) = plan_with_a_room();
        let walls = snapshot_json(h)["walls"].as_array().unwrap().clone();
        let ids = serde_json::json!([walls[1]["id"], walls[0]["id"], walls[1]["id"]]);
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({
                    "kind":"setWall", "wallId":walls[1]["id"], "heightMm":2800.0
                })
            )
            .0,
            status::OK
        );
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({
                    "kind":"addOpening", "hostWallId":walls[0]["id"], "opening":"door",
                    "u":0.5, "widthMm":900.0, "heightMm":2100.0
                })
            )
            .0,
            status::OK
        );
        let before = snapshot_json(h);
        let base = state_of(h)["undo_depth"].as_u64().unwrap();
        let request = serde_json::json!({"kind":"setWalls", "wallIds":ids,
            "thicknessMm":250.0, "heightMm":3000.0});
        assert_eq!(edit_json(h, request.clone()).0, status::OK);
        assert_eq!(state_of(h)["undo_depth"], base + 1);
        let after = snapshot_json(h);
        let mut expected = before.clone();
        for wall in expected["walls"].as_array_mut().unwrap() {
            if ids.as_array().unwrap().contains(&wall["id"]) {
                wall["heightMm"] = serde_json::json!(3000.0);
                wall["thicknessMm"] = serde_json::json!(250.0);
            }
        }
        assert_eq!(after, expected, "only selected wall dimensions change");
        let state = state_of(h);
        let (code, body) = edit_json(h, request);
        assert_eq!(code, status::OK);
        assert!(body.is_empty(), "unchanged bulk form has no batch");
        assert_eq!(state_of(h), state);
        assert_eq!(
            call_returning(|out| unsafe { domowik_plan_undo(h, out) }).0,
            status::OK
        );
        assert_eq!(snapshot_json(h), before);
        let state = state_of(h);
        for request in [
            serde_json::json!({"kind":"setWalls", "wallIds":ids, "heightMm":2000.0, "thicknessMm":250.0}),
            serde_json::json!({"kind":"setWalls", "wallIds":ids, "thicknessMm":100.0}),
            serde_json::json!({"kind":"setWalls", "wallIds":ids, "heightMm":0.0}),
            serde_json::json!({"kind":"setWalls", "wallIds":[walls[0]["id"], Uuid::nil()], "heightMm":3000.0}),
            serde_json::json!({"kind":"setWalls", "wallIds":[walls[0]["id"], level], "heightMm":3000.0}),
        ] {
            assert_eq!(edit_json(h, request).0, status::REJECTED);
            assert_eq!(snapshot_json(h), before, "no partial updates on refusal");
            assert_eq!(state_of(h), state, "both history stacks survive refusal");
        }
        // A no-change command must preserve an existing redo entry as well.
        assert_eq!(
            edit_json(h, serde_json::json!({"kind":"setWalls", "wallIds":ids})).0,
            status::OK
        );
        assert_eq!(state_of(h), state);
        assert_eq!(
            call_returning(|out| unsafe { domowik_plan_redo(h, out) }).0,
            status::OK
        );
        assert_eq!(snapshot_json(h), after);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// The gesture path must carry the specification's consequences, or the
    /// GUI is producing different documents from the CLI for the same action.
    #[test]
    fn a_drawn_wall_auto_joins_at_an_occupied_node() {
        let (h, level) = plan_with_a_room();
        // A wall leaving the corner at (4000, 3000), which two walls occupy.
        let req = serde_json::to_vec(&serde_json::json!({
            "kind": "addWall", "levelId": level,
            "start": [4000.0, 3000.0], "end": [4000.0, 6000.0],
            "thicknessMm": 200.0, "heightMm": 2500.0
        }))
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_edit(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        let (_, body) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let joins = doc["joins"].as_array().unwrap();
        let three = joins
            .iter()
            .any(|j| j["members"].as_array().unwrap().len() == 3);
        assert!(three, "the new wall joined the occupied node: {joins:#?}");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// R4.3.3 through the gesture path: a wall drawn across a hosted opening
    /// refuses the WHOLE edit, and nothing changes.
    #[test]
    fn a_gesture_that_would_straddle_an_opening_is_refused_atomically() {
        let (h, level) = plan_with_a_room();
        // A door centred on the first wall, so a crossing at its midpoint
        // straddles the aperture.
        let wall_id = {
            let (_, body) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
            let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
            doc["walls"][0]["id"].as_str().unwrap().to_owned()
        };
        let door_id = uuid::Uuid::now_v7();
        let door = serde_json::json!({
            "operations": [{
                "operationId": uuid::Uuid::now_v7(),
                "entityId": door_id,
                "kind": "createEntity",
                "entity": {
                    "type": "opening", "id": door_id, "kind": "door",
                    "hostWallId": wall_id, "u": 0.5, "widthMm": 900.0,
                    "heightMm": 2100.0, "sillMm": 0.0, "depthMm": 200.0,
                    "side": "Left", "hinge": "nearStart", "opensToward": "Left",
                    "swingDeg": 90.0
                }
            }]
        });
        let req = serde_json::to_vec(&door).unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK, "the door commits");

        let (_, before) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        let crossing = serde_json::to_vec(&serde_json::json!({
            "kind": "addWall", "levelId": level,
            "start": [2000.0, 0.0], "end": [2000.0, -3000.0],
            "thicknessMm": 200.0, "heightMm": 2500.0
        }))
        .unwrap();
        let mut buf = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        let code =
            unsafe { domowik_plan_edit(h, crossing.as_ptr(), crossing.len() as u64, &mut buf) };
        assert_eq!(code, status::REJECTED, "R4.3.3 refuses the whole edit");

        let (_, after) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        assert_eq!(before, after, "a refused gesture changes nothing");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// Adding a door through the gesture path: the depth must come from the
    /// HOST, or a door in a thin wall fails R4.0.4 validation.
    #[test]
    fn an_opening_gesture_takes_its_depth_from_the_host() {
        let (h, _) = plan_with_a_room();
        let wall_id = {
            let (_, body) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
            let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
            doc["walls"][0]["id"].as_str().unwrap().to_owned()
        };
        let req = serde_json::to_vec(&serde_json::json!({
            "kind": "addOpening", "hostWallId": wall_id, "opening": "door",
            "u": 0.5, "widthMm": 900.0, "heightMm": 2100.0
        }))
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_edit(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        let (_, body) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let o = &doc["openings"][0];
        assert_eq!(
            o["depthMm"].as_f64().unwrap(),
            200.0,
            "the wall's thickness"
        );
        assert_eq!(o["kind"], "door");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// An opening the core cannot fit is refused, and the plan is untouched.
    #[test]
    fn an_opening_wider_than_its_host_is_refused() {
        let (h, _) = plan_with_a_room();
        let wall_id = {
            let (_, body) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
            let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
            doc["walls"][0]["id"].as_str().unwrap().to_owned()
        };
        let (_, before) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        let req = serde_json::to_vec(&serde_json::json!({
            "kind": "addOpening", "hostWallId": wall_id, "opening": "door",
            "u": 0.5, "widthMm": 99000.0, "heightMm": 2100.0
        }))
        .unwrap();
        let mut buf = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        assert_eq!(
            unsafe { domowik_plan_edit(h, req.as_ptr(), req.len() as u64, &mut buf) },
            status::REJECTED
        );
        let (_, after) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        assert_eq!(before, after);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// R4.4 at the bridge: an addOpening naming an unknown kind or hinge is
    /// a refusal WITH its code (not a malformed request), a too-wide one
    /// reports `aperture-not-strict-fit`, and broken JSON stays malformed.
    #[test]
    fn opening_refusals_carry_their_r4_4_code() {
        let (h, _) = plan_with_a_room();
        let wall_id = {
            let (_, body) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
            let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
            doc["walls"][0]["id"].as_str().unwrap().to_owned()
        };
        let edit = |req: &[u8]| {
            let mut buf = DomowikBuf {
                data: std::ptr::null_mut(),
                len: 0,
                token: 0,
            };
            unsafe { domowik_plan_edit(h, req.as_ptr(), req.len() as u64, &mut buf) }
        };
        let last_error = || {
            let (plan, _lease) = resolve(h).unwrap();
            let e = plan.lock().unwrap().last_error.clone();
            e
        };
        let opening = |extra: serde_json::Value| {
            let mut v = serde_json::json!({
                "kind": "addOpening", "hostWallId": wall_id, "opening": "door",
                "u": 0.5, "widthMm": 900.0, "heightMm": 2100.0
            });
            for (k, x) in extra.as_object().unwrap() {
                v[k] = x.clone();
            }
            serde_json::to_vec(&v).unwrap()
        };
        for (extra, code) in [
            (
                serde_json::json!({"opening": "sliding"}),
                "invalid-opening-kind",
            ),
            (serde_json::json!({"hinge": "middle"}), "invalid-hinge"),
            (
                serde_json::json!({"opensToward": "Inward"}),
                "invalid-opens-toward",
            ),
            (
                serde_json::json!({"widthMm": 99000.0}),
                "aperture-not-strict-fit",
            ),
        ] {
            assert_eq!(edit(&opening(extra.clone())), status::REJECTED, "{extra}");
            assert!(
                last_error().starts_with(&format!("command rejected: {code}")),
                "{extra}: {}",
                last_error()
            );
        }
        assert_eq!(edit(b"{not json"), status::MALFORMED_REQUEST);
        // Codex GPT-6.1-Sol's cases: precedence across the request and the
        // candidate state; forbidden fields refused, not silently dropped;
        // an incomplete request is still malformed.
        let missing_host = opening(serde_json::json!({
            "opening": "sliding", "hostWallId": Uuid::now_v7().to_string()
        }));
        assert_eq!(edit(&missing_host), status::REJECTED);
        assert!(
            last_error().starts_with("command rejected: unknown-host"),
            "{}",
            last_error()
        );
        assert_eq!(
            edit(br#"{"kind":"addOpening","opening":"sliding"}"#),
            status::MALFORMED_REQUEST
        );
        assert_eq!(
            edit(&opening(serde_json::json!({}))),
            status::OK,
            "a valid door"
        );
        let door = {
            let (_, body) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
            let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
            doc["openings"][0]["id"].as_str().unwrap().to_owned()
        };
        let (_, before) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        for (req, code) in [
            (
                serde_json::json!({"kind": "setOpening", "openingId": door, "u": 1.25, "hinge": "middle"}),
                "invalid-opening-center",
            ),
            (
                serde_json::json!({"kind": "setOpening", "openingId": door, "hostWallId": wall_id, "u": 0.6}),
                "opening-direct-rehost",
            ),
            (
                serde_json::json!({"kind": "setOpening", "openingId": door, "levelId": Uuid::now_v7().to_string(), "u": 0.6}),
                "opening-level-immutable",
            ),
        ] {
            assert_eq!(
                edit(&serde_json::to_vec(&req).unwrap()),
                status::REJECTED,
                "{req}"
            );
            assert!(
                last_error().starts_with(&format!("command rejected: {code}")),
                "{req}: {}",
                last_error()
            );
        }
        let (_, after) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        assert_eq!(before, after, "refusals change nothing");

        // A raw execute creating an opening with an invalid kind.
        let host = Uuid::parse_str(&wall_id).unwrap();
        let mut raw = serde_json::to_value(Operation::create(Entity::Opening(
            domowik_format::Opening::door(host, 0.25, 600.0, 2100.0, 200.0),
        )))
        .unwrap();
        raw["entity"]["kind"] = serde_json::json!("sliding");
        let req = serde_json::to_vec(&serde_json::json!({ "operations": [raw] })).unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::REJECTED);
        assert!(
            last_error().contains("invalid-opening-kind"),
            "{}",
            last_error()
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn a_malformed_gesture_is_rejected_without_touching_the_plan() {
        let (h, _) = plan_with_a_room();
        let junk = b"{\"kind\":\"notAGesture\"}";
        let mut buf = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        assert_eq!(
            unsafe { domowik_plan_edit(h, junk.as_ptr(), junk.len() as u64, &mut buf) },
            status::MALFORMED_REQUEST
        );
        assert!(buf.data.is_null());
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// A coordinate survives save and reopen BIT FOR BIT across the FFI.
    ///
    /// This is the build behind the save and open path a GUI user's file
    /// actually travels. The witness is a double whose shortest decimal form
    /// re-parses one ULP low under serde_json's default parser:
    /// 0x3ff23d70a3d70a3e writes as 1.1400000000000001 and reads back as
    /// 0x3ff23d70a3d70a3d.
    ///
    /// It is deliberately an AWKWARD value rather than a plan-like one. Grid
    /// coordinates round-trip fine under either parser; what breaks is a
    /// full-mantissa value, which is what arithmetic produces — a snapped
    /// point, a split point — and then stores.
    ///
    /// The feature reaches this graph from EITHER Cargo.toml — measured, see
    /// the truth table beside the spike's own `serde_json` line — so this test
    /// guards the pair of them, not one.
    ///
    /// Asserted on BITS, not on approximate equality. An assertion with a
    /// tolerance passes happily while every coordinate in the document drifts,
    /// which is exactly the failure this exists to catch.
    #[test]
    fn a_coordinate_survives_save_and_reopen_bit_for_bit() {
        // Scaled into the coordinate domain while keeping an awkward mantissa.
        let awkward = f64::from_bits(0x3ff23d70a3d70a3e) * 1000.0;
        assert_ne!(
            awkward,
            f64::from_bits(awkward.to_bits() - 1),
            "the witness must be a real double"
        );

        let h = open_plan();
        let level = domowik_format::Level::new("Ground");
        let level_id = level.id;
        let op = Operation::create(Entity::Level(level));
        let req = serde_json::to_vec(&serde_json::json!({ "operations": [op] })).unwrap();
        assert_eq!(
            call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            })
            .0,
            status::OK
        );

        let wall = domowik_format::Wall::new(
            level_id,
            domowik_format::Point::new(awkward, 0.0),
            domowik_format::Point::new(awkward + 4000.0, 0.0),
            200.0,
            2500.0,
        );
        let wall_id = wall.id;
        let op = Operation::create(Entity::Wall(wall));
        let req = serde_json::to_vec(&serde_json::json!({ "operations": [op] })).unwrap();
        assert_eq!(
            call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            })
            .0,
            status::OK
        );

        let dir = std::env::temp_dir().join(format!("domowik-bits-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let file = dir.join("bits.domo");
        let path = std::ffi::CString::new(file.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert_eq!(domowik_plan_close(h), status::OK);

        let mut reopened = 0u64;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut reopened) },
            status::OK
        );
        let doc = snapshot_json(reopened);
        let found = doc["walls"]
            .as_array()
            .expect("walls")
            .iter()
            .find(|w| w["id"] == wall_id.to_string())
            .expect("the wall survived");
        let back = found["startMm"][0].as_f64().expect("a number");
        assert_eq!(
            back.to_bits(),
            awkward.to_bits(),
            "save and reopen moved the coordinate: wrote {:#x}, read {:#x}. \
             `float_roundtrip` is off in BOTH this workspace and the root \
             one — either alone is enough to fix it.",
            awkward.to_bits(),
            back.to_bits()
        );
        assert_eq!(domowik_plan_close(reopened), status::OK);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The feature is active in THIS graph, asserted directly.
    ///
    /// Separate from the end-to-end test above so a failure says which half
    /// broke: the parser configuration, or the save path around it.
    #[test]
    fn this_workspace_parses_floats_with_full_precision() {
        let witness = f64::from_bits(0x3ff23d70a3d70a3e);
        let text = serde_json::to_string(&witness).expect("serialises");
        let back: f64 = serde_json::from_str(&text).expect("parses");
        assert_eq!(
            back.to_bits(),
            witness.to_bits(),
            "this graph is parsing without `float_roundtrip` — check BOTH \
             Cargo.toml files, either one carries it; {text} came back as \
             {:#x}",
            back.to_bits()
        );
    }

    /// Save, close, reopen: an editor you cannot save from is not an editor.
    ///
    /// The reopened plan must carry the same walls AND the same derived area,
    /// which is the check that the log round-tripped rather than merely the
    /// snapshot.
    #[test]
    fn a_plan_saves_and_reopens_through_the_boundary() {
        let (h, level) = plan_with_a_room();
        let dir = std::env::temp_dir().join(format!("domowik-spike-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let file = dir.join("round-trip.domo");
        let path = std::ffi::CString::new(file.to_str().unwrap()).unwrap();

        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert!(file.exists(), "the file was written");

        let id = std::ffi::CString::new(level.to_string()).unwrap();
        let (_, before) =
            call_returning(|out| unsafe { domowik_plan_derived(h, id.as_ptr(), out) });
        assert_eq!(domowik_plan_close(h), status::OK);

        let mut reopened = 0u64;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut reopened) },
            status::OK
        );
        let (_, after) =
            call_returning(|out| unsafe { domowik_plan_derived(reopened, id.as_ptr(), out) });
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&before).unwrap(),
            serde_json::from_slice::<serde_json::Value>(&after).unwrap(),
            "the reopened plan derives the same geometry"
        );
        assert_eq!(domowik_plan_close(reopened), status::OK);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn encode_plan(h: u64) -> (i32, u64, Vec<u8>) {
        let mut revision = u64::MAX;
        let (code, bytes) =
            call_returning(|out| unsafe { domowik_plan_encode(h, &mut revision, out) });
        (code, revision, bytes)
    }

    #[test]
    fn encoded_bytes_reopen_as_the_saved_file_does() {
        let (h, level) = plan_with_a_room();
        let dir = std::env::temp_dir().join(format!("domowik-bytes-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("plan.domo");
        let (code, _, bytes) = encode_plan(h);
        assert_eq!(code, status::OK);
        let path = std::ffi::CString::new(file.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert_eq!(
            bytes,
            std::fs::read(&file).unwrap(),
            "one writer, two targets"
        );

        let mut reopened = 0u64;
        assert_eq!(
            unsafe { domowik_plan_open_bytes(bytes.as_ptr(), bytes.len() as u64, &mut reopened) },
            status::OK
        );
        let id = std::ffi::CString::new(level.to_string()).unwrap();
        let derived =
            |h| call_returning(|out| unsafe { domowik_plan_derived(h, id.as_ptr(), out) }).1;
        assert_eq!(derived(h), derived(reopened));
        assert_eq!(state_of(reopened)["dirty"], false);
        assert_eq!(
            state_of(reopened)["document_revision"],
            state_of(h)["document_revision"]
        );

        let mut none = 0u64;
        let garbage = b"not a package";
        assert_eq!(
            unsafe { domowik_plan_open_bytes(garbage.as_ptr(), garbage.len() as u64, &mut none) },
            status::REJECTED
        );
        assert_eq!(none, 0, "no handle on refusal");
        assert_eq!(
            unsafe { domowik_plan_open_bytes(std::ptr::null(), 4, &mut none) },
            status::INVALID_ARGUMENT
        );
        for h in [h, reopened] {
            assert_eq!(domowik_plan_close(h), status::OK);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_browser_save_clears_dirty_only_for_the_revision_it_stored() {
        let (h, level) = plan_with_a_room();
        assert_eq!(state_of(h)["dirty"], true);
        let (code, revision, _) = encode_plan(h);
        assert_eq!(code, status::OK);
        assert_eq!(state_of(h)["dirty"], true, "encoding is not saving");

        // An edit lands while the host is still writing the bytes.
        let (code, _) = edit_json(
            h,
            serde_json::json!({"kind":"addWall","levelId":level,"start":[0,5000],"end":[1000,5000],
                "thicknessMm":200,"heightMm":2500}),
        );
        assert_eq!(code, status::OK);
        assert_eq!(domowik_plan_mark_saved(h, revision), status::REJECTED);
        assert_eq!(state_of(h)["dirty"], true);

        let (_, revision, _) = encode_plan(h);
        let undo_depth = state_of(h)["undo_depth"].clone();
        assert_eq!(domowik_plan_mark_saved(h, revision), status::OK);
        assert_eq!(state_of(h)["dirty"], false);
        assert_eq!(
            state_of(h)["undo_depth"],
            undo_depth,
            "history survives a save"
        );
        assert_eq!(
            domowik_plan_mark_saved(u64::MAX, revision),
            status::UNKNOWN_HANDLE
        );

        // Undo after the save makes the plan dirty again and moves the revision.
        call_returning(|out| unsafe { domowik_plan_undo(h, out) });
        assert_eq!(state_of(h)["dirty"], true);
        assert_eq!(domowik_plan_mark_saved(h, revision), status::REJECTED);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn an_empty_builder_cannot_be_encoded() {
        let h = open_plan();
        let (code, revision, _) = encode_plan(h);
        assert_eq!(code, status::REJECTED);
        assert_eq!(revision, u64::MAX, "nothing is written on refusal");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn opening_a_file_that_is_not_there_is_refused_without_a_handle() {
        let path = std::ffi::CString::new("/nonexistent/definitely/not/here.domo").unwrap();
        let mut handle = 12345u64;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut handle) },
            status::REJECTED
        );
        assert_eq!(handle, 12345, "a refused open writes no handle");
        let mut h2 = 0u64;
        assert_eq!(
            unsafe { domowik_plan_open(std::ptr::null(), &mut h2) },
            status::INVALID_ARGUMENT
        );
    }

    /// The derived pass must cross the boundary, because an editor that drew
    /// only the stored centre lines would be drawing sticks.
    #[test]
    fn derived_geometry_crosses_the_boundary_with_rooms_and_area() {
        let (h, level) = plan_with_a_room();
        let id = std::ffi::CString::new(level.to_string()).unwrap();
        let (code, body) =
            call_returning(|out| unsafe { domowik_plan_derived(h, id.as_ptr(), out) });
        assert_eq!(code, status::OK);
        let v: serde_json::Value = serde_json::from_slice(&body).expect("json");

        assert_eq!(v["levelId"], level.to_string());
        assert_eq!(v["occupied"].as_array().unwrap().len(), 1, "one shell");
        let rooms = v["rooms"].as_array().unwrap();
        assert_eq!(rooms.len(), 1, "one enclosed face");

        // Measured to the INNER faces: 3800 x 2800, not the 4000 x 3000 the
        // centre lines enclose. That difference is the whole reason the core
        // computes this and the UI does not.
        let area = v["floorAreaMm2"].as_f64().unwrap();
        assert!(
            (area - 3800.0 * 2800.0).abs() < 1.0,
            "floor area was {area}, expected {}",
            3800.0 * 2800.0
        );
        assert!((rooms[0]["areaMm2"].as_f64().unwrap() - area).abs() < 1.0);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// The three room gestures, the boundary they keep, and the derived data
    /// an editor needs to draw a named room.
    ///
    /// ED-0101 is what this is really testing: the front end says what the
    /// user is trying to do and the CORE decides what it means. Candidacy, the
    /// lowest-UUID tie-break and every status are computed behind the bridge
    /// and arrive as derived data; the editor is never in a position to invent
    /// a different answer.
    #[test]
    fn the_three_room_gestures_cross_the_boundary_and_the_core_decides_status() {
        let (h, level) = plan_with_a_room();
        let derived = |h: u64| -> serde_json::Value {
            let id = std::ffi::CString::new(level.to_string()).unwrap();
            let (code, body) =
                call_returning(|out| unsafe { domowik_plan_derived(h, id.as_ptr(), out) });
            assert_eq!(code, status::OK);
            serde_json::from_slice(&body).expect("json")
        };

        // A face with no record carries no label at all (R5.3.3 absence).
        let v = derived(h);
        assert!(v["rooms"][0]["label"].is_null());
        assert!(v["rooms"][0]["recordId"].is_null());
        assert_eq!(v["roomRecords"].as_array().unwrap().len(), 0);

        // addRoom, with a supplied identifier so the collision below is
        // reachable.
        let first = uuid::Uuid::from_u128(1);
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"addRoom", "levelId":level.to_string(),
                    "seed":[2000.0, 1500.0], "id":first.to_string(), "name":"Salon"})
            )
            .0,
            status::OK
        );
        let v = derived(h);
        assert_eq!(v["rooms"][0]["label"], "Salon");
        assert_eq!(v["rooms"][0]["recordId"], first.to_string());
        assert_eq!(v["rooms"][0]["displayAreaM2"], "10.64");
        assert_eq!(v["roomRecords"][0]["status"], "associated");
        assert_eq!(
            v["roomRecords"][0]["seedMm"],
            serde_json::json!([2000.0, 1500.0])
        );

        // A supplied id already in use rejects; the state does not move.
        let before = snapshot_json(h);
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"addRoom", "levelId":level.to_string(),
                    "seed":[2500.0, 1500.0], "id":first.to_string()})
            )
            .0,
            status::REJECTED
        );
        assert_eq!(
            snapshot_json(h),
            before,
            "a refused gesture changes nothing"
        );

        // A second seed in the same face loses the tie-break, and the CORE
        // says so — the editor is told, not asked.
        let second = uuid::Uuid::from_u128(2);
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"addRoom", "levelId":level.to_string(),
                    "seed":[3000.0, 1500.0], "id":second.to_string(), "name":"Study"})
            )
            .0,
            status::OK
        );
        let v = derived(h);
        assert_eq!(
            v["rooms"][0]["label"], "Salon",
            "the lower UUID keeps the label"
        );
        assert_eq!(v["roomRecords"][0]["status"], "associated");
        assert_eq!(v["roomRecords"][1]["status"], "superseded");

        // setRoom renames, and an explicit null CLEARS rather than leaving
        // alone — the distinction F37d makes, carried across the wire.
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"setRoom", "roomId":first.to_string(),
                    "name":"Kuchnia"})
            )
            .0,
            status::OK
        );
        assert_eq!(derived(h)["rooms"][0]["label"], "Kuchnia");
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"setRoom", "roomId":first.to_string(),
                    "name":serde_json::Value::Null})
            )
            .0,
            status::OK
        );
        let v = derived(h);
        assert!(v["roomRecords"][0]["name"].is_null(), "cleared to absent");
        assert_eq!(v["rooms"][0]["label"], "Room", "and displays the fallback");

        // A seed moved outside every face keeps the record and changes only
        // its status, which is pure projection.
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"setRoom", "roomId":first.to_string(),
                    "seed":[20000.0, 1500.0]})
            )
            .0,
            status::OK
        );
        let v = derived(h);
        assert_eq!(v["roomRecords"][0]["status"], "seed-in-exterior");
        assert_eq!(
            v["rooms"][0]["label"], "Study",
            "the superseded record takes the face over as projection"
        );

        // deleteRoom removes the record; the FACE is untouched (R5.1.8).
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"deleteRoom", "roomId":first.to_string()})
            )
            .0,
            status::OK
        );
        let v = derived(h);
        assert_eq!(v["roomRecords"].as_array().unwrap().len(), 1);
        assert_eq!(
            v["rooms"].as_array().unwrap().len(),
            1,
            "the room is still there"
        );

        // An unknown target rejects deterministically.
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"deleteRoom",
                    "roomId":uuid::Uuid::now_v7().to_string()})
            )
            .0,
            status::REJECTED
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// An absent field and an explicit `null` are different requests, and the
    /// wire form has to keep them apart or the editor cannot express both.
    #[test]
    fn an_absent_set_room_field_leaves_it_alone_and_null_clears_it() {
        let (h, level) = plan_with_a_room();
        let id = uuid::Uuid::from_u128(1);
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"addRoom", "levelId":level.to_string(),
                    "seed":[2000.0, 1500.0], "id":id.to_string(), "name":"Salon",
                    "floorStyleRef":"F-wood"})
            )
            .0,
            status::OK
        );
        // Mentioning only the seed must not disturb the name or the style.
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"setRoom", "roomId":id.to_string(),
                    "seed":[2500.0, 1500.0]})
            )
            .0,
            status::OK
        );
        let doc = snapshot_json(h);
        assert_eq!(doc["roomRecords"][0]["name"], "Salon");
        assert_eq!(doc["roomRecords"][0]["floorStyleRef"], "F-wood");

        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"setRoom", "roomId":id.to_string(),
                    "floorStyleRef":serde_json::Value::Null})
            )
            .0,
            status::OK
        );
        let doc = snapshot_json(h);
        assert_eq!(doc["roomRecords"][0]["name"], "Salon", "untouched");
        assert!(doc["roomRecords"][0]["floorStyleRef"].is_null(), "cleared");

        // An empty-string style reference rejects; clearing uses null.
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"setRoom", "roomId":id.to_string(),
                    "ceilingStyleRef":""})
            )
            .0,
            status::REJECTED
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// An empty level's floor area must be +0.0, not -0.0. Rust's `Sum for
    /// f64` folds from negative zero, and a UI formatting that to two places
    /// renders "-0.00 m²".
    #[test]
    fn an_empty_levels_floor_area_is_positive_zero() {
        let h = open_plan();
        let id = std::ffi::CString::new(uuid::Uuid::now_v7().to_string()).unwrap();
        let (code, body) =
            call_returning(|out| unsafe { domowik_plan_derived(h, id.as_ptr(), out) });
        assert_eq!(code, status::OK);
        assert!(
            !String::from_utf8_lossy(&body).contains("-0.0"),
            "negative zero leaked into the response: {}",
            String::from_utf8_lossy(&body)
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// The derived pass must carry APERTURES too, or an editor cannot draw a
    /// door it just placed — and reconstructing the cut in the view would be a
    /// second implementation of R4.2.1.
    #[test]
    fn derived_geometry_carries_apertures_and_leaves() {
        let (h, level) = plan_with_a_room();
        let wall_id = {
            let (_, body) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
            let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
            doc["walls"][0]["id"].as_str().unwrap().to_owned()
        };
        let req = serde_json::to_vec(&serde_json::json!({
            "kind": "addOpening", "hostWallId": wall_id, "opening": "door",
            "u": 0.5, "widthMm": 900.0, "heightMm": 2100.0
        }))
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_edit(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        let id = std::ffi::CString::new(level.to_string()).unwrap();
        let (_, body) = call_returning(|out| unsafe { domowik_plan_derived(h, id.as_ptr(), out) });
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let openings = v["openings"].as_array().unwrap();
        assert_eq!(openings.len(), 1);
        let o = &openings[0];
        assert_eq!(o["kind"], "door");
        assert_eq!(o["cut"].as_array().unwrap().len(), 4, "an aperture quad");
        let leaf = o["leaf"].as_array().unwrap();
        assert_eq!(leaf.len(), 2, "hinge and tip");
        // A 90-degree leaf must not lie along the wall.
        let hinge = leaf[0].as_array().unwrap();
        let tip = leaf[1].as_array().unwrap();
        assert!(
            (tip[1].as_f64().unwrap() - hinge[1].as_f64().unwrap()).abs() > 1.0,
            "the leaf leaves the wall plane: {leaf:?}"
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// Host-dependent, opt-in measurement of the public derived-glyph path.
    /// Fixture setup is outside the measured loop; no timing is a pass gate.
    #[test]
    #[ignore = "host-dependent geometry and FFI timing"]
    fn opening_mechanism_glyph_benchmark() {
        let (h, level) = plan_with_a_room();
        let (_, body) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let wall = doc["walls"][0]["id"].as_str().unwrap();
        for (u, mechanism) in [
            (0.2, "windowSideHung"),
            (0.5, "windowDoubleSwing"),
            (0.8, "windowLift"),
        ] {
            let req = serde_json::to_vec(&serde_json::json!({
                "kind": "addOpening", "hostWallId": wall, "opening": "window",
                "u": u, "widthMm": 400.0, "heightMm": 1200.0,
                "sillMm": 900.0, "mechanism": mechanism,
            }))
            .unwrap();
            let (code, _) = call_returning(|out| unsafe {
                domowik_plan_edit(h, req.as_ptr(), req.len() as u64, out)
            });
            assert_eq!(code, status::OK);
        }
        let level = std::ffi::CString::new(level.to_string()).unwrap();
        let started = std::time::Instant::now();
        for _ in 0..200 {
            let (code, bytes) =
                call_returning(|out| unsafe { domowik_plan_derived(h, level.as_ptr(), out) });
            assert_eq!(code, status::OK);
            std::hint::black_box(bytes);
        }
        eprintln!(
            "mechanism glyphs: {} ns/derived call (200 runs)",
            started.elapsed().as_nanos() / 200
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn derived_geometry_of_an_unknown_level_is_empty_rather_than_an_error() {
        let (h, _) = plan_with_a_room();
        let id = std::ffi::CString::new(uuid::Uuid::now_v7().to_string()).unwrap();
        let (code, body) =
            call_returning(|out| unsafe { domowik_plan_derived(h, id.as_ptr(), out) });
        assert_eq!(code, status::OK, "an empty level is not a failure");
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(v["occupied"].as_array().unwrap().is_empty());
        assert!(v["rooms"].as_array().unwrap().is_empty());
        assert_eq!(v["floorAreaMm2"].as_f64().unwrap(), 0.0);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn derived_geometry_refuses_a_forged_handle_and_a_bad_level_id() {
        let mut buf = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        let id = std::ffi::CString::new(uuid::Uuid::now_v7().to_string()).unwrap();
        assert_eq!(
            unsafe { domowik_plan_derived(0xdead_beef, id.as_ptr(), &mut buf) },
            status::UNKNOWN_HANDLE
        );
        let (h, _) = plan_with_a_room();
        let bad = std::ffi::CString::new("not-a-uuid").unwrap();
        assert_eq!(
            unsafe { domowik_plan_derived(h, bad.as_ptr(), &mut buf) },
            status::INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { domowik_plan_derived(h, std::ptr::null(), &mut buf) },
            status::INVALID_ARGUMENT
        );
        assert!(buf.data.is_null(), "a refused call transfers no buffer");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn a_command_executes_and_the_document_reflects_it() {
        let h = open_plan();
        let req = create_level_request();
        let (code, body) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        let v: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(v["entities"], 1);
        assert_eq!(v["undo_depth"], 1);
        assert!(!v["batch_id"].as_str().expect("id").is_empty());

        let (code, doc) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        assert_eq!(code, status::OK);
        let doc: serde_json::Value = serde_json::from_slice(&doc).expect("json");
        assert_eq!(doc["levels"][0]["name"], "Ground");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn undo_is_stateful_across_calls() {
        // The seam has to be STATEFUL to be worth a handle at all: a bridge
        // that rebuilt the document per call would pass every other test here.
        let mut h = 0;
        assert_eq!(unsafe { domowik_plan_create_default(&mut h) }, status::OK);
        let req = create_level_request();
        call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        let (code, body) = call_returning(|out| unsafe { domowik_plan_undo(h, out) });
        assert_eq!(code, status::OK);
        let v: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(
            v["entities"], 1,
            "undo removed the added level, not the baseline"
        );
        assert_eq!(v["redo_depth"], 1);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn undo_of_a_legacy_first_level_creation_is_refused_atomically() {
        let h = open_plan();
        let req = create_level_request();
        assert_eq!(
            call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            })
            .0,
            status::OK
        );
        let before = state_of(h);
        let (code, body) = call_returning(|out| unsafe { domowik_plan_undo(h, out) });
        assert_eq!(code, status::REJECTED);
        assert!(body.is_empty());
        assert_eq!(state_of(h), before);
        let (code, error) = call_returning(|out| unsafe { domowik_last_error(h, out) });
        assert_eq!(code, status::OK);
        assert!(String::from_utf8(error).unwrap().contains("only level"));
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn empty_materialized_files_are_inspectable_but_not_editor_openable() {
        let dir = std::env::temp_dir().join(format!("domowik-empty-{}", Uuid::now_v7()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("legacy-empty.domo");
        domowik_container::write_package(
            &path,
            &domowik_container::Package::new(Document::default()),
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(ContainerPlan::open(&path)
            .unwrap()
            .document()
            .levels
            .is_empty());
        let native = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        let mut sentinel = 42;
        assert_eq!(
            unsafe { domowik_plan_open(native.as_ptr(), &mut sentinel) },
            status::REJECTED
        );
        assert_eq!(sentinel, 42, "refusal must not transfer a handle");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let h = open_plan();
        assert_eq!(
            unsafe { domowik_plan_save(h, native.as_ptr()) },
            status::REJECTED
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn state_of(h: u64) -> serde_json::Value {
        let (code, body) = call_returning(|out| unsafe { domowik_plan_state(h, out) });
        assert_eq!(code, status::OK);
        serde_json::from_slice(&body).unwrap()
    }

    fn edit_json(h: u64, request: serde_json::Value) -> (i32, Vec<u8>) {
        let bytes = serde_json::to_vec(&request).unwrap();
        call_returning(|out| unsafe {
            domowik_plan_edit(h, bytes.as_ptr(), bytes.len() as u64, out)
        })
    }

    fn snapshot_json(h: u64) -> serde_json::Value {
        let (code, bytes) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        assert_eq!(code, status::OK);
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn wall_contact_refusals_preserve_native_state_snapshot_history_and_saved_bytes() {
        let mut h = 0;
        assert_eq!(unsafe { domowik_plan_create_default(&mut h) }, status::OK);
        let level = snapshot_json(h)["levels"][0]["id"].clone();
        for (start, end) in [
            ([0.0, 0.0], [4000.0, 0.0]),
            ([2000.0, 1000.0], [2000.0, 3000.0]),
        ] {
            assert_eq!(
                edit_json(
                    h,
                    serde_json::json!({"kind":"addWall", "levelId":level,
                "start":start, "end":end, "thicknessMm":200.0})
                )
                .0,
                status::OK
            );
        }
        let snapshot = snapshot_json(h);
        let moving = snapshot["walls"][1]["id"].clone();
        let dir = std::env::temp_dir().join(format!("domowik-contact-{}", Uuid::now_v7()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("plan.domo");
        let native = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, native.as_ptr()) }, status::OK);
        let saved = std::fs::read(&path).unwrap();
        let state = state_of(h);
        for (request, diagnostic) in [
            (
                serde_json::json!({"kind":"addWall", "levelId":level, "start":[2000.0,0.0], "end":[6000.0,0.0], "thicknessMm":200.0}),
                "collinear-overlap",
            ),
            (
                serde_json::json!({"kind":"addWall", "levelId":level, "start":[4000.0,0.0], "end":[0.0,0.0], "thicknessMm":200.0}),
                "wall-coincidence",
            ),
            (
                serde_json::json!({"kind":"moveWall", "wallId":moving, "start":[0.0,0.0], "end":[4000.0,0.0]}),
                "wall-coincidence",
            ),
        ] {
            assert_eq!(edit_json(h, request).0, status::REJECTED);
            let (code, error) = call_returning(|out| unsafe { domowik_last_error(h, out) });
            assert_eq!(code, status::OK);
            assert!(String::from_utf8_lossy(&error).contains(diagnostic));
            assert_eq!(snapshot_json(h), snapshot);
            assert_eq!(
                state_of(h),
                state,
                "dirty, revision and history did not change"
            );
            assert_eq!(std::fs::read(&path).unwrap(), saved);
        }
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"moveWall", "wallId":moving,
            "start":[2000.0,-1000.0], "end":[2000.0,1000.0]})
            )
            .0,
            status::OK
        );
        assert_eq!(snapshot_json(h)["walls"].as_array().unwrap().len(), 4);
        assert_eq!(
            call_returning(|out| unsafe { domowik_plan_undo(h, out) }).0,
            status::OK
        );
        assert_eq!(
            snapshot_json(h),
            snapshot,
            "one undo reverses the move and all incidental splits/joins"
        );
        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// ED-0119 through the GUI's own path: a wall drawn exactly through an
    /// existing crossing at a non-binary64 point joins it rather than being
    /// refused, and one undo removes it with its split.
    #[test]
    fn a_wall_drawn_through_an_existing_crossing_joins_it() {
        let mut h = 0;
        assert_eq!(unsafe { domowik_plan_create_default(&mut h) }, status::OK);
        let level = snapshot_json(h)["levels"][0]["id"].clone();
        let add = |start: [f64; 2], end: [f64; 2]| {
            edit_json(
                h,
                serde_json::json!({"kind":"addWall", "levelId":level,
                    "start":start, "end":end, "thicknessMm":120.0}),
            )
            .0
        };
        assert_eq!(add([0.0, 0.0], [3000.0, 1000.0]), status::OK);
        assert_eq!(add([0.0, 700.0], [3000.0, 0.0]), status::OK);
        let before = snapshot_json(h);
        assert_eq!(add([0.0, 1000.0], [21000.0, -9000.0]), status::OK);
        let after = snapshot_json(h);
        assert_eq!(after["walls"].as_array().unwrap().len(), 6);
        assert!(after["joins"]
            .as_array()
            .unwrap()
            .iter()
            .any(|j| j["members"].as_array().unwrap().len() == 6));
        assert_eq!(
            call_returning(|out| unsafe { domowik_plan_undo(h, out) }).0,
            status::OK
        );
        assert_eq!(snapshot_json(h), before);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn level_fields_and_warnings_cross_history_save_and_open() {
        let mut h = 0;
        assert_eq!(unsafe { domowik_plan_create_default(&mut h) }, status::OK);
        let original = snapshot_json(h);
        let id = original["levels"][0]["id"].clone();
        let request = serde_json::json!({"kind":"setLevel", "levelId":id,
            "name":" Cellar ", "elevationMm":-3000.0, "floorThicknessMm":0.0,
            "defaultWallHeightMm":3300.0, "defaultCeilingHeightMm":3000.0});
        assert_eq!(edit_json(h, request.clone()).0, status::OK);
        let changed = snapshot_json(h);
        let state = state_of(h);
        assert_eq!(state["undo_depth"], 1);
        assert_eq!(state["document_revision"], 1);
        assert_eq!(state["warnings"][0]["code"], "ceiling-below-wall-height");
        assert_eq!(state["warnings"][0]["level"], id);
        assert!(state["warnings"][0]["message"].is_string());
        assert_eq!(changed["levels"][0]["name"], " Cellar ");
        assert_eq!(changed["levels"][0]["elevationMm"], -3000.0);
        assert_eq!(changed["levels"][0]["floorThicknessMm"], 0.0);
        assert_eq!(changed["levels"][0]["defaultWallHeightMm"], 3300.0);
        assert_eq!(changed["levels"][0]["defaultCeilingHeightMm"], 3000.0);
        let (code, bytes) = edit_json(h, request);
        assert_eq!(code, status::OK);
        assert!(bytes.is_empty(), "unchanged fields return no batch id");
        assert_eq!(state_of(h), state);
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"setLevel", "levelId":id,
            "name":"must not land", "floorThicknessMm":-1.0})
            )
            .0,
            status::REJECTED
        );
        assert_eq!(snapshot_json(h), changed);
        assert_eq!(state_of(h), state);
        assert_eq!(
            call_returning(|out| unsafe { domowik_plan_undo(h, out) }).0,
            status::OK
        );
        assert_eq!(snapshot_json(h), original);
        assert_eq!(state_of(h)["warnings"], serde_json::json!([]));
        assert_eq!(
            call_returning(|out| unsafe { domowik_plan_redo(h, out) }).0,
            status::OK
        );
        assert_eq!(snapshot_json(h), changed);
        assert_eq!(state_of(h)["warnings"], state["warnings"]);
        let dir = std::env::temp_dir().join(format!("domowik-level-fields-{}", Uuid::now_v7()));
        std::fs::create_dir(&dir).unwrap();
        let path = std::ffi::CString::new(dir.join("plan.domo").to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert_eq!(state_of(h)["warnings"], state["warnings"]);
        let mut reopened = 0;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut reopened) },
            status::OK
        );
        assert_eq!(snapshot_json(reopened), changed);
        assert_eq!(state_of(reopened)["warnings"], state["warnings"]);
        assert_eq!(state_of(reopened)["dirty"], false);
        assert_eq!(state_of(reopened)["undo_depth"], 0);
        assert_eq!(domowik_plan_close(reopened), status::OK);
        assert_eq!(domowik_plan_close(h), status::OK);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn omitted_wall_height_resolves_once_from_the_current_level() {
        let mut h = 0;
        assert_eq!(unsafe { domowik_plan_create_default(&mut h) }, status::OK);
        let id = snapshot_json(h)["levels"][0]["id"].clone();
        let wall = |y| {
            serde_json::json!({"kind":"addWall", "levelId":id,
            "start":[0.0,y], "end":[4000.0,y], "thicknessMm":120.0})
        };
        assert_eq!(edit_json(h, wall(0.0)).0, status::OK);
        assert_eq!(
            edit_json(
                h,
                serde_json::json!({"kind":"setLevel", "levelId":id,
            "defaultWallHeightMm":3100.0})
            )
            .0,
            status::OK
        );
        assert_eq!(edit_json(h, wall(2000.0)).0, status::OK);
        let mut explicit = wall(4000.0);
        explicit["heightMm"] = serde_json::json!(1700.0);
        assert_eq!(edit_json(h, explicit).0, status::OK);
        let snapshot = snapshot_json(h);
        let heights: Vec<_> = snapshot["walls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|wall| wall["heightMm"].as_f64().unwrap())
            .collect();
        assert_eq!(heights, vec![2500.0, 3100.0, 1700.0]);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// Issue 0115: the reorder gesture stores the document-carried list
    /// order, moves no elevation, is idempotent, undoes as one list, and
    /// refuses partial or unknown lists without committing.
    #[test]
    fn the_order_levels_gesture_controls_only_the_list_order() {
        let h = open_plan();
        let ground = domowik_format::Level::new("Ground");
        let ground_id = ground.id;
        let mut basement = domowik_format::Level::new("Basement");
        basement.elevation_mm = -2600.0;
        let basement_id = basement.id;
        for level in [ground, basement] {
            let req = serde_json::to_vec(
                &serde_json::json!({"operations": [Operation::create(Entity::Level(level))]}),
            )
            .unwrap();
            assert_eq!(
                call_returning(|out| unsafe {
                    domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
                })
                .0,
                status::OK
            );
        }

        // Top of the list first: Basement, then Ground. A changed list
        // commits one batch the caller can parse.
        let reorder = serde_json::json!({"kind":"orderLevels",
            "levelIds":[basement_id, ground_id]});
        let (code, bytes) = edit_json(h, reorder.clone());
        assert_eq!(code, status::OK);
        assert!(!bytes.is_empty(), "a changed list commits a batch");

        let doc = snapshot_json(h);
        let level = |doc: &serde_json::Value, id| {
            doc["levels"]
                .as_array()
                .unwrap()
                .iter()
                .find(|l| l["id"] == serde_json::json!(id))
                .unwrap()
                .clone()
        };
        assert_eq!(level(&doc, basement_id)["displayOrder"], 0);
        assert_eq!(level(&doc, ground_id)["displayOrder"], 1);
        // The list moved; the geometry did not.
        assert_eq!(level(&doc, basement_id)["elevationMm"], -2600.0);
        assert_eq!(level(&doc, ground_id)["elevationMm"], 0.0);
        assert_eq!(level(&doc, basement_id)["name"], "Basement");
        assert_eq!(level(&doc, ground_id)["name"], "Ground");

        // An unchanged list is not a document command: accepted, empty.
        let (code, bytes) = edit_json(h, reorder.clone());
        assert_eq!(code, status::OK);
        assert!(bytes.is_empty(), "the same order commits nothing");
        assert_eq!(snapshot_json(h), doc);

        // A partial or unknown list is refused without committing anything.
        for bad in [
            serde_json::json!({"kind":"orderLevels","levelIds":[ground_id]}),
            serde_json::json!({"kind":"orderLevels",
                "levelIds":[basement_id, ground_id, Uuid::now_v7()]}),
        ] {
            assert_eq!(edit_json(h, bad).0, status::REJECTED);
        }
        assert_eq!(snapshot_json(h), doc);

        // One undo compensates the WHOLE list.
        assert_eq!(
            call_returning(|out| unsafe { domowik_plan_undo(h, out) }).0,
            status::OK
        );
        let undone = snapshot_json(h);
        assert!(level(&undone, basement_id).get("displayOrder").is_none());
        assert!(level(&undone, ground_id).get("displayOrder").is_none());
        assert_eq!(undone["levels"].as_array().unwrap().len(), 2);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// Issue 0116: the plan snapshot carries the stable document identity.
    /// The editor keys per-device state (issue 0117) on exactly this reply —
    /// it must not need a second call to learn who a document is.
    #[test]
    fn the_plan_snapshot_names_the_document_identity() {
        let mut h = 0;
        assert_eq!(unsafe { domowik_plan_create_default(&mut h) }, status::OK);
        let identity = snapshot_json(h)["documentId"]
            .as_str()
            .expect("a baseline plan carries the identity it was minted with")
            .to_owned();

        // An accepted document command and a save do not regenerate it: the
        // same key survives reopen (BEHAVIOR-SPEC §2.6).
        let req = serde_json::to_vec(&serde_json::json!({
            "operations": [Operation::create(Entity::Level(
                domowik_format::Level::new("Second")),
            )],
        }))
        .unwrap();
        assert_eq!(
            call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            })
            .0,
            status::OK
        );
        assert_eq!(snapshot_json(h)["documentId"], serde_json::json!(identity));
        let dir = std::env::temp_dir().join(format!("domowik-identity-{}", Uuid::now_v7()));
        std::fs::create_dir(&dir).unwrap();
        let path = std::ffi::CString::new(dir.join("plan.domo").to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert_eq!(domowik_plan_close(h), status::OK);
        let mut reopened = 0;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut reopened) },
            status::OK
        );
        let reopened_snapshot = snapshot_json(reopened);
        let changed = reopened_snapshot["documentId"]
            .as_str()
            .expect("identity is carried by the snapshot envelope");
        assert_eq!(changed, identity);
        assert_eq!(domowik_plan_close(reopened), status::OK);
    }

    #[test]
    fn editor_baseline_has_no_history_and_empty_history_is_clean() {
        let mut h = 0;
        assert_eq!(unsafe { domowik_plan_create_default(&mut h) }, status::OK);
        let before = state_of(h);
        assert_eq!(before["entities"], 1);
        assert_eq!(before["undo_depth"], 0);
        assert_eq!(before["redo_depth"], 0);
        assert_eq!(before["dirty"], false);
        let (code, bytes) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        assert_eq!(code, status::OK);
        let doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(doc["levels"][0]["name"], "Level 1");
        assert_eq!(doc["levels"][0]["elevationMm"], 0.0);
        // `plan_state` carries a `document_revision` absent from history
        // responses; both it and every mutation response (execute/edit/undo/
        // redo) carry `warnings` now, so only the two revisions are stripped before
        // comparing the two as whole objects.
        let mut summary = before.clone();
        summary
            .as_object_mut()
            .expect("object")
            .remove("document_revision")
            .expect("plan_state reports the revision");
        summary
            .as_object_mut()
            .expect("object")
            .remove("asset_revision")
            .expect("plan_state reports the asset revision");
        assert_eq!(summary["warnings"], serde_json::json!([]));
        for redo in [false, true] {
            let (code, body) = call_returning(|out| unsafe { change_history(h, out, redo) });
            assert_eq!(code, status::OK);
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
                summary
            );
        }
        assert_eq!(state_of(h), before, "an empty stack changed nothing at all");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn saved_and_opened_plans_report_the_live_session_and_redo_survives_save() {
        let mut h = 0;
        assert_eq!(unsafe { domowik_plan_create_default(&mut h) }, status::OK);
        let req = create_level_request();
        let execute = |h| {
            call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            })
        };
        assert_eq!(execute(h).0, status::OK);
        assert_eq!(
            call_returning(|out| unsafe { domowik_plan_undo(h, out) }).0,
            status::OK
        );
        let dir = std::env::temp_dir().join(format!("domowik-history-{}", Uuid::now_v7()));
        std::fs::create_dir(&dir).unwrap();
        let path = std::ffi::CString::new(dir.join("plan.domo").to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert_eq!(state_of(h)["dirty"], false);
        assert_eq!(state_of(h)["redo_depth"], 1);
        let (code, body) = call_returning(|out| unsafe { domowik_plan_redo(h, out) });
        assert_eq!(code, status::OK);
        let after: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(after["entities"], 2);
        assert_eq!(after["undo_depth"], 1);
        assert_eq!(after["dirty"], true);
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert_eq!(domowik_plan_close(h), status::OK);
        let mut reopened = 0;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut reopened) },
            status::OK
        );
        let state = state_of(reopened);
        assert_eq!(state["entities"], 2);
        assert_eq!(state["undo_depth"], 0);
        assert_eq!(state["redo_depth"], 0);
        assert_eq!(state["dirty"], false);
        let next = create_level_request();
        let (code, body) = call_returning(|out| unsafe {
            domowik_plan_execute(reopened, next.as_ptr(), next.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        let state: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(state["entities"], 3);
        assert_eq!(state["undo_depth"], 1);
        assert_eq!(state["dirty"], true);
        assert_eq!(domowik_plan_close(reopened), status::OK);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_forged_handle_is_detected_rather_than_dereferenced() {
        // The reason handles are registry tokens and not `Box::into_raw`
        // pointers. With pointers this test is undefined behaviour; here it is
        // a return value.
        let mut buf = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        for forged in [0u64, 1 << 40, u64::MAX, 0xdead_beef] {
            assert_eq!(
                unsafe { domowik_plan_snapshot(forged, &mut buf) },
                status::UNKNOWN_HANDLE,
                "forged handle {forged} was not rejected"
            );
        }
    }

    #[test]
    fn a_closed_handle_stays_unknown_and_is_never_reissued() {
        let h = open_plan();
        assert_eq!(domowik_plan_close(h), status::OK);
        assert_eq!(domowik_plan_close(h), status::UNKNOWN_HANDLE);

        // A later plan must not receive the same token, or a stale handle
        // would silently address someone else's document.
        let h2 = open_plan();
        assert_ne!(h, h2);
        assert_eq!(domowik_plan_close(h2), status::OK);
    }

    #[test]
    fn a_double_free_is_refused_rather_than_performed() {
        let h = open_plan();
        let mut buf = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        assert_eq!(unsafe { domowik_plan_snapshot(h, &mut buf) }, status::OK);
        assert_eq!(domowik_buffer_free(buf.token), status::OK);
        // The second free must NOT reach the allocator.
        assert_eq!(domowik_buffer_free(buf.token), status::UNKNOWN_BUFFER);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn a_forged_or_stale_token_is_refused() {
        // The free path takes no pointer at all, so there is no pointer for a
        // caller to get wrong: an arbitrary integer is refused, not acted on.
        for forged in [0u64, 1 << 40, u64::MAX, 0xdead_beef] {
            assert_eq!(
                domowik_buffer_free(forged),
                status::UNKNOWN_BUFFER,
                "forged token {forged} was not refused"
            );
        }
    }

    #[test]
    fn two_empty_buffers_are_distinct_and_both_freeable() {
        // The defect that made the registry token-keyed. Rust gives every
        // zero-length allocation the same dangling address (`0x1` for `u8`),
        // so two outstanding EMPTY buffers — two `last_error` calls on handles
        // with no error — collided in a pointer-keyed map: the second insert
        // overwrote the first, the outstanding count read 1 for 2, and freeing
        // the second returned UNKNOWN_BUFFER for a buffer this library had
        // legitimately issued.
        let h1 = open_plan();
        let h2 = open_plan();
        let mut a = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        let mut b = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        assert_eq!(unsafe { domowik_last_error(h1, &mut a) }, status::OK);
        assert_eq!(unsafe { domowik_last_error(h2, &mut b) }, status::OK);
        assert_eq!(a.len, 0, "the fixture needs EMPTY buffers");
        assert_eq!(b.len, 0);
        assert_eq!(a.data, b.data, "empty allocations do share an address");
        assert_ne!(a.token, b.token, "but they must not share a token");

        assert_eq!(domowik_buffer_free(a.token), status::OK);
        assert_eq!(
            domowik_buffer_free(b.token),
            status::OK,
            "a legitimately issued buffer must be freeable"
        );
        assert_eq!(domowik_plan_close(h1), status::OK);
        assert_eq!(domowik_plan_close(h2), status::OK);
    }

    #[test]
    fn a_panic_is_contained_and_the_library_stays_usable() {
        assert_eq!(domowik_debug_panic(), status::PANIC);
        // "Contained" means the process survived AND the library still works.
        // A mechanism that survives the panic but leaves a poisoned lock is
        // not containment, it is a delayed crash.
        let h = open_plan();
        let req = create_level_request();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn a_panic_while_holding_the_plan_lock_leaves_the_plan_usable() {
        // Poison recovery, which `domowik_debug_panic` never exercised: it
        // panics holding nothing, so "usable afterwards" proved containment
        // and said nothing about the poisoned lock this ABI claims to recover
        // from. Found by adversarial review.
        let h = open_plan();
        let req = create_level_request();
        call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });

        assert_eq!(domowik_debug_panic_holding_lock(h), status::PANIC);

        // The lock is now poisoned. Every path must still work, and the
        // document must be intact rather than merely reachable.
        let (code, doc) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        assert_eq!(code, status::OK);
        let doc: serde_json::Value = serde_json::from_slice(&doc).expect("json");
        assert_eq!(doc["levels"].as_array().expect("levels").len(), 1);

        let req = create_level_request();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK, "a poisoned lock must not brick the plan");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn a_malformed_request_is_rejected_and_the_reason_is_retrievable() {
        let h = open_plan();
        let junk = b"{not json";
        let mut buf = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        assert_eq!(
            unsafe { domowik_plan_execute(h, junk.as_ptr(), junk.len() as u64, &mut buf) },
            status::MALFORMED_REQUEST
        );

        let (code, message) = call_returning(|out| unsafe { domowik_last_error(h, out) });
        assert_eq!(code, status::OK);
        assert!(!message.is_empty(), "no reason was recorded");

        // Sticky: retrieving does not clear it, so a caller can read it twice
        // and get the same answer.
        let (_, again) = call_returning(|out| unsafe { domowik_last_error(h, out) });
        assert_eq!(message, again);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn a_command_the_core_refuses_is_reported_as_rejected_not_as_success() {
        // An opening hosted by a wall that does not exist: the core validates
        // and refuses. The bridge must not launder that into OK.
        let h = open_plan();
        let op = Operation::create(Entity::Opening(domowik_format::Opening::door(
            uuid::Uuid::now_v7(),
            0.5,
            900.0,
            2100.0,
            200.0,
        )));
        let req = serde_json::to_vec(&serde_json::json!({ "operations": [op] })).expect("encodes");
        let mut buf = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        assert_eq!(
            unsafe { domowik_plan_execute(h, req.as_ptr(), req.len() as u64, &mut buf) },
            status::REJECTED
        );
        assert!(
            buf.data.is_null(),
            "a failed call must not hand out a buffer"
        );
        let (_, message) = call_returning(|out| unsafe { domowik_last_error(h, out) });
        assert!(!message.is_empty());
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn a_gesture_rejection_does_not_duplicate_the_command_rejected_prefix() {
        let h = open_plan();
        let level = domowik_format::Level::new("Ground");
        let wall = domowik_format::Wall::new(
            level.id,
            domowik_format::Point::new(0.0, 0.0),
            domowik_format::Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let req = serde_json::to_vec(&serde_json::json!({
            "operations": [
                Operation::create(Entity::Level(level)),
                Operation::create(Entity::Wall(wall.clone()))
            ]
        }))
        .unwrap();
        assert_eq!(
            call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            })
            .0,
            status::OK
        );

        assert_eq!(
            edit_json(
                h,
                serde_json::json!({
                    "kind": "setWall", "wallId": wall.id, "thicknessMm": 0.0
                })
            )
            .0,
            status::REJECTED
        );
        let (_, message) = call_returning(|out| unsafe { domowik_last_error(h, out) });
        let message = String::from_utf8(message).unwrap();
        assert_eq!(
            message.matches("command rejected:").count(),
            1,
            "user-facing error should have one rejection prefix: {message}"
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn a_cancel_stops_a_running_operation_without_waiting_for_it() {
        // The structural point: `domowik_cancel` must not take the lock the
        // running operation holds. If it did, this test would take the full
        // 10 seconds instead of milliseconds.
        let h = open_plan();
        let start = std::time::Instant::now();
        let worker = std::thread::spawn(move || domowik_busy(h, 10_000));
        // Let it actually start, so this measures a cancel and not a race.
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(domowik_cancel(h), status::OK);
        assert_eq!(worker.join().expect("joins"), status::CANCELED);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "cancel waited for the work it was cancelling: {:?}",
            start.elapsed()
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn no_operation_ever_erases_a_recorded_cancellation() {
        // The review's medium finding. The cancellation state used to be a
        // bool that every long operation CLEARED on entry, so a cancel could
        // be overwritten by an unrelated operation starting and the caller got
        // `OK` from `domowik_cancel` for a cancellation that never happened.
        //
        // This asserts the invariant that makes the loss impossible — the
        // cancellation state only ever moves forward — rather than racing the
        // few-instruction window in which the old shape lost it. A stress test
        // that raced it was written first and PASSED on the broken
        // implementation, which is why it is not here.
        let h = open_plan();
        assert_eq!(domowik_debug_cancel_epoch(h), 0);

        assert_eq!(domowik_cancel(h), status::OK);
        let recorded = domowik_debug_cancel_epoch(h);
        assert!(recorded > 0, "the cancel was not recorded");

        // Operations start and finish; none of them may walk it back.
        assert_eq!(domowik_busy(h, 0), status::OK);
        assert_eq!(domowik_busy(h, 1), status::OK);
        let req = create_level_request();
        call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(
            domowik_debug_cancel_epoch(h),
            recorded,
            "an operation erased a recorded cancellation"
        );

        // And a further cancel still moves it forward.
        assert_eq!(domowik_cancel(h), status::OK);
        assert!(domowik_debug_cancel_epoch(h) > recorded);
        assert_eq!(domowik_plan_close(h), status::OK);
        assert_eq!(
            domowik_debug_cancel_epoch(h),
            -1,
            "a closed handle has no epoch"
        );
    }

    #[test]
    fn a_cancel_does_not_leak_into_the_next_operation() {
        let h = open_plan();
        assert_eq!(domowik_cancel(h), status::OK);
        // The flag is cleared when an operation STARTS, so this one runs.
        assert_eq!(domowik_busy(h, 1), status::OK);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn concurrent_calls_on_one_handle_serialise_and_all_succeed() {
        // The core's Session is not safe for unsynchronised concurrent use, so
        // the bridge serialises per handle. Sixteen threads racing one handle
        // must produce sixteen levels and no corruption.
        let h = open_plan();
        let threads: Vec<_> = (0..16)
            .map(|_| {
                std::thread::spawn(move || {
                    let req = create_level_request();
                    let mut buf = DomowikBuf {
                        data: std::ptr::null_mut(),
                        len: 0,
                        token: 0,
                    };
                    let code = unsafe {
                        domowik_plan_execute(h, req.as_ptr(), req.len() as u64, &mut buf)
                    };
                    if code == status::OK {
                        assert_eq!(domowik_buffer_free(buf.token), status::OK);
                    }
                    code
                })
            })
            .collect();
        for t in threads {
            assert_eq!(t.join().expect("joins"), status::OK);
        }
        let (_, doc) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        let doc: serde_json::Value = serde_json::from_slice(&doc).expect("json");
        assert_eq!(doc["levels"].as_array().expect("levels").len(), 16);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn distinct_handles_are_independent() {
        let a = open_plan();
        let b = open_plan();
        let req = create_level_request();
        call_returning(|out| unsafe {
            domowik_plan_execute(a, req.as_ptr(), req.len() as u64, out)
        });
        let (_, doc_b) = call_returning(|out| unsafe { domowik_plan_snapshot(b, out) });
        let doc_b: serde_json::Value = serde_json::from_slice(&doc_b).expect("json");
        assert!(
            doc_b["levels"].as_array().is_none_or(|l| l.is_empty()),
            "handles share state: {doc_b}"
        );
        assert_eq!(domowik_plan_close(a), status::OK);
        assert_eq!(domowik_plan_close(b), status::OK);
    }

    /// Joins `handle`, but fails with a clear message instead of hanging
    /// `cargo test` forever (no per-test timeout exists) if it does not
    /// finish within `timeout`. Used by the quiescence tests below, where a
    /// genuine deadlock in the fix under test — not a flaky timing issue —
    /// is exactly the failure mode a bare `.join()` would turn into a stuck
    /// CI run instead of a red test.
    fn join_with_timeout<T: Send + 'static>(
        handle: std::thread::JoinHandle<T>,
        timeout: std::time::Duration,
    ) -> T {
        let start = std::time::Instant::now();
        while !handle.is_finished() {
            assert!(
                start.elapsed() < timeout,
                "thread did not finish within {timeout:?} — treat this as a \
                 deadlock in the code under test, not a flaky timing issue"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        handle.join().expect("joins")
    }

    #[test]
    fn close_waits_for_a_call_that_has_not_reached_the_core_yet() {
        // The review's high finding, and the reason the plan mutex was never a
        // quiescence barrier: a call resolves its handle and parses its request
        // BEFORE taking the plan lock, so `close` saw a free mutex and returned
        // while that thread was about to enter the core.
        //
        // issue 0063: the ORIGINAL version of this test guessed at that
        // timing with a fixed `sleep`, then sampled a flag the worker set
        // AFTER its export call returned — both guesses were wrong, and a
        // green run was never proof otherwise (see the module doc). A
        // SECOND version replaced the sleep with `test_hooks::arm` but kept
        // `sleep(50ms)` + `!closer.is_finished()` as the proof that `close`
        // was blocked — which has the identical shape of flaw: an
        // unscheduled `closer` thread also reads as "not finished," so a
        // `close` that skipped the wait ENTIRELY would still pass. This
        // version watches the actual wait seam (`Gate::wait_for_quiescence`'s
        // `on_quiescence_wait` hook) instead: receiving that signal is only
        // possible if `close` reached a point it cannot return from except
        // via this lease dropping, which is airtight rather than probable.
        let h = open_plan();
        let ops: Vec<_> = (0..4000)
            .map(|i| Operation::create(Entity::Level(Level::new(format!("L{i}")))))
            .collect();
        let big = serde_json::to_vec(&serde_json::json!({ "operations": ops })).expect("encodes");

        let (admitted, release) = test_hooks::arm(h, test_hooks::Phase::Admitted);
        let worker = std::thread::spawn(move || {
            let mut buf = DomowikBuf {
                data: std::ptr::null_mut(),
                len: 0,
                token: 0,
            };
            let code = unsafe { domowik_plan_execute(h, big.as_ptr(), big.len() as u64, &mut buf) };
            if code == status::OK {
                assert_eq!(domowik_buffer_free(buf.token), status::OK);
            }
            code
        });

        // Deterministic: the lease really has been taken, not "probably
        // taken by now."
        admitted
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the call must be admitted for this test to mean anything");

        // The worker is now parked INSIDE the ABI, lease held, plan lock not
        // even taken yet. Race `close` against it and require proof it
        // actually reached its wait — not a sleep-and-hope.
        let waiting = test_hooks::watch_quiescence_wait(h);
        let closer = std::thread::spawn(move || domowik_plan_close(h));
        waiting
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect(
                "close must reach its quiescence wait while an admitted call's lease is still held",
            );
        // Corroboration, not the proof: having just observed the wait seam
        // with the lease still open, `close` cannot legitimately have
        // returned already — this is belt-and-suspenders, not load-bearing.
        assert!(
            !closer.is_finished(),
            "close returned immediately after reaching its wait, before the lease was released"
        );

        release.release();
        assert_eq!(
            join_with_timeout(closer, std::time::Duration::from_secs(5)),
            status::OK
        );
        assert_eq!(
            join_with_timeout(worker, std::time::Duration::from_secs(5)),
            status::OK,
            "the execute must have been IN FLIGHT for this test to mean anything"
        );
    }

    #[test]
    fn close_waits_for_a_call_that_never_touches_the_session() {
        // `domowik_busy` takes no plan lock, so the previous drain never
        // waited for it at all. The lease does. See the sibling test above
        // for why this watches the wait seam rather than sleeping.
        let h = open_plan();
        let (admitted, release) = test_hooks::arm(h, test_hooks::Phase::Admitted);
        let worker = std::thread::spawn(move || domowik_busy(h, 0));

        admitted
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the call must be admitted for this test to mean anything");

        let waiting = test_hooks::watch_quiescence_wait(h);
        let closer = std::thread::spawn(move || domowik_plan_close(h));
        waiting
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect(
                "close must reach its quiescence wait while domowik_busy's lease is still held",
            );
        assert!(
            !closer.is_finished(),
            "close returned immediately after reaching its wait, before the lease was released"
        );

        release.release();
        assert_eq!(
            join_with_timeout(closer, std::time::Duration::from_secs(5)),
            status::OK
        );
        assert_eq!(
            join_with_timeout(worker, std::time::Duration::from_secs(5)),
            status::OK
        );
    }

    /// The pre-fix `close`, faithfully reproduced: removes the registry
    /// entry and returns at once, never waiting for outstanding leases.
    /// Exists ONLY so the tests below can prove they actually detect the
    /// defect issue 0063 fixes — a fixture that cannot go red against this
    /// is not evidence of anything, no matter how many times it passes
    /// against the real implementation.
    ///
    /// Memory-safe by construction, despite dropping the registry entry out
    /// from under an in-flight call: that call holds its own `Arc<Gate>` and
    /// `Arc<Mutex<Plan>>` clones (taken in `resolve`/`resolve_epoch` before
    /// this ever runs), so nothing it is using is freed.
    extern "C" fn close_without_waiting(handle: u64) -> i32 {
        if registry().plans.remove(&handle).is_none() {
            return status::UNKNOWN_HANDLE;
        }
        status::OK
    }

    #[derive(Debug, PartialEq, Eq)]
    enum CloseVerdict {
        /// Reached its quiescence wait and did not return before release.
        BlockedUntilReleased,
        /// Returned without ever reaching a quiescence wait.
        ReturnedWhileLeaseHeld,
    }

    /// Holds a call open on a fresh plan, races `close_fn` against it, and
    /// reports which of the two ways it behaved. Deterministic either way:
    /// the discriminator is whether `close_fn` reaches the quiescence-wait
    /// seam within a generous bound, not how long anything takes to run.
    /// The real `close` always reaches it almost immediately once the lease
    /// is held; `close_without_waiting` never reaches it at all, so its
    /// absence for the FULL bound is itself the (bounded) evidence — not a
    /// race that could go either way.
    fn probe_close(close_fn: extern "C" fn(u64) -> i32) -> CloseVerdict {
        let h = open_plan();
        let (admitted, release) = test_hooks::arm(h, test_hooks::Phase::Admitted);
        let holder = std::thread::spawn(move || domowik_busy(h, 0));
        admitted
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the call must be admitted for this test to mean anything");

        let waiting = test_hooks::watch_quiescence_wait(h);
        let closer = std::thread::spawn(move || close_fn(h));
        let verdict = match waiting.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(()) => {
                assert!(
                    !closer.is_finished(),
                    "reached the wait but returned immediately anyway"
                );
                CloseVerdict::BlockedUntilReleased
            }
            Err(_) => {
                // Distinguish the mutant (already returned) from a real
                // `close` that was merely slow to reach the seam within the
                // 1s bound (a late scheduler, not a defect): the latter is
                // still genuinely blocked on the lease we have not released
                // yet, so joining it here first would just spend the next
                // 5s discovering that and panic with a "deadlock" message
                // that is honest about the SYMPTOM but not the cause.
                assert!(
                    closer.is_finished(),
                    "close neither reached its quiescence wait nor returned"
                );
                assert_eq!(
                    join_with_timeout(closer, std::time::Duration::from_secs(5)),
                    status::OK,
                    "a close that skips the wait must still return OK, just without waiting"
                );
                release.release();
                assert_eq!(
                    join_with_timeout(holder, std::time::Duration::from_secs(5)),
                    status::OK
                );
                return CloseVerdict::ReturnedWhileLeaseHeld;
            }
        };

        release.release();
        assert_eq!(
            join_with_timeout(closer, std::time::Duration::from_secs(5)),
            status::OK
        );
        assert_eq!(
            join_with_timeout(holder, std::time::Duration::from_secs(5)),
            status::OK
        );
        verdict
    }

    #[test]
    fn close_blocks_until_the_admitted_calls_lease_is_released() {
        assert_eq!(
            probe_close(domowik_plan_close),
            CloseVerdict::BlockedUntilReleased
        );
    }

    /// The negative control issue 0063's review asked for: proves the
    /// fixture above actually discriminates, by running it against a
    /// deliberately reintroduced version of the ORIGINAL defect.
    #[test]
    fn close_without_waiting_is_caught_returning_while_a_lease_is_still_held() {
        assert_eq!(
            probe_close(close_without_waiting),
            CloseVerdict::ReturnedWhileLeaseHeld
        );
    }

    /// Reviewer-flagged coverage gap (issue 0063): a call landing after
    /// `close` removes the registry entry, but before quiescence completes,
    /// must be refused — not admitted into a plan that is on its way out.
    ///
    /// This holds a lease at `PreLeaseDrop` on a REAL buffered export
    /// (`domowik_plan_snapshot`), not `domowik_busy`, which publishes
    /// nothing — parking a call with no output at `PreLeaseDrop` would only
    /// re-demonstrate what `Admitted` already does. What proves this
    /// buffer was published before `close` can return is the SEAM
    /// POSITION: `PreLeaseDrop` fires at the top of `Lease::drop`, which is
    /// after `release()` — the call that writes `*out` and registers the
    /// token — in that call's own program order, combined with having
    /// observed admission before racing `close` at all. It is not a
    /// post-hoc inference from reading the buffer afterwards.
    #[test]
    fn no_new_call_can_be_admitted_after_the_registry_entry_is_removed() {
        let h = open_plan();

        let (admitted, release) = test_hooks::arm(h, test_hooks::Phase::PreLeaseDrop);
        let holder = std::thread::spawn(move || {
            let mut buf = DomowikBuf {
                data: std::ptr::null_mut(),
                len: 0,
                token: 0,
            };
            let code = unsafe { domowik_plan_snapshot(h, &mut buf) };
            assert_eq!(code, status::OK);
            // `DomowikBuf` holds a raw pointer and is not `Send` — copy the
            // bytes here, on the thread that owns them, before crossing.
            let bytes = unsafe { std::slice::from_raw_parts(buf.data, buf.len as usize) }.to_vec();
            (buf.token, bytes)
        });
        admitted
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect(
                "the held call must reach its pre-lease-drop point, after publishing its output",
            );

        let removed = test_hooks::watch_registry_removal(h);
        // Same treatment as the other close tests, and load-bearing here
        // too (peer-review MUST FIX): `watch_registry_removal` alone only
        // proves close STARTED — a remove-then-return mutant fires that
        // latch just as well, and the `!closer.is_finished()` check below
        // could still pass on it if the closer thread happened to be
        // descheduled right after returning. Requiring BOTH one-shot
        // signals before that check is what makes it evidence that close
        // reached its actual wait while a call parked at `PreLeaseDrop`
        // held the lease, not just that it began running.
        let waiting = test_hooks::watch_quiescence_wait(h);
        let closer = std::thread::spawn(move || domowik_plan_close(h));
        removed
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("close must remove the registry entry promptly");
        waiting
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("close must reach its quiescence wait while the held call's lease is open");

        // The entry is gone, but the held call has not dropped its lease
        // yet — close is still draining. A brand-new call on this handle
        // now must see an unknown handle, not a plan mid-teardown.
        assert_eq!(domowik_busy(h, 0), status::UNKNOWN_HANDLE);
        assert!(
            !closer.is_finished(),
            "close returned while the held call's lease was still open"
        );

        release.release();
        assert_eq!(
            join_with_timeout(closer, std::time::Duration::from_secs(5)),
            status::OK
        );
        let (token, bytes) = join_with_timeout(holder, std::time::Duration::from_secs(5));

        assert!(token != 0);
        let snapshot: serde_json::Value =
            serde_json::from_slice(&bytes).expect("a snapshot is JSON");
        assert!(snapshot.is_object());
        // The token-freeable-after-close contract: a buffer issued before
        // `close` is a caller-owned object independent of the plan's own
        // lifetime, not something `close` invalidates.
        assert_eq!(
            domowik_buffer_free(token),
            status::OK,
            "a buffer issued before close must still be freeable after it"
        );
    }

    #[test]
    fn close_with_nothing_in_flight_returns_promptly() {
        // Control: without this, a `close` that blocked FOREVER — not just
        // one that skips waiting — would still pass every other test here,
        // since every other test releases its own held call before joining.
        //
        // Asserts the actual property, not a wall-clock proxy for it: with
        // nothing outstanding, `*active > 0` is false on `close`'s very
        // first check, so `on_quiescence_wait` is never called at all —
        // `try_recv` on it finding nothing IS "close did not wait," with no
        // timing dependence. This also happens to confirm the seam is
        // conditioned on `*active > 0`, not fired unconditionally.
        //
        // Run on its own thread and joined with a bound, like every other
        // close in this group: a future regression that made the wait
        // unconditional would otherwise deadlock this call inline on the
        // test's own thread, with nothing left to run `cargo test` at all.
        let h = open_plan();
        let waiting = test_hooks::watch_quiescence_wait(h);
        let closer = std::thread::spawn(move || domowik_plan_close(h));
        assert_eq!(
            join_with_timeout(closer, std::time::Duration::from_secs(5)),
            status::OK
        );
        assert!(
            waiting.try_recv().is_err(),
            "close waited on a lease when nothing was outstanding"
        );
    }

    #[test]
    fn close_on_one_handle_does_not_stall_progress_on_another() {
        // `close`'s own doc claims the registry lock is released before
        // waiting specifically so "a long-running commit on one plan cannot
        // stall every other handle" — this is the first test that actually
        // exercises two handles at once to check it.
        let a = open_plan();
        let b = open_plan();

        let (admitted, release) = test_hooks::arm(a, test_hooks::Phase::Admitted);
        let holder = std::thread::spawn(move || domowik_busy(a, 0));
        admitted
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the call on A must be admitted for this test to mean anything");

        let waiting = test_hooks::watch_quiescence_wait(a);
        let closer = std::thread::spawn(move || domowik_plan_close(a));
        waiting
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("close on A must reach its quiescence wait while A's lease is held");

        // A's close is genuinely blocked on A's lease right now. B must be
        // completely unaffected.
        assert_eq!(domowik_busy(b, 0), status::OK);
        assert_eq!(domowik_plan_close(b), status::OK);

        assert!(
            !closer.is_finished(),
            "A's close must still be waiting on A's lease after B made progress"
        );
        release.release();
        assert_eq!(
            join_with_timeout(closer, std::time::Duration::from_secs(5)),
            status::OK
        );
        assert_eq!(
            join_with_timeout(holder, std::time::Duration::from_secs(5)),
            status::OK
        );
    }

    #[test]
    fn the_abi_version_is_reported() {
        assert_eq!(domowik_abi_version(), ABI_VERSION);
    }

    // -----------------------------------------------------------------------
    // Hit testing (issue 0045)
    // -----------------------------------------------------------------------

    fn hit_request(level: Uuid, x: f64, y: f64, radius: f64) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "levelId": level,
            "pointMm": [x, y],
            "radiusMm": radius,
        }))
        .expect("encodes")
    }

    fn snap_at(h: u64, request: serde_json::Value) -> serde_json::Value {
        let req = serde_json::to_vec(&request).expect("encodes");
        let (code, body) = call_returning(|out| unsafe {
            domowik_plan_snap(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        serde_json::from_slice(&body).expect("snap response")
    }

    #[test]
    fn snap_keeps_local_grid_and_honors_alt_without_document_sources() {
        let h = open_plan();
        let unknown = Uuid::nil();
        let request = serde_json::json!({
            "levelId": unknown, "pointMm": [1.0, 1.0], "radiusMm": 10.0,
            "visibleRectMm": [-20.0, -20.0, 20.0, 20.0], "gridSpacingMm": 10.0,
            "objectEnabled": false, "angleEnabled": false,
        });
        let response = snap_at(h, request.clone());
        assert_eq!(response["candidates"][0]["class"], "grid");
        assert_eq!(
            response["candidates"][0]["entityId"],
            serde_json::Value::Null
        );
        assert_eq!(response["candidates"][0]["feature"]["kind"], "grid");
        let mut alt_request = request;
        alt_request["altDisabled"] = serde_json::Value::Bool(true);
        let response = snap_at(h, alt_request);
        assert!(response["candidates"].as_array().unwrap().is_empty());
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn snap_returns_wall_endpoints_and_axis_extensions_outside_the_document() {
        let (h, level) = plan_with_a_room();
        let endpoint = snap_at(
            h,
            serde_json::json!({
                "levelId": level, "pointMm": [0.0, 0.0], "radiusMm": 1.0,
                "visibleRectMm": [-10.0, -10.0, 10.0, 10.0],
                "gridEnabled": false, "angleEnabled": false,
            }),
        );
        assert_eq!(endpoint["candidates"][0]["class"], "free-endpoint");
        assert_eq!(endpoint["candidates"][0]["feature"]["kind"], "entity");
        let extension = snap_at(
            h,
            serde_json::json!({
                "levelId": level, "pointMm": [20_000_000.0, 0.0], "radiusMm": 1.0,
                "visibleRectMm": [19_999_990.0, -10.0, 20_000_010.0, 10.0],
                "gridEnabled": false, "angleEnabled": false,
            }),
        );
        assert_eq!(extension["candidates"][0]["class"], "axis");
        assert_eq!(extension["candidates"][0]["feature"]["onSegment"], false);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// `drawn` is the occupied region with opening cuts (R4.2.2) while
    /// `occupied` stays unamended; a level without openings keeps no second
    /// engine, and deleting its last opening frees it.
    #[test]
    fn derived_drawn_region_carries_the_cuts() {
        let h = open_plan();
        let level = domowik_format::Level::new("Ground");
        let level_id = level.id;
        let execute = |ops: Vec<Operation>| {
            let req = serde_json::to_vec(&serde_json::json!({ "operations": ops })).unwrap();
            let (code, _) = call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            });
            assert_eq!(code, status::OK);
        };
        execute(vec![Operation::create(Entity::Level(level))]);
        let w = domowik_format::Wall::new(
            level_id,
            domowik_format::Point::new(0.0, 0.0),
            domowik_format::Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let w_id = w.id;
        execute(vec![Operation::create(Entity::Wall(w))]);
        let id = std::ffi::CString::new(level_id.to_string()).unwrap();
        let read = || {
            let (code, out) =
                call_returning(|out| unsafe { domowik_plan_derived(h, id.as_ptr(), out) });
            assert_eq!(code, status::OK);
            serde_json::from_slice::<serde_json::Value>(&out).unwrap()
        };
        let engine = || {
            let (plan, _lease) = resolve(h).unwrap();
            let present = plan.lock().unwrap().levels[&level_id].drawn.is_some();
            present
        };
        let d = read();
        assert_eq!(d["drawn"], d["occupied"]);
        assert!(!engine(), "no openings: no second engine");

        let door = domowik_format::Opening::door(w_id, 0.5, 900.0, 2100.0, 200.0);
        let door_id = door.id;
        execute(vec![Operation::create(Entity::Opening(door))]);
        let d = read();
        assert_eq!(d["occupied"].as_array().unwrap().len(), 1, "unamended");
        assert_eq!(
            d["drawn"],
            serde_json::json!([
                {"outer": [[0.0, -100.0], [1550.0, -100.0], [1550.0, 100.0], [0.0, 100.0]], "holes": []},
                {"outer": [[2450.0, -100.0], [4000.0, -100.0], [4000.0, 100.0], [2450.0, 100.0]], "holes": []}
            ]),
            "§4 F25"
        );
        assert!(engine());

        execute(vec![Operation::delete(door_id)]);
        let d = read();
        assert_eq!(d["drawn"], d["occupied"]);
        assert!(!engine(), "the last opening's removal frees the engine");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// The derived `cut` is the R4.2.1 footprint: a partial-depth window on
    /// the Left face is the notch y ∈ [0, 100] (§4 F26), not a
    /// full-thickness hole.
    #[test]
    fn a_partial_depth_cut_is_a_notch() {
        let w = domowik_format::Wall::new(
            Uuid::nil(),
            domowik_format::Point::new(0.0, 0.0),
            domowik_format::Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let mut window = domowik_format::Opening::door(w.id, 0.5, 1200.0, 1300.0, 100.0);
        window.side = domowik_format::Side::Left;
        let json = aperture_json(&w, &window).expect("fits");
        assert_eq!(
            json["cut"],
            serde_json::json!([
                [1400.0, 0.0],
                [2600.0, 0.0],
                [2600.0, 100.0],
                [1400.0, 100.0]
            ])
        );
        window.side = domowik_format::Side::Right;
        let json = aperture_json(&w, &window).expect("fits");
        assert_eq!(
            json["cut"],
            serde_json::json!([
                [1400.0, -100.0],
                [2600.0, -100.0],
                [2600.0, 0.0],
                [1400.0, 0.0]
            ])
        );
    }

    /// Apertures are reused across derived reads while their opening and
    /// host are unchanged (R4.3.7, issue 0013): an edit on another component
    /// rebuilds none, an edit of a host rebuilds only its openings, and the
    /// output always equals a fresh construction.
    #[test]
    fn derived_apertures_are_rebuilt_only_for_changed_hosts() {
        let h = open_plan();
        let level = domowik_format::Level::new("Ground");
        let level_id = level.id;
        let execute = |ops: Vec<Operation>| {
            let req = serde_json::to_vec(&serde_json::json!({ "operations": ops })).unwrap();
            let (code, _) = call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            });
            assert_eq!(code, status::OK);
        };
        execute(vec![Operation::create(Entity::Level(level))]);
        let wall = |x: f64| {
            domowik_format::Wall::new(
                level_id,
                domowik_format::Point::new(x, 0.0),
                domowik_format::Point::new(x + 4000.0, 0.0),
                200.0,
                2500.0,
            )
        };
        let (a, b) = (wall(0.0), wall(20_000.0));
        let (a_id, b_id) = (a.id, b.id);
        execute(vec![
            Operation::create(Entity::Wall(a)),
            Operation::create(Entity::Wall(b)),
        ]);
        execute(vec![
            Operation::create(Entity::Opening(domowik_format::Opening::door(
                a_id, 0.3, 900.0, 2100.0, 200.0,
            ))),
            Operation::create(Entity::Opening(domowik_format::Opening::door(
                a_id, 0.7, 900.0, 2100.0, 200.0,
            ))),
            Operation::create(Entity::Opening(domowik_format::Opening::door(
                b_id, 0.5, 900.0, 2100.0, 200.0,
            ))),
        ]);
        let id = std::ffi::CString::new(level_id.to_string()).unwrap();
        let read = || {
            let (code, out) =
                call_returning(|out| unsafe { domowik_plan_derived(h, id.as_ptr(), out) });
            assert_eq!(code, status::OK);
            let derived: serde_json::Value = serde_json::from_slice(&out).unwrap();
            derived["openings"].clone()
        };
        let fresh = || {
            let (plan, _lease) = resolve(h).unwrap();
            let plan = plan.lock().unwrap();
            let doc = plan.session().document();
            let mut out = Vec::new();
            for wall in doc.walls_on_level(level_id) {
                for o in doc.openings.iter().filter(|o| o.host_wall_id == wall.id) {
                    out.extend(aperture_json(wall, o));
                }
            }
            serde_json::Value::Array(out)
        };
        let builds = || {
            let (plan, _lease) = resolve(h).unwrap();
            let n = plan.lock().unwrap().aperture_builds;
            n
        };
        assert_eq!(read(), fresh());
        assert_eq!(builds(), 3);
        assert_eq!(read(), fresh());
        assert_eq!(builds(), 3, "a repeated read rebuilds nothing");

        // A new wall elsewhere on the level: no aperture rebuilt.
        execute(vec![Operation::create(Entity::Wall(wall(40_000.0)))]);
        assert_eq!(read(), fresh());
        assert_eq!(builds(), 3, "an unrelated edit rebuilds no aperture");

        // Thickening host B rebuilds B's opening only.
        execute(vec![Operation::set_fields(
            b_id,
            serde_json::Map::from_iter([("thicknessMm".to_owned(), serde_json::json!(300.0))]),
        )]);
        assert_eq!(read(), fresh());
        assert_eq!(builds(), 4, "only the edited host's opening is rebuilt");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// The derived response displays the EXACT area (R5.3.2), not a rounding
    /// of the float one: Codex GPT-6-Astra's room whose float area rounds to
    /// `.98` and whose exact area to `.99`.
    #[test]
    fn derived_rooms_display_the_exact_area() {
        let h = open_plan();
        let level = domowik_format::Level::new("Ground");
        let level_id = level.id;
        let execute = |op: Operation| {
            let req = serde_json::to_vec(&serde_json::json!({ "operations": [op] })).unwrap();
            let (code, _) = call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            });
            assert_eq!(code, status::OK);
        };
        execute(Operation::create(Entity::Level(level)));
        let (width, height) = (10_239_999_999.0 / 1024.0, 5_488_639_999.0 / 1024.0);
        let c = [
            (-5_000_100.0, -5_000_100.0),
            (-4_999_900.0 + width, -5_000_100.0),
            (-4_999_900.0 + width, -4_999_900.0 + height),
            (-5_000_100.0, -4_999_900.0 + height),
        ];
        for i in 0..4 {
            let (a, b) = (c[i], c[(i + 1) % 4]);
            execute(Operation::create(Entity::Wall(domowik_format::Wall::new(
                level_id,
                domowik_format::Point::new(a.0, a.1),
                domowik_format::Point::new(b.0, b.1),
                200.0,
                2500.0,
            ))));
        }
        let id = std::ffi::CString::new(level_id.to_string()).unwrap();
        let (code, out) =
            call_returning(|out| unsafe { domowik_plan_derived(h, id.as_ptr(), out) });
        assert_eq!(code, status::OK);
        let derived: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(derived["rooms"][0]["displayAreaM2"], "53599999.99");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// The 3D scene's floors come from the level's retained geometry (issue
    /// 0013): the scene equals a one-shot `domowik_scene::build` of the same
    /// document, shares the snap index's sync, and follows edits.
    #[test]
    fn scene_floors_reuse_the_retained_level_geometry() {
        let (h, level) = plan_with_a_room();
        let scene = |h: u64| {
            let bytes =
                serde_json::to_vec(&serde_json::json!({"kind": "scene", "levelIds": [level]}))
                    .unwrap();
            let (code, out) = call_returning(|out| unsafe {
                domowik_plan_assets(h, bytes.as_ptr(), bytes.len() as u64, out)
            });
            assert_eq!(code, status::OK);
            let mut value: serde_json::Value = serde_json::from_slice(&out).unwrap();
            value.as_object_mut().unwrap().remove("modelDecodes");
            // Presenter baseline identity is not geometry. The pure builder
            // leaves it zero; each full bridge load assigns a fresh token.
            value["sceneToken"] = serde_json::json!(0);
            value
        };
        let oracle = |h: u64| {
            let (plan, _lease) = resolve(h).unwrap();
            let doc = plan.lock().unwrap().session().document().clone();
            let built = domowik_scene::build(
                &doc,
                &[level],
                &std::collections::BTreeSet::new(),
                &std::collections::BTreeMap::new(),
            )
            .unwrap();
            serde_json::to_value(built).unwrap()
        };
        let floors = |v: &serde_json::Value| {
            v["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|n| n["id"].as_str().unwrap().starts_with("floor:"))
                .count()
        };

        let first = scene(h);
        assert_eq!(first, oracle(h));
        assert_eq!(floors(&first), 1);
        assert_eq!(domowik_debug_snap_index_builds(h), 1);
        assert_eq!(scene(h), first);
        assert_eq!(
            domowik_debug_snap_index_builds(h),
            1,
            "a repeated scene at the same revision does not sync again"
        );

        // A dividing wall splits the room: the next scene syncs once and has
        // two floors, still equal to the one-shot build. Drawn through the
        // edit builder, which splits the walls it lands on: a raw batch
        // leaving those T-junctions unsplit is refused (ED-0119).
        let (code, _) = edit_json(
            h,
            serde_json::json!({"kind":"addWall", "levelId":level,
                "start":[2000.0, 0.0], "end":[2000.0, 3000.0], "thicknessMm":100.0}),
        );
        assert_eq!(code, status::OK);
        let split = scene(h);
        assert_eq!(split, oracle(h));
        assert_eq!(floors(&split), 2);
        assert_eq!(domowik_debug_snap_index_builds(h), 2);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn snap_cache_survives_reads_and_save_and_rebuilds_for_history() {
        let (h, level) = plan_with_a_room();
        let request = serde_json::json!({
            "levelId": level, "pointMm": [0.0, 0.0], "radiusMm": 10.0,
            "visibleRectMm": [-20.0, -20.0, 20.0, 20.0],
            "gridEnabled": false, "angleEnabled": false,
        });
        snap_at(h, request.clone());
        snap_at(h, request.clone());
        assert_eq!(domowik_debug_snap_index_builds(h), 1);
        let mut other_level = request.clone();
        other_level["levelId"] = serde_json::json!(Uuid::nil());
        snap_at(h, other_level);
        assert_eq!(
            domowik_debug_snap_index_builds(h),
            2,
            "a new level gets its own index"
        );
        snap_at(h, request.clone());
        assert_eq!(
            domowik_debug_snap_index_builds(h),
            2,
            "returning to a level at the same revision reuses its index"
        );
        let (_, _) = call_returning(|out| unsafe { domowik_plan_state(h, out) });
        let (_, _) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        let path = std::ffi::CString::new(
            std::env::temp_dir()
                .join(format!("domowik-snap-{}.domo", Uuid::now_v7()))
                .to_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        snap_at(h, request.clone());
        assert_eq!(domowik_debug_snap_index_builds(h), 2);
        let mutate = create_level_request();
        assert_eq!(
            call_returning(|out| unsafe {
                domowik_plan_execute(h, mutate.as_ptr(), mutate.len() as u64, out)
            })
            .0,
            status::OK
        );
        snap_at(h, request.clone());
        assert_eq!(domowik_debug_snap_index_builds(h), 3);
        for redo in [false, true] {
            assert_eq!(
                call_returning(|out| unsafe { change_history(h, out, redo) }).0,
                status::OK
            );
            snap_at(h, request.clone());
        }
        assert_eq!(domowik_debug_snap_index_builds(h), 5);
        let refused_edit = serde_json::to_vec(&serde_json::json!({
            "kind": "moveWall", "wallId": Uuid::nil(), "start": [0.0, 0.0],
        }))
        .unwrap();
        refused(status::REJECTED, |out| unsafe {
            domowik_plan_edit(h, refused_edit.as_ptr(), refused_edit.len() as u64, out)
        });
        snap_at(h, request);
        assert_eq!(
            domowik_debug_snap_index_builds(h),
            5,
            "a refused edit retains the current snap cache"
        );
        assert_eq!(domowik_plan_close(h), status::OK);
        let _ = std::fs::remove_file(path.into_string().unwrap());
    }

    #[test]
    fn snap_exclusions_carry_identity_through_the_existing_abi_12_fields() {
        // `excludedWallIds` used to be refused outright because the merged
        // boundary had lost its provenance. It now does real work, and does it
        // without any change to the wire shape.
        let (h, level) = plan_with_a_room();
        // Documented by `a_marquee_selects_a_wall_it_only_touches`: the room's
        // first wall is (0,0)-(4000,0), t=200, so its outer face is y = -100.
        let bottom = snapshot_json(h)["walls"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();

        let classes = |excluded: &[&str]| -> Vec<String> {
            let request = serde_json::to_vec(&serde_json::json!({
                "levelId": level, "pointMm": [2000.0, -102.0], "radiusMm": 120.0,
                "visibleRectMm": [-5000.0, -5000.0, 9000.0, 9000.0],
                "excludedWallIds": excluded,
                "gridEnabled": false, "angleEnabled": false,
            }))
            .unwrap();
            let (code, body) = call_returning(|out| unsafe {
                domowik_plan_snap(h, request.as_ptr(), request.len() as u64, out)
            });
            assert_eq!(code, status::OK);
            let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
            parsed["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["class"].as_str().unwrap().to_owned())
                .collect()
        };

        // The pointer sits 2 mm below that face and 102 mm below the axis.
        let before = classes(&[]);
        assert!(before.contains(&"edge".to_owned()), "{before:?}");
        assert!(before.contains(&"axis".to_owned()), "{before:?}");

        // R6.1.3: the dragged wall's own edges and axis both go.
        assert_eq!(classes(&[&bottom]), Vec::<String>::new());

        // An id naming nothing on this level excludes nothing, and is not an
        // error — a caller may legitimately hold a stale or foreign id.
        let unknown = Uuid::now_v7().to_string();
        assert_eq!(classes(&[&unknown]), before);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn snap_refusals_leave_the_out_buffer_and_document_state_unchanged() {
        let (h, level) = plan_with_a_room();
        let (_, before) = call_returning(|out| unsafe { domowik_plan_state(h, out) });
        let invalid_radius = format!(
            "{{\"levelId\":\"{level}\",\"pointMm\":[0,0],\"radiusMm\":-1,\"visibleRectMm\":[-1,-1,1,1]}}"
        ).into_bytes();
        let invalid_view = format!(
            "{{\"levelId\":\"{level}\",\"pointMm\":[0,0],\"radiusMm\":1,\"visibleRectMm\":[1,-1,-1,1]}}"
        ).into_bytes();
        let oversized_grid = format!(
            "{{\"levelId\":\"{level}\",\"pointMm\":[0,0],\"radiusMm\":240000,\"visibleRectMm\":[-1,-1,1,1],\"gridSpacingMm\":1,\"objectEnabled\":false,\"angleEnabled\":false}}"
        ).into_bytes();
        for (request, reason) in [
            (&invalid_radius, "snap request contains invalid coordinates"),
            (&invalid_view, "snap request contains invalid coordinates"),
            (&oversized_grid, "oversized grid range"),
        ] {
            refused(status::MALFORMED_REQUEST, |out| unsafe {
                domowik_plan_snap(h, request.as_ptr(), request.len() as u64, out)
            });
            let (_, error) = call_returning(|out| unsafe { domowik_last_error(h, out) });
            assert!(std::str::from_utf8(&error).unwrap().contains(reason));
            let (_, after) = call_returning(|out| unsafe { domowik_plan_state(h, out) });
            assert_eq!(
                before, after,
                "refused snap changed session/history/warnings state"
            );
        }
        // Finite cursors must not silently saturate a lattice index to i64.
        // Test both axes and signs through JSON and the actual ABI, preserving
        // the sentinel output buffer and the complete session-state response.
        for point in [[1e25, 0.0], [-1e25, 0.0], [0.0, 1e25], [0.0, -1e25]] {
            let request = serde_json::to_vec(&serde_json::json!({
                "levelId": level, "pointMm": point, "radiusMm": 120.0,
                "visibleRectMm": [-1000.0, -1000.0, 1000.0, 1000.0],
                "gridSpacingMm": 100.0, "objectEnabled": false,
                "angleEnabled": false,
            }))
            .unwrap();
            refused(status::MALFORMED_REQUEST, |out| unsafe {
                domowik_plan_snap(h, request.as_ptr(), request.len() as u64, out)
            });
            let (_, error) = call_returning(|out| unsafe { domowik_last_error(h, out) });
            assert!(std::str::from_utf8(&error).unwrap().contains("grid range"));
            let (_, after) = call_returning(|out| unsafe { domowik_plan_state(h, out) });
            assert_eq!(before, after, "out-of-range grid query changed plan state");
        }
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// The ordered response, or a panic. Frees the buffer through the ABI.
    /// ED-0112: `radiusPx / zoom` is admitted exactly. 12 px at 0.1 px/mm
    /// is 12/fl(0.1) = 119.99999999999999… mm, so a free endpoint at exactly
    /// 120 mm is OUT — while the rounded `radiusMm` 12/0.1 = 120.0 lets it in.
    #[test]
    fn a_screen_radius_is_admitted_exactly_not_through_its_rounded_quotient() {
        let (h, level) = plan_with_a_room();
        let classes_with = |extra: serde_json::Value| -> Vec<String> {
            let mut body = serde_json::json!({
                "levelId": level, "pointMm": [0.0, -120.0], "radiusMm": 12.0 / 0.1,
            });
            for (k, v) in extra.as_object().unwrap() {
                body[k] = v.clone();
            }
            let req = serde_json::to_vec(&body).unwrap();
            let (code, out) = call_returning(|out| unsafe {
                domowik_plan_hit_test(h, req.as_ptr(), req.len() as u64, out)
            });
            assert_eq!(code, status::OK);
            let hits: Vec<serde_json::Value> = serde_json::from_slice(&out).unwrap();
            hits.iter()
                .map(|h| h["class"].as_str().unwrap().to_owned())
                .collect()
        };
        assert!(classes_with(serde_json::json!({})).contains(&"free-endpoint".to_owned()));
        assert!(
            !classes_with(serde_json::json!({"radiusPx": 12.0, "zoom": 0.1}))
                .contains(&"free-endpoint".to_owned())
        );
        // Half a pair is malformed, not silently ignored.
        let req = serde_json::to_vec(&serde_json::json!({
            "levelId": level, "pointMm": [0.0, 0.0], "radiusMm": 10.0, "radiusPx": 12.0,
        }))
        .unwrap();
        refused(status::MALFORMED_REQUEST, |out| unsafe {
            domowik_plan_hit_test(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// ED-0112: the relative angle family comes from the accepted segment's
    /// stored endpoints. A 45° turn of (3, 1) is exactly the ray (1, 2), so a
    /// pointer on it snaps to itself at distance zero.
    #[test]
    fn the_previous_segment_drives_exact_relative_angle_rays() {
        let (h, level) = plan_with_a_room();
        let anchor = [10_000.0, 10_000.0];
        let on_ray = [anchor[0] + 100.0, anchor[1] + 200.0];
        let response = snap_at(
            h,
            serde_json::json!({
                "levelId": level, "pointMm": on_ray, "radiusMm": 10.0,
                "visibleRectMm": [9000.0, 9000.0, 11000.0, 11000.0],
                "anchorMm": anchor,
                "previousSegmentMm": [[0.0, 0.0], [3.0, 1.0]],
                "gridEnabled": false, "objectEnabled": false, "angleEnabled": true,
            }),
        );
        let top = &response["candidates"][0];
        assert_eq!(top["class"], "angle");
        assert_eq!(top["pointMm"], serde_json::json!(on_ray));
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    fn hits_at(h: u64, level: Uuid, x: f64, y: f64, radius: f64) -> Vec<serde_json::Value> {
        let req = hit_request(level, x, y, radius);
        let (code, body) = call_returning(|out| unsafe {
            domowik_plan_hit_test(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK, "hit test at ({x}, {y})");
        serde_json::from_slice(&body).expect("an ordered array")
    }

    fn classes(hits: &[serde_json::Value]) -> Vec<&str> {
        hits.iter()
            .map(|h| h["class"].as_str().expect("class"))
            .collect()
    }

    /// Calls `f` with a SENTINEL-filled out-buffer and asserts nothing wrote
    /// to it.
    ///
    /// Zeroes would not do: an implementation that memset the whole struct
    /// would satisfy `data == null`, and that is a real bug this must see.
    fn refused(want: i32, f: impl FnOnce(*mut DomowikBuf) -> i32) {
        let mut buf = DomowikBuf {
            data: 0xA5A5_A5A5_usize as *mut u8,
            len: 0x5A5A_5A5A,
            token: 0x1234,
        };
        assert_eq!(f(&mut buf), want);
        assert_eq!(buf.data as usize, 0xA5A5_A5A5, "out-parameter was written");
        assert_eq!(buf.len, 0x5A5A_5A5A);
        assert_eq!(buf.token, 0x1234);
    }

    /// R7.1.3's precedence, through the boundary: a join node outranks every
    /// wall region it sits inside, and the joined member endpoints do NOT
    /// appear as free endpoints of their own (R6.3.2).
    #[test]
    fn a_hit_test_ranks_a_join_node_above_the_regions_it_sits_in() {
        let (h, level) = plan_with_a_room();
        // Drawn through the gesture path, which is what MAKES the join: the
        // raw operations `plan_with_a_room` uses create no join entity.
        let req = serde_json::to_vec(&serde_json::json!({
            "kind": "addWall", "levelId": level,
            "start": [4000.0, 3000.0], "end": [4000.0, 6000.0],
            "thicknessMm": 200.0, "heightMm": 2500.0
        }))
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_edit(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        // 12 logical px at the default zoom of 0.1 px/mm.
        let hits = hits_at(h, level, 4000.0, 3000.0, 120.0);
        let seen = classes(&hits);
        assert_eq!(seen[0], "join-node", "top-ranked candidate: {seen:?}");
        assert!(
            seen[1..].iter().all(|c| *c == "region"),
            "the three walls meeting here contribute regions only: {seen:?}"
        );
        assert_eq!(hits.len(), 4, "{seen:?}");
        assert_eq!(hits[0]["distanceSquaredMm2"], 0.0);
        assert_eq!(
            hits[0]["closestMm"],
            serde_json::json!([4000.0, 3000.0]),
            "the join node reports its own point"
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// R7.1.3: a non-active level is never hit. `levelId` is a FILTER, so a
    /// level that exists but holds nothing answers empty rather than
    /// answering with the neighbouring level's walls.
    #[test]
    fn a_hit_test_sees_only_the_level_it_was_asked_about() {
        let (h, level) = plan_with_a_room();
        let other = Level::new("First floor");
        let other_id = other.id;
        let op = Operation::create(Entity::Level(other));
        let req = serde_json::to_vec(&serde_json::json!({ "operations": [op] })).unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        assert!(
            hits_at(h, other_id, 0.0, 0.0, 120.0).is_empty(),
            "the empty level has no candidates"
        );
        assert!(
            !hits_at(h, level, 0.0, 0.0, 120.0).is_empty(),
            "the level with the room does"
        );
        // Indexes are kept per level, so alternating levels at one revision
        // refreshes nothing.
        let builds = domowik_debug_hit_index_builds(h);
        hits_at(h, other_id, 0.0, 0.0, 120.0);
        hits_at(h, level, 0.0, 0.0, 120.0);
        assert_eq!(domowik_debug_hit_index_builds(h), builds);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// A zero radius is legal and is the tightest boundary case: it finds
    /// exactly what the point touches, at exact distance zero.
    #[test]
    fn a_zero_radius_hit_test_finds_only_what_the_point_touches() {
        let (h, level) = plan_with_a_room();
        // The corner two walls share. No join entity exists, so both
        // endpoints are FREE, and the point lies on both rectangles.
        let corner = hits_at(h, level, 0.0, 0.0, 0.0);
        assert_eq!(
            classes(&corner),
            ["free-endpoint", "free-endpoint", "region", "region"]
        );
        for hit in &corner {
            assert_eq!(hit["distanceSquaredMm2"], 0.0, "{hit}");
        }
        // Inside the room, clear of every 200 mm wall: nothing is touched.
        assert!(hits_at(h, level, 2000.0, 1500.0, 0.0).is_empty());
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// The radius is bounded by the widest radius a legal viewport can ask
    /// for: 24 logical px at the 0.0001 px/mm zoom floor.
    #[test]
    fn the_hit_radius_is_bounded_by_the_widest_legal_viewport() {
        let (h, level) = plan_with_a_room();
        let at_the_limit = hit_request(level, 0.0, 0.0, MAX_HIT_RADIUS_MM);
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_hit_test(h, at_the_limit.as_ptr(), at_the_limit.len() as u64, out)
        });
        assert_eq!(code, status::OK, "the inclusive limit is accepted");

        for radius in [
            MAX_HIT_RADIUS_MM + 1.0,
            -1.0,
            -0.0001,
            f64::INFINITY,
            f64::NAN,
        ] {
            // `json!` renders a non-finite f64 as `null`, which serde refuses
            // for an f64 field, so these arrive as malformed either way — the
            // point is that neither reaches the index.
            let req = hit_request(level, 0.0, 0.0, radius);
            refused(status::MALFORMED_REQUEST, |out| unsafe {
                domowik_plan_hit_test(h, req.as_ptr(), req.len() as u64, out)
            });
        }

        // Overflow written as JSON TEXT, which `json!` cannot express: this is
        // the shape a real caller sends, and it must not become `inf` inside.
        for text in [
            format!("{{\"levelId\":\"{level}\",\"pointMm\":[0,0],\"radiusMm\":1e400}}"),
            format!("{{\"levelId\":\"{level}\",\"pointMm\":[1e400,0],\"radiusMm\":120}}"),
            format!("{{\"levelId\":\"{level}\",\"pointMm\":[0,-1e400],\"radiusMm\":120}}"),
        ] {
            let bytes = text.into_bytes();
            refused(status::MALFORMED_REQUEST, |out| unsafe {
                domowik_plan_hit_test(h, bytes.as_ptr(), bytes.len() as u64, out)
            });
        }
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// A finite point far outside the plan domain is a normal consequence of
    /// panning, so it is answered — with nothing, and without building or
    /// indexing anything.
    #[test]
    fn a_point_outside_the_plan_domain_is_empty_and_builds_no_index() {
        let (h, level) = plan_with_a_room();
        assert_eq!(domowik_debug_hit_index_builds(h), 0, "nothing built yet");

        let reach = DERIVED_COORD_BOUND_MM + 120.0;
        for (x, y) in [
            (reach + 1.0, 0.0),
            (-reach - 1.0, 0.0),
            (0.0, reach + 1.0),
            (0.0, -reach - 1.0),
            (f64::MAX, f64::MAX),
        ] {
            assert!(hits_at(h, level, x, y, 120.0).is_empty(), "({x}, {y})");
        }
        assert_eq!(
            domowik_debug_hit_index_builds(h),
            0,
            "an out-of-domain query did index work"
        );

        // Exactly AT the reach is inside the domain: the cull is strict, so
        // this one is answered by the index rather than short-circuited.
        assert!(hits_at(h, level, reach, 0.0, 120.0).is_empty());
        assert_eq!(domowik_debug_hit_index_builds(h), 1);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// The cache contract: reads and saves keep the index, edits drop it.
    #[test]
    fn the_hit_index_survives_reads_and_saves_but_not_edits() {
        let (h, level) = plan_with_a_room();
        let id = std::ffi::CString::new(level.to_string()).unwrap();
        let probe = || hits_at(h, level, 0.0, 0.0, 120.0);

        let expected = probe();
        probe();
        probe();
        assert_eq!(
            domowik_debug_hit_index_builds(h),
            1,
            "three queries, one build"
        );

        let mutate = create_level_request();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, mutate.as_ptr(), mutate.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        probe();
        assert_eq!(domowik_debug_hit_index_builds(h), 2, "an edit drops it");

        for redo in [false, true] {
            let before = domowik_debug_hit_index_builds(h);
            let (code, _) = call_returning(|out| unsafe { change_history(h, out, redo) });
            assert_eq!(code, status::OK);
            probe();
            assert_eq!(
                domowik_debug_hit_index_builds(h),
                before + 1,
                "undo/redo (redo={redo}) drops it"
            );
        }

        // READS. None of these appends to the log, so none may cost a rebuild.
        let held = domowik_debug_hit_index_builds(h);
        call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        call_returning(|out| unsafe { domowik_plan_state(h, out) });
        call_returning(|out| unsafe { domowik_plan_derived(h, id.as_ptr(), out) });
        probe();
        assert_eq!(domowik_debug_hit_index_builds(h), held, "a read rebuilt it");

        // SAVE. `Plan::save` writes the package and clears dirty; it appends
        // no batch, so the index is still describing the same document.
        let dir = std::env::temp_dir().join(format!("domowik-hit-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = std::ffi::CString::new(dir.join("cache.domo").to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert_eq!(
            probe(),
            expected,
            "the answer must not change across a save"
        );
        assert_eq!(domowik_debug_hit_index_builds(h), held, "a save rebuilt it");

        assert_eq!(domowik_plan_close(h), status::OK);
        assert_eq!(
            domowik_debug_hit_index_builds(h),
            -1,
            "a closed handle has no counter"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The cache is keyed off the ACTIVE session, wherever it lives.
    ///
    /// A reopened plan keeps its session inside the container and leaves
    /// `Plan::session` an empty stand-in. Keying off that stand-in would pin
    /// every reopened plan's generation at zero, and the first edit after a
    /// reopen would then be answered from a stale index — a wrong hit, not a
    /// slow one. This is the same live-session confusion issue 0042 fixed in
    /// the summary counts.
    #[test]
    fn a_reopened_plan_hit_tests_from_its_live_session() {
        let (h, level) = plan_with_a_room();
        let before = hits_at(h, level, 0.0, 0.0, 120.0);
        assert!(!before.is_empty());

        let dir = std::env::temp_dir().join(format!("domowik-hit-live-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = std::ffi::CString::new(dir.join("live.domo").to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert_eq!(hits_at(h, level, 0.0, 0.0, 120.0), before);
        assert_eq!(domowik_plan_close(h), status::OK);

        let mut reopened = 0u64;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut reopened) },
            status::OK
        );
        assert_eq!(
            hits_at(reopened, level, 0.0, 0.0, 120.0),
            before,
            "the reopened plan answers the same"
        );

        // Room centre: empty before the edit, occupied after it. A cache keyed
        // off the wrong session would still answer empty here.
        assert!(hits_at(reopened, level, 2000.0, 1500.0, 120.0).is_empty());
        let builds = domowik_debug_hit_index_builds(reopened);
        let wall = serde_json::to_vec(&serde_json::json!({
            "kind": "addWall", "levelId": level,
            "start": [1000.0, 1500.0], "end": [3000.0, 1500.0],
            "thicknessMm": 200.0, "heightMm": 2500.0
        }))
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_edit(reopened, wall.as_ptr(), wall.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        let after = hits_at(reopened, level, 2000.0, 1500.0, 120.0);
        assert_eq!(classes(&after), ["region"], "the new wall is hit");
        assert_eq!(
            domowik_debug_hit_index_builds(reopened),
            builds + 1,
            "the edit invalidated the reopened plan's index"
        );
        assert_eq!(domowik_plan_close(reopened), status::OK);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The response buffer is the CALLER's, released by token exactly once.
    #[test]
    fn a_hit_test_response_buffer_belongs_to_the_caller() {
        let (h, level) = plan_with_a_room();
        let req = hit_request(level, 0.0, 0.0, 120.0);
        let mut buf = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        let code = unsafe { domowik_plan_hit_test(h, req.as_ptr(), req.len() as u64, &mut buf) };
        assert_eq!(code, status::OK);
        assert!(!buf.data.is_null() && buf.len > 0 && buf.token != 0);
        let bytes = unsafe { std::slice::from_raw_parts(buf.data, buf.len as usize) }.to_vec();
        assert_eq!(bytes.first(), Some(&b'['), "the response is an array");
        assert_eq!(domowik_buffer_free(buf.token), status::OK);
        assert_eq!(
            domowik_buffer_free(buf.token),
            status::UNKNOWN_BUFFER,
            "a double free is refused, never performed"
        );

        // The empty result travels the same path and is freed the same way.
        let far = hit_request(level, DERIVED_COORD_BOUND_MM * 2.0, 0.0, 120.0);
        let mut empty = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        assert_eq!(
            unsafe { domowik_plan_hit_test(h, far.as_ptr(), far.len() as u64, &mut empty) },
            status::OK
        );
        assert_eq!(empty.len, 2);
        assert_ne!(empty.token, buf.token, "tokens are never reused");
        assert_eq!(domowik_buffer_free(empty.token), status::OK);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// Bad arguments are DETECTED and reported, and none of them writes to the
    /// out-parameter or leaves the plan changed.
    #[test]
    fn a_hit_test_refuses_bad_arguments_without_touching_the_out_parameter() {
        let (h, level) = plan_with_a_room();
        let good = hit_request(level, 0.0, 0.0, 120.0);

        for forged in [0u64, 999_999, 0xdead_beef, u64::MAX] {
            refused(status::UNKNOWN_HANDLE, |out| unsafe {
                domowik_plan_hit_test(forged, good.as_ptr(), good.len() as u64, out)
            });
        }
        assert_eq!(
            unsafe {
                domowik_plan_hit_test(h, good.as_ptr(), good.len() as u64, std::ptr::null_mut())
            },
            status::INVALID_ARGUMENT,
            "a null out-parameter is detected, not written through"
        );
        refused(status::INVALID_ARGUMENT, |out| unsafe {
            domowik_plan_hit_test(h, std::ptr::null(), 16, out)
        });

        for bad in [
            "{not json".to_owned(),
            "{}".to_owned(),
            "{\"levelId\":\"not-a-uuid\",\"pointMm\":[0,0],\"radiusMm\":1}".to_owned(),
            format!("{{\"levelId\":\"{level}\",\"pointMm\":[0],\"radiusMm\":1}}"),
            format!("{{\"levelId\":\"{level}\",\"pointMm\":\"0,0\",\"radiusMm\":1}}"),
        ] {
            let bytes = bad.clone().into_bytes();
            refused(status::MALFORMED_REQUEST, |out| unsafe {
                domowik_plan_hit_test(h, bytes.as_ptr(), bytes.len() as u64, out)
            });
            let (code, message) = call_returning(|out| unsafe { domowik_last_error(h, out) });
            assert_eq!(code, status::OK);
            assert!(!message.is_empty(), "no reason recorded for {bad}");
        }

        // An unknown LEVEL is not an error, for the reason `plan_derived`
        // gives: a level with nothing on it has no candidates.
        assert!(hits_at(h, Uuid::now_v7(), 0.0, 0.0, 120.0).is_empty());

        // Nothing above changed the document.
        assert!(!hits_at(h, level, 0.0, 0.0, 120.0).is_empty());
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    // -----------------------------------------------------------------------
    // Marquee (issue 0059)
    // -----------------------------------------------------------------------

    fn marquee_request(level: Uuid, c1: (f64, f64), c2: (f64, f64)) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "levelId": level,
            "corner1Mm": [c1.0, c1.1],
            "corner2Mm": [c2.0, c2.1],
        }))
        .expect("encodes")
    }

    /// The ordered entity-id response, or a panic. Frees the buffer through
    /// the ABI.
    fn marquee_at(h: u64, level: Uuid, c1: (f64, f64), c2: (f64, f64)) -> Vec<String> {
        let req = marquee_request(level, c1, c2);
        let (code, body) = call_returning(|out| unsafe {
            domowik_plan_marquee(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK, "marquee {c1:?}..{c2:?}");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("an object");
        value["entityIds"]
            .as_array()
            .expect("entityIds array")
            .iter()
            .map(|v| v.as_str().expect("string id").to_owned())
            .collect()
    }

    /// Original PH1/PH2 package, with explicit requirement and optional declared extent.
    fn placeholder_plan(
        required: bool,
        declared: bool,
        content: Option<&[u8]>,
        media: &str,
    ) -> (u64, Uuid, Uuid) {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let digest = content
            .map(domowik_container::digest_of)
            .unwrap_or_else(|| format!("sha256:{}", "0".repeat(64)));
        let mut instance = domowik_format::AssetInstance {
            id: Uuid::now_v7(),
            level_id: level,
            digest: digest.clone(),
            transform: domowik_format::InstanceTransform {
                translation_mm: [2000.0, 0.0],
                ..Default::default()
            },
            display_name: None,
            unknown: Default::default(),
        };
        let id = instance.id;
        instance.transform.scale_uniform = 2.0;
        instance.transform.rotation_deg = 90.0;
        doc.asset_instances.push(instance);
        let session =
            Session::open_snapshot(Uuid::now_v7(), doc, &Default::default(), &[], vec![]).unwrap();
        let mut file = ContainerPlan::unsaved(session);
        let mut descriptor = serde_json::json!({
            "digest": digest, "byteLength": content.map_or(1, |b| b.len()), "mediaType": media,
            "purpose": "furniture-model", "encoding": "original", "requirement": if required { "required" } else { "optional" }, "provenance": {}
        });
        if declared {
            descriptor["placeholderDimensions"] =
                serde_json::json!({"widthMm":600.0,"heightMm":2100.0,"depthMm":400.0});
        }
        file.insert_descriptor(&digest, descriptor).unwrap();
        if let Some(bytes) = content {
            file.assets.insert(digest, bytes.to_vec());
        }
        let dir = std::env::temp_dir().join(format!("domowik-ph1-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ph1.domo");
        file.save_as(&path).unwrap();
        let cpath = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        let mut h = 0;
        assert_eq!(
            unsafe { domowik_plan_open(cpath.as_ptr(), &mut h) },
            status::OK
        );
        std::fs::remove_dir_all(dir).unwrap();
        (h, level, id)
    }

    fn delete_placeholder(h: u64, id: Uuid) -> i32 {
        let req =
            serde_json::to_vec(&serde_json::json!({"kind":"delete","instanceId":id})).unwrap();
        call_returning(|out| unsafe { domowik_plan_assets(h, req.as_ptr(), req.len() as u64, out) })
            .0
    }

    #[test]
    fn ph1_and_ph2_select_delete_and_undo_through_the_native_bridge() {
        let (h, level, id) = placeholder_plan(false, false, None, "model/obj");
        let expected = vec![id.to_string()];
        let before = call_returning(|out| unsafe { domowik_plan_state(h, out) }).1;
        assert_eq!(
            marquee_at(h, level, (1400.0, -600.0), (2600.0, 600.0)),
            expected
        );
        assert_eq!(
            marquee_at(h, level, (1000.0, -100.0), (1500.0, 100.0)),
            expected
        );
        assert!(marquee_at(h, level, (1000.0, -100.0), (1499.0, 100.0)).is_empty());
        assert_eq!(marquee_at(h, level, (2000.0, 0.0), (2000.0, 0.0)), expected);
        assert!(marquee_at(h, Uuid::now_v7(), (1400.0, -600.0), (2600.0, 600.0)).is_empty());
        let hits = hits_at(h, level, 2000.0, 0.0, 0.0);
        assert_eq!(hits[0]["class"], "placeholder-box");
        assert_eq!(hits[0]["entityId"], id.to_string());
        assert_eq!(
            before,
            call_returning(|out| unsafe { domowik_plan_state(h, out) }).1
        );
        assert_eq!(
            delete_placeholder(h, id),
            status::OK,
            "{}",
            resolve(h).unwrap().0.lock().unwrap().last_error
        );
        assert!(marquee_at(h, level, (1400.0, -600.0), (2600.0, 600.0)).is_empty());
        assert!(hits_at(h, level, 2000.0, 0.0, 0.0).is_empty());
        assert_eq!(
            call_returning(|out| unsafe { domowik_plan_undo(h, out) }).0,
            status::OK
        );
        assert_eq!(
            marquee_at(h, level, (1400.0, -600.0), (2600.0, 600.0)),
            expected
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn declared_placeholder_bounds_and_required_read_only_inspection_cross_the_bridge() {
        let (h, level, id) = placeholder_plan(true, true, None, "model/obj");
        let before = call_returning(|out| unsafe { domowik_plan_state(h, out) }).1;
        // Width/depth 600/400, scale 2, rotation 90 gives 800/1200.
        assert_eq!(
            marquee_at(h, level, (1599.0, 550.0), (1601.0, 590.0)),
            vec![id.to_string()]
        );
        assert!(marquee_at(h, level, (1500.0, 0.0), (1599.0, 100.0)).is_empty());
        assert_eq!(
            hits_at(h, level, 2000.0, 550.0, 0.0)[0]["entityId"],
            id.to_string()
        );
        assert_eq!(delete_placeholder(h, id), status::REJECTED);
        let path = std::env::temp_dir().join(format!(
            "domowik-required-placeholder-{}.domo",
            Uuid::now_v7()
        ));
        let cpath = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        assert_eq!(
            unsafe { domowik_plan_save(h, cpath.as_ptr()) },
            status::REJECTED
        );
        assert!(!path.exists(), "normal save must not write while read-only");
        assert_eq!(
            before,
            call_returning(|out| unsafe { domowik_plan_state(h, out) }).1
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn resolved_unsupported_and_undecodable_instances_have_distinct_candidacy() {
        let obj = b"v 0 0 0\nv 1 0 0\nv 0 0 1\nf 1 2 3\n";
        for (content, media, degraded) in [
            (obj.as_slice(), "model/obj", false),
            (obj.as_slice(), "application/x-unsupported", true),
            (b"invalid model".as_slice(), "model/obj", true),
        ] {
            let (h, level, id) = placeholder_plan(false, false, Some(content), media);
            let ids = marquee_at(h, level, (1400.0, -600.0), (2600.0, 600.0));
            // Issue 0111: all instances participate in marquee exactly once.
            // Only the candidacy class differs: resolved footprint versus
            // degraded placeholder. Geometry point hits still exclude resolved
            // instances, whose click hits come from the separate asset query.
            assert_eq!(ids, vec![id.to_string()]);
            assert_eq!(!hits_at(h, level, 2000.0, 0.0, 0.0).is_empty(), degraded);
            assert_eq!(domowik_plan_close(h), status::OK);
        }
        let (h, level, _) = placeholder_plan(true, false, Some(b"invalid model"), "model/obj");
        assert!(!marquee_at(h, level, (1400.0, -600.0), (2600.0, 600.0)).is_empty());
        let id = resolve(h)
            .unwrap()
            .0
            .lock()
            .unwrap()
            .session()
            .document()
            .asset_instances[0]
            .id;
        assert_eq!(delete_placeholder(h, id), status::REJECTED);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// R7.3.1: tangential contact selects. Mirrors BEHAVIOR-SPEC fixture L2 —
    /// a rectangle whose lower edge lies exactly on the wall's top face.
    #[test]
    fn a_marquee_selects_a_wall_it_only_touches() {
        let (h, level) = plan_with_a_room();
        // The room's first wall is (0,0)-(4000,0), t=200: band y in [-100,100].
        let ids = marquee_at(h, level, (-1000.0, 100.0), (5000.0, 600.0));
        let (_, body) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let wall_id = doc["walls"][0]["id"].as_str().unwrap();
        assert!(
            ids.iter().any(|id| id == wall_id),
            "a tangent rectangle must still select the wall, got {ids:?}"
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn a_marquee_answers_the_same_regardless_of_corner_order() {
        let (h, level) = plan_with_a_room();
        let forward = marquee_at(h, level, (-1000.0, -1000.0), (5000.0, 4000.0));
        let reversed = marquee_at(h, level, (5000.0, 4000.0), (-1000.0, -1000.0));
        assert_eq!(forward, reversed);
        assert_eq!(forward.len(), 4, "all four room walls");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// R7.3.2's marquee-key order (min-corner x, then y, then class, then
    /// stable id) must survive the JSON round trip, not fall back to
    /// creation/document order.
    #[test]
    fn a_marquee_returns_ascending_marquee_key_order() {
        let h = open_plan();
        let level = Level::new("Ground");
        let level_id = level.id;
        let level_req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Level(level))] }),
        )
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, level_req.as_ptr(), level_req.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        // Three parallel, non-overlapping walls placed in DESCENDING x order,
        // so document order and marquee-key order disagree unless the
        // response is genuinely re-ordered.
        let mut ids = Vec::new();
        for x in [2000.0, 1000.0, 0.0] {
            let wall = domowik_format::Wall::new(
                level_id,
                domowik_format::Point::new(x, 0.0),
                domowik_format::Point::new(x, 500.0),
                20.0,
                2500.0,
            );
            ids.push(wall.id.to_string());
            let req = serde_json::to_vec(
                &serde_json::json!({ "operations": [Operation::create(Entity::Wall(wall))] }),
            )
            .unwrap();
            let (code, _) = call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            });
            assert_eq!(code, status::OK);
        }
        let got = marquee_at(h, level_id, (-100.0, -100.0), (2100.0, 600.0));
        // Ascending x: the wall at x=0 first, x=2000 last — the REVERSE of
        // creation order.
        assert_eq!(got, vec![ids[2].clone(), ids[1].clone(), ids[0].clone()]);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn a_marquee_that_misses_answers_empty_not_refused() {
        let (h, level) = plan_with_a_room();
        let ids = marquee_at(h, level, (100_000.0, 100_000.0), (100_100.0, 100_100.0));
        assert!(ids.is_empty());
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// R7.5.4/R7.1.3: only the named level's entities are eligible, even when
    /// another level's wall occupies identical XY.
    #[test]
    fn a_marquee_never_selects_across_levels() {
        let h = open_plan();
        let level_a = Level::new("A");
        let level_a_id = level_a.id;
        let level_b = Level::new("B");
        let level_b_id = level_b.id;
        for level in [level_a, level_b] {
            let req = serde_json::to_vec(
                &serde_json::json!({ "operations": [Operation::create(Entity::Level(level))] }),
            )
            .unwrap();
            let (code, _) = call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            });
            assert_eq!(code, status::OK);
        }
        for level_id in [level_a_id, level_b_id] {
            let wall = domowik_format::Wall::new(
                level_id,
                domowik_format::Point::new(0.0, 0.0),
                domowik_format::Point::new(4000.0, 0.0),
                200.0,
                2500.0,
            );
            let req = serde_json::to_vec(
                &serde_json::json!({ "operations": [Operation::create(Entity::Wall(wall))] }),
            )
            .unwrap();
            let (code, _) = call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            });
            assert_eq!(code, status::OK);
        }
        let ids = marquee_at(h, level_a_id, (-1000.0, -1000.0), (5000.0, 1000.0));
        assert_eq!(ids.len(), 1, "only level A's identically-placed wall");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// R7.1.2: selection is local view state, never document history. This
    /// read-only query must not advance the revision or either undo/redo
    /// stack.
    #[test]
    fn a_marquee_is_read_only() {
        let (h, level) = plan_with_a_room();
        let before = revision_of(h);
        let before_state = state_of(h);
        let _ = marquee_at(h, level, (-1000.0, -1000.0), (5000.0, 4000.0));
        assert_eq!(
            revision_of(h),
            before,
            "marquee must not advance the revision"
        );
        let after_state = state_of(h);
        assert_eq!(after_state["undo_depth"], before_state["undo_depth"]);
        assert_eq!(after_state["redo_depth"], before_state["redo_depth"]);
        assert_eq!(after_state["dirty"], before_state["dirty"]);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    #[test]
    fn a_marquee_refuses_bad_arguments_without_touching_the_out_parameter() {
        let (h, level) = plan_with_a_room();
        let good = marquee_request(level, (-1000.0, -1000.0), (5000.0, 4000.0));

        for forged in [0u64, 999_999, 0xdead_beef, u64::MAX] {
            refused(status::UNKNOWN_HANDLE, |out| unsafe {
                domowik_plan_marquee(forged, good.as_ptr(), good.len() as u64, out)
            });
        }
        assert_eq!(
            unsafe {
                domowik_plan_marquee(h, good.as_ptr(), good.len() as u64, std::ptr::null_mut())
            },
            status::INVALID_ARGUMENT,
            "a null out-parameter is detected, not written through"
        );
        refused(status::INVALID_ARGUMENT, |out| unsafe {
            domowik_plan_marquee(h, std::ptr::null(), 16, out)
        });

        for bad in [
            "{not json".to_owned(),
            "{}".to_owned(),
            "{\"levelId\":\"not-a-uuid\",\"corner1Mm\":[0,0],\"corner2Mm\":[1,1]}".to_owned(),
            format!("{{\"levelId\":\"{level}\",\"corner1Mm\":[0],\"corner2Mm\":[1,1]}}"),
            // Syntactically valid JSON numbers that overflow binary64 to a
            // non-finite value: the SAME technique the hit-test refusal
            // tests use for its point/radius, applied to a marquee corner.
            format!("{{\"levelId\":\"{level}\",\"corner1Mm\":[1e400,0],\"corner2Mm\":[1,1]}}"),
            format!("{{\"levelId\":\"{level}\",\"corner1Mm\":[0,0],\"corner2Mm\":[1,-1e400]}}"),
        ] {
            let bytes = bad.clone().into_bytes();
            refused(status::MALFORMED_REQUEST, |out| unsafe {
                domowik_plan_marquee(h, bytes.as_ptr(), bytes.len() as u64, out)
            });
            let (code, message) = call_returning(|out| unsafe { domowik_last_error(h, out) });
            assert_eq!(code, status::OK);
            assert!(!message.is_empty(), "no reason recorded for {bad}");
        }

        // An unknown LEVEL is not an error: a level with nothing on it has no
        // candidates, same as hit-test and plan_derived.
        assert!(marquee_at(h, Uuid::now_v7(), (-1000.0, -1000.0), (5000.0, 4000.0)).is_empty());

        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// Numeric caveat, not a fix: corners at extreme-but-finite opposite
    /// values can make `Aabb::width()`/`height()` overflow to infinity.
    /// `domowik_geometry::hit::marquee` has no domain-bound check of its own
    /// (unlike `axis_contact`'s COORD_BOUND_MM guard elsewhere) — this
    /// characterizes what actually happens rather than assuming it is safe,
    /// and rather than inventing a new clamp/tolerance at this boundary.
    /// Finiteness alone is what R7.3.2 requires and what this bridge checks.
    #[test]
    fn a_marquee_at_extreme_finite_corners_does_not_panic() {
        let (h, level) = plan_with_a_room();
        let req = marquee_request(level, (-f64::MAX, -f64::MAX), (f64::MAX, f64::MAX));
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_marquee(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_ne!(
            code,
            status::PANIC,
            "extreme finite corners must not panic the core query"
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// Opens a fresh plan with one level and one wall from `start` to `end`,
    /// and returns (handle, levelId, wallId).
    fn open_level_with_wall(
        start: (f64, f64),
        end: (f64, f64),
        thickness_mm: f64,
    ) -> (u64, Uuid, String) {
        let h = open_plan();
        let level = Level::new("Ground");
        let level_id = level.id;
        let level_req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Level(level))] }),
        )
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, level_req.as_ptr(), level_req.len() as u64, out)
        });
        assert_eq!(code, status::OK);

        let wall = domowik_format::Wall::new(
            level_id,
            domowik_format::Point::new(start.0, start.1),
            domowik_format::Point::new(end.0, end.1),
            thickness_mm,
            2500.0,
        );
        let wall_id = wall.id.to_string();
        let req = serde_json::to_vec(
            &serde_json::json!({ "operations": [Operation::create(Entity::Wall(wall))] }),
        )
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        (h, level_id, wall_id)
    }

    /// Regression guard for the core overflow fix in progress (issue 0012,
    /// tracked separately from this issue): a query band this narrow (1mm)
    /// combined with corners at the extremes of the finite range must still
    /// find a wall that spans the whole band. These corners are passed
    /// straight through to `domowik_geometry::hit::marquee`, unmodified —
    /// the bridge does NOT clip or otherwise narrow them; this test
    /// exercises whatever the core currently does, and is expected to start
    /// passing once that fix lands, not before.
    #[test]
    fn a_marquee_with_extreme_finite_bounds_still_finds_a_wall_it_crosses() {
        let (h, level_id, wall_id) = open_level_with_wall((0.0, 0.0), (4000.0, 0.0), 200.0);
        let ids = marquee_at(h, level_id, (2000.0, -1e308), (2001.0, 1e308));
        assert!(
            ids.iter().any(|id| id == &wall_id),
            "a 1mm-wide band at x in [2000,2001], with corners at the \
             extremes of the finite range, must still find the wall it \
             crosses; got {ids:?}"
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// Transpose of the above: the narrow band runs on y instead of x, and
    /// the wall is vertical instead of horizontal.
    #[test]
    fn a_marquee_with_extreme_finite_bounds_still_finds_a_wall_it_crosses_transposed() {
        let (h, level_id, wall_id) = open_level_with_wall((0.0, 0.0), (0.0, 4000.0), 200.0);
        let ids = marquee_at(h, level_id, (-1e308, 2000.0), (1e308, 2001.0));
        assert!(
            ids.iter().any(|id| id == &wall_id),
            "transposed: a 1mm-wide band at y in [2000,2001], with corners \
             at the extremes of the finite range, must still find the wall \
             it crosses; got {ids:?}"
        );
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    fn revision_of(h: u64) -> u64 {
        state_of(h)["document_revision"]
            .as_u64()
            .expect("document_revision is present and unsigned")
    }

    /// R7.2.3 needs "any accepted document change", which neither the undo
    /// depth nor the dirty flag can answer. This is the field that can.
    #[test]
    fn the_document_revision_counts_accepted_changes_and_nothing_else() {
        let mut h = 0;
        assert_eq!(unsafe { domowik_plan_create_default(&mut h) }, status::OK);
        assert_eq!(revision_of(h), 0, "a new handle starts at its own baseline");

        // READS move nothing, and calling state twice is itself a read.
        assert_eq!(revision_of(h), 0);
        call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        assert_eq!(revision_of(h), 0);

        let req = create_level_request();
        let execute = || {
            call_returning(|out| unsafe {
                domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
            })
        };
        assert_eq!(execute().0, status::OK);
        assert_eq!(revision_of(h), 1, "an accepted execute advances it");

        // UNDO appends a compensating batch, so history moves FORWARD in both
        // directions: this is the case an undo-depth comparison gets wrong,
        // because depth is 1 before the undo and 1 again after the redo.
        let before_undo = state_of(h);
        call_returning(|out| unsafe { domowik_plan_undo(h, out) });
        assert_eq!(revision_of(h), 2, "undo is an accepted change");
        call_returning(|out| unsafe { domowik_plan_redo(h, out) });
        assert_eq!(revision_of(h), 3, "redo is an accepted change");
        assert_eq!(
            state_of(h)["undo_depth"],
            before_undo["undo_depth"],
            "the undo depth is back where it started; the revision is not"
        );

        // A REJECTED command changes nothing, including this.
        let junk = b"{not json";
        let mut sink = DomowikBuf {
            data: std::ptr::null_mut(),
            len: 0,
            token: 0,
        };
        assert_eq!(
            unsafe { domowik_plan_execute(h, junk.as_ptr(), junk.len() as u64, &mut sink) },
            status::MALFORMED_REQUEST
        );
        assert_eq!(revision_of(h), 3, "a refused command is not a change");
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// An empty undo or redo stack is a no-op, not a change.
    #[test]
    fn a_no_op_history_call_does_not_advance_the_revision() {
        let mut h = 0;
        assert_eq!(unsafe { domowik_plan_create_default(&mut h) }, status::OK);
        assert_eq!(revision_of(h), 0, "the baseline is not a batch");
        for redo in [false, true] {
            call_returning(|out| unsafe { change_history(h, out, redo) });
            assert_eq!(revision_of(h), 0, "empty history (redo={redo}) is a no-op");
        }
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// Saving writes the package and clears dirty; it appends no batch, so it
    /// is not a document change. Reopening continues the same log.
    #[test]
    fn saving_does_not_change_the_revision_and_reopening_continues_it() {
        let (h, level) = plan_with_a_room();
        let saved = revision_of(h);
        assert!(saved > 0);

        let dir = std::env::temp_dir().join(format!("domowik-rev-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = std::ffi::CString::new(dir.join("rev.domo").to_str().unwrap()).unwrap();
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        assert_eq!(state_of(h)["dirty"], false, "the save landed");
        assert_eq!(
            revision_of(h),
            saved,
            "a save is not a document change, though it clears dirty"
        );

        // An edit FAR from anything previously queried still counts: R7.2.3
        // says any accepted change, and an entity-id comparison at the old
        // point would see nothing move.
        let far = serde_json::to_vec(&serde_json::json!({
            "kind": "addWall", "levelId": level,
            "start": [500_000.0, 500_000.0], "end": [504_000.0, 500_000.0],
            "thicknessMm": 200.0, "heightMm": 2500.0
        }))
        .unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_edit(h, far.as_ptr(), far.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        assert_eq!(
            revision_of(h),
            saved + 1,
            "a distant edit is still a change"
        );
        assert_eq!(unsafe { domowik_plan_save(h, path.as_ptr()) }, status::OK);
        let final_revision = revision_of(h);
        assert_eq!(domowik_plan_close(h), status::OK);

        let mut reopened = 0u64;
        assert_eq!(
            unsafe { domowik_plan_open(path.as_ptr(), &mut reopened) },
            status::OK
        );
        assert_eq!(
            revision_of(reopened),
            final_revision,
            "opening CONTINUES the log rather than starting a new stream"
        );
        let next = create_level_request();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(reopened, next.as_ptr(), next.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        assert_eq!(revision_of(reopened), final_revision + 1);

        // A handle of its own starts from its own baseline: the number is
        // comparable within a handle, never across two.
        let fresh = open_plan();
        assert_eq!(revision_of(fresh), 0);
        assert_eq!(domowik_plan_close(fresh), status::OK);
        assert_eq!(domowik_plan_close(reopened), status::OK);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The revision and the hit index move together, by construction.
    #[test]
    fn the_revision_advances_exactly_when_the_hit_index_is_dropped() {
        let (h, level) = plan_with_a_room();
        hits_at(h, level, 0.0, 0.0, 120.0);
        let builds = domowik_debug_hit_index_builds(h);
        let revision = revision_of(h);

        let req = create_level_request();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        hits_at(h, level, 0.0, 0.0, 120.0);
        assert_eq!(revision_of(h), revision + 1);
        assert_eq!(domowik_debug_hit_index_builds(h), builds + 1);
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// A plan holding one empty level.
    fn plan_with_a_level() -> (u64, Uuid) {
        let h = open_plan();
        let level = Level::new("Ground");
        let level_id = level.id;
        let op = Operation::create(Entity::Level(level));
        let req = serde_json::to_vec(&serde_json::json!({ "operations": [op] })).unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK);
        (h, level_id)
    }

    /// Commits one wall exactly as given, id included.
    fn commit_wall(h: u64, wall: domowik_format::Wall) {
        let op = Operation::create(Entity::Wall(wall));
        let req = serde_json::to_vec(&serde_json::json!({ "operations": [op] })).unwrap();
        let (code, _) = call_returning(|out| unsafe {
            domowik_plan_execute(h, req.as_ptr(), req.len() as u64, out)
        });
        assert_eq!(code, status::OK, "the fixture wall must be valid");
    }

    /// The response is the CORE's ranking, field for field — not a re-sort.
    ///
    /// IDs and insertion order oppose the expected order. One query resolves
    /// by distance; another lies inside both unequal rectangles, forcing the
    /// canonical-ring key before UUID. Both cross the ABI unchanged.
    #[test]
    fn overlapping_regions_come_back_in_the_cores_order_not_a_re_sort() {
        let (h, level) = plan_with_a_level();
        let far_id = Uuid::parse_str("01940000-0000-7000-8000-000000000001").unwrap();
        let near_id = Uuid::parse_str("0194ffff-ffff-7fff-bfff-ffffffffffff").unwrap();
        assert!(
            near_id > far_id,
            "the fixture must invert id against distance"
        );

        // Two 1000 mm-thick horizontal walls whose rectangles OVERLAP each
        // other in y ∈ [400, 800]. From (0, 0) the first contains the query
        // point (squared distance 0) and the second is 400 mm away (160000).
        let mut near = domowik_format::Wall::new(
            level,
            domowik_format::Point::new(-2000.0, 300.0),
            domowik_format::Point::new(2000.0, 300.0),
            1000.0,
            2500.0,
        );
        near.id = near_id;
        let mut far = domowik_format::Wall::new(
            level,
            domowik_format::Point::new(-2000.0, 900.0),
            domowik_format::Point::new(2000.0, 900.0),
            1000.0,
            2500.0,
        );
        far.id = far_id;
        commit_wall(h, far);
        commit_wall(h, near);

        let (_, snapshot) = call_returning(|out| unsafe { domowik_plan_snapshot(h, out) });
        let doc: Document = serde_json::from_slice(&snapshot).expect("the document");
        for (y, radius) in [(0.0, 500.0), (600.0, 12.0)] {
            // Both radii exclude every endpoint, which would outrank regions.
            let through_abi = hits_at(h, level, 0.0, y, radius);
            assert_eq!(classes(&through_abi), ["region", "region"]);
            assert_eq!(
                through_abi[0]["entityId"].as_str().expect("id"),
                near_id.to_string(),
                "distance or canonical geometry ranks first despite the larger id"
            );
            if y == 600.0 {
                assert_eq!(through_abi[0]["distanceSquaredMm2"], 0.0);
                assert_eq!(through_abi[1]["distanceSquaredMm2"], 0.0);
                assert_eq!(through_abi[0]["closestMm"], through_abi[1]["closestMm"]);
            } else {
                assert!(
                    through_abi[0]["distanceSquaredMm2"].as_f64().expect("d2")
                        < through_abi[1]["distanceSquaredMm2"].as_f64().expect("d2"),
                    "unequal distances, ascending: {through_abi:?}"
                );
            }

            // And the whole array is the core's own output, field for field.
            let direct =
                HitIndex::build(&doc, level).hit_test(domowik_format::Point::new(0.0, y), radius);
            assert_eq!(through_abi.len(), direct.len());
            for (json, hit) in through_abi.iter().zip(direct.iter()) {
                assert_eq!(
                    json["class"].as_str().expect("class"),
                    match hit.class {
                        HitClass::JoinNode => "join-node",
                        HitClass::FreeEndpoint => "free-endpoint",
                        HitClass::Opening => "opening",
                        HitClass::Region => "region",
                        HitClass::PlaceholderBox => "placeholder-box",
                        HitClass::AssetFootprint => "asset-footprint",
                    }
                );
                assert_eq!(
                    json["entityId"].as_str().expect("id"),
                    hit.entity.to_string()
                );
                assert_eq!(
                    json["distanceSquaredMm2"].as_f64().expect("d2"),
                    hit.distance_squared
                );
                assert_eq!(
                    json["closestMm"],
                    serde_json::json!([hit.closest.x, hit.closest.y])
                );
            }
        }
        assert_eq!(domowik_plan_close(h), status::OK);
    }

    /// The domain cull uses the DERIVED bound, not the stored one.
    ///
    /// A wall may lie exactly on `COORD_BOUND_MM`, and its region — derived
    /// geometry, half a thickness either side of the centre line — then
    /// extends past it. Culling at the STORED bound would answer "no hit" for
    /// a click squarely inside such a wall, which is why
    /// `DERIVED_COORD_BOUND_MM` is what this ABI widens by the radius.
    #[test]
    fn a_wall_on_the_stored_bound_is_still_hit_outside_it() {
        let (h, level) = plan_with_a_level();
        let wall = domowik_format::Wall::new(
            level,
            domowik_format::Point::new(COORD_BOUND_MM, -500_000.0),
            domowik_format::Point::new(COORD_BOUND_MM, 500_000.0),
            DIMENSION_CAP_MM,
            2500.0,
        );
        let wall_id = wall.id;
        commit_wall(h, wall);

        // The rectangle spans x ∈ [9.5e6, 1.05e7]. This point is 400 m past
        // the STORED bound and still 100 m inside the wall's own far edge.
        let x = COORD_BOUND_MM + 400_000.0;
        assert!(x > COORD_BOUND_MM && x < DERIVED_COORD_BOUND_MM);
        let hits = hits_at(h, level, x, 0.0, 120.0);
        assert_eq!(
            classes(&hits),
            ["region"],
            "a cull at the stored bound would have dropped this hit"
        );
        assert_eq!(
            hits[0]["entityId"].as_str().expect("id"),
            wall_id.to_string()
        );
        assert_eq!(hits[0]["distanceSquaredMm2"].as_f64().expect("d2"), 0.0);

        // The DERIVED bound still culls: past it nothing can be in reach.
        let builds = domowik_debug_hit_index_builds(h);
        assert!(hits_at(h, level, DERIVED_COORD_BOUND_MM + 121.0, 0.0, 120.0).is_empty());
        assert_eq!(domowik_debug_hit_index_builds(h), builds, "no index work");
        assert_eq!(domowik_plan_close(h), status::OK);
    }
}
