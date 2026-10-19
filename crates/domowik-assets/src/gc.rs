//! Cache garbage collection — ASSET-STRATEGY §10.7.
//!
//! Two protection records, with deliberately different strength:
//!
//! - A **pin** is durable protection. It survives restarts and is released
//!   only by a durable root-set diff or an explicit counterpart action.
//!   Everything that matters for data safety uses pins, because §10.7 G4 says
//!   GC correctness must never be load-bearing: pins cover every root, so a
//!   collector that is simply wrong still cannot destroy reachable data.
//! - A **lease** is advisory and time-bound. It is a courtesy to a holder that
//!   is using a blob right now, and §10.7 says in as many words that
//!   durability-critical protection uses pins only.
//!
//! **Fail-closed is the whole design.** Records carry checksums and a
//! monotonic store-generation counter, both verified at startup. ANY anomaly —
//! a bad checksum, a generation gap or regression, a truncated tail, a record
//! that will not parse — stops collection entirely until a rebuild repairs it.
//! Not "skips the bad record": zero collection. A metadata store that has lost
//! records has lost the evidence that something was protected, and collecting
//! under that condition is exactly the failure the rule exists to prevent.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cache::{CacheError, CacheStore};
use crate::time::parse_rfc3339_utc;

/// §10.7 default grace M for cross-boot lease expiry: 24 hours.
pub const DEFAULT_CROSS_BOOT_GRACE_SECONDS: i64 = 24 * 60 * 60;

/// What kind of root a pin stands for (§10.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RootKind {
    Snapshot,
    OperationFrontier,
    RootTable,
    /// Survives document deregistration, and is released only by an explicit
    /// counterpart action — the `asset` commands are its surface.
    Manual,
}

/// Durable protection for one digest (§10.7).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    pub digest: String,
    pub owner_id: String,
    pub root_kind: RootKind,
    pub created_at: String,
}

/// A durable monotonic anchor, where the platform has one (§10.7).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonoAnchor {
    pub boot_id: String,
    pub mono_deadline_nanos: u128,
}

/// Advisory, time-bound permission (§10.7).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lease {
    pub digest: String,
    pub holder_id: String,
    pub expires_at_utc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<MonoAnchor>,
}

/// How this host measures a lease deadline (§10.7 "platform modes"; observable
/// at collector startup).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformMode {
    /// A durable boot identity plus a monotonic clock. §10.7 names Linux's
    /// `/proc/sys/kernel/random/boot_id` with `CLOCK_MONOTONIC`.
    Monotonic { boot_id: String },
    /// §10.7's default for Windows and macOS: no verified monotonic mechanism
    /// is documented for them, so every lease decision goes through the
    /// cross-boot UTC rule and its grace period.
    UtcOnly,
}

impl PlatformMode {
    /// Detects the mode for this host.
    ///
    /// Deliberately conservative: anything that is not a readable Linux
    /// `boot_id` is `UtcOnly`. Guessing a monotonic mechanism would move
    /// leases onto the faster expiry path on the strength of an assumption,
    /// and §10.7 makes documenting the mechanism the precondition.
    pub fn detect() -> Self {
        match fs::read_to_string("/proc/sys/kernel/random/boot_id") {
            Ok(id) if !id.trim().is_empty() => Self::Monotonic {
                boot_id: id.trim().to_owned(),
            },
            _ => Self::UtcOnly,
        }
    }
}

/// What the collector knows about "now".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClockReading {
    /// Same-boot: a monotonic reading whose boot identity can be compared with
    /// a lease's anchor.
    Monotonic {
        boot_id: String,
        now_mono_nanos: u128,
        now_utc_seconds: i64,
    },
    UtcOnly {
        now_utc_seconds: i64,
    },
}

impl ClockReading {
    fn utc_seconds(&self) -> i64 {
        match self {
            Self::Monotonic {
                now_utc_seconds, ..
            } => *now_utc_seconds,
            Self::UtcOnly { now_utc_seconds } => *now_utc_seconds,
        }
    }
}

/// Whether a lease may be treated as expired — §10.7's expiry decision, kept
/// as a PURE FUNCTION because §10.7 asks for one so fixtures can drive it.
///
/// Same boot, with an anchor: the monotonic deadline decides, and a wall-clock
/// jump cannot touch it. Otherwise: the persisted UTC deadline plus grace `M`
/// decides. §10.7 declares the residual risk openly — a forward wall-clock
/// jump combined with a reboot can collect an active lease early — and bounds
/// the harm by digest-keyed re-admission of identical bytes.
///
/// A malformed `expiresAtUtc` returns `false`: an unreadable deadline is not
/// evidence of expiry, and this function never fails open.
pub fn lease_is_expired(lease: &Lease, now: &ClockReading, grace_seconds: i64) -> bool {
    if let (
        Some(anchor),
        ClockReading::Monotonic {
            boot_id,
            now_mono_nanos,
            ..
        },
    ) = (&lease.anchor, now)
    {
        if &anchor.boot_id == boot_id {
            return *now_mono_nanos > anchor.mono_deadline_nanos;
        }
    }
    match parse_rfc3339_utc(&lease.expires_at_utc) {
        Ok(deadline) => now.utc_seconds() > deadline.saturating_add(grace_seconds),
        Err(_) => false,
    }
}

/// One journal entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum Record {
    AddPin(Pin),
    RemovePin {
        digest: String,
        owner_id: String,
        root_kind: RootKind,
    },
    AddLease(Lease),
    RemoveLease {
        digest: String,
        holder_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JournalLine {
    /// The monotonic store-generation counter (§10.7). Contiguous from 1.
    generation: u64,
    record: Record,
    /// SHA-256 over the canonical JSON of `{generation, record}`.
    checksum: String,
}

fn checksum_of(generation: u64, record: &Record) -> String {
    // serde_json's object order follows the struct field order, and every
    // field here is either a scalar or another such struct, so the encoding is
    // deterministic without a separate canonicaliser.
    let payload = serde_json::json!({ "generation": generation, "record": record });
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(&payload).unwrap_or_default());
    format!("sha256:{:x}", hasher.finalize())
}

/// Why the collector refuses to collect (§10.7 fail-closed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anomaly {
    /// A record's checksum does not cover its content.
    Checksum { line: usize },
    /// The generation counter skipped forward or went backwards.
    Generation {
        line: usize,
        expected: u64,
        found: u64,
    },
    /// A line would not parse — including the partial line a crash mid-append
    /// leaves behind.
    Unparsable { line: usize },
    /// The journal could not be read at all.
    Unreadable { reason: String },
}

impl std::fmt::Display for Anomaly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Checksum { line } => {
                write!(f, "record {line} fails its checksum")
            }
            Self::Generation {
                line,
                expected,
                found,
            } => write!(
                f,
                "record {line} carries generation {found}, but {expected} was expected"
            ),
            Self::Unparsable { line } => write!(f, "record {line} does not parse"),
            Self::Unreadable { reason } => write!(f, "the journal is unreadable: {reason}"),
        }
    }
}

/// Whether the collector may collect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Clean,
    /// Zero collection until a rebuild. §10.7 admits no partial mode.
    FailClosed(Anomaly),
}

const JOURNAL: &str = "meta/journal";
const LOCK: &str = "meta/lock";

/// Exclusive access to a cache's protection records, held for as long as the
/// guard lives.
///
/// §10.7 G2 requires admission and collection to be "mutually atomic against
/// the metadata store". Without that, a pin recorded after a collector has
/// computed its protected set, but before it reaches that digest in the
/// deletion loop, is invisible to the collector — and the blob it was meant to
/// protect is deleted.
///
/// Exclusive-create is the whole mechanism. There is no staleness heuristic
/// and no timeout on purpose: a lock left behind by a killed collector makes
/// the next collection REFUSE, which is over-retention, and the error names
/// the file so a person can decide. A heuristic that broke a lock after some
/// interval would eventually break a live one, and the thing on the other side
/// of that race is a deletion.
#[derive(Debug)]
pub struct RecordsLock {
    path: PathBuf,
}

impl RecordsLock {
    /// Takes the lock, or reports who holds it.
    pub fn acquire(cache_root: &Path) -> Result<Self, CacheError> {
        let path = cache_root.join(LOCK);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| CacheError::Io {
                path: parent.to_owned(),
                source,
            })?;
        }
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", std::process::id());
                Ok(Self { path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(CacheError::Locked { path })
            }
            Err(source) => Err(CacheError::Io { path, source }),
        }
    }
}

impl Drop for RecordsLock {
    fn drop(&mut self) {
        // Best effort. A lock that outlives its holder refuses the next
        // collection, which is the safe direction.
        let _ = fs::remove_file(&self.path);
    }
}

/// The durable protection record store.
#[derive(Debug)]
pub struct MetaStore {
    path: PathBuf,
    generation: u64,
    pins: BTreeSet<Pin>,
    leases: BTreeSet<Lease>,
    health: Health,
    /// Pins found AFTER an anomaly, kept only to feed [`MetaStore::rebuild`].
    ///
    /// Replay stops applying at the first anomaly, because a store that has
    /// lost records has lost the evidence that something was protected. But
    /// stopping the SCAN there as well would throw away every pin recorded
    /// behind the damage, and a rebuild would then write a journal without
    /// them and re-enable collection — deleting exactly the blobs those pins
    /// protected. So the scan continues, salvaging whatever still verifies,
    /// and the salvage feeds only the conservative superset. It never makes
    /// anything collectable, and it never clears fail-closed.
    salvaged: BTreeSet<Pin>,
}

impl MetaStore {
    /// Opens the store under `cache_root`, verifying every record.
    ///
    /// An anomaly does not fail this call. The store still opens, still knows
    /// the pins it managed to read, and still admits new records — it simply
    /// will not collect. Refusing to open would take the cache offline over a
    /// problem whose entire consequence is supposed to be over-retention.
    pub fn open(cache_root: &Path) -> Result<Self, CacheError> {
        let path = cache_root.join(JOURNAL);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| CacheError::Io {
                path: parent.to_owned(),
                source,
            })?;
        }
        let mut store = Self {
            path,
            generation: 0,
            pins: BTreeSet::new(),
            leases: BTreeSet::new(),
            health: Health::Clean,
            salvaged: BTreeSet::new(),
        };
        store.replay();
        Ok(store)
    }

    fn replay(&mut self) {
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                self.health = Health::FailClosed(Anomaly::Unreadable {
                    reason: e.to_string(),
                });
                return;
            }
        };
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            if raw.trim().is_empty() {
                continue;
            }
            // Once an anomaly has been seen, nothing more is APPLIED — the
            // store is fail-closed and its state is not to be trusted. The
            // scan continues anyway, salvaging pins that still verify, so a
            // later rebuild can put them back. Losing them would turn a
            // damaged journal into deleted data.
            let damaged = self.health != Health::Clean;
            let Ok(entry) = serde_json::from_str::<JournalLine>(raw) else {
                self.fail(Anomaly::Unparsable { line });
                continue;
            };
            let expected = self.generation + 1;
            if !damaged && entry.generation != expected {
                self.fail(Anomaly::Generation {
                    line,
                    expected,
                    found: entry.generation,
                });
                continue;
            }
            if checksum_of(entry.generation, &entry.record) != entry.checksum {
                self.fail(Anomaly::Checksum { line });
                self.salvage(entry.record);
                continue;
            }
            if self.health == Health::Clean {
                self.generation = entry.generation;
                self.apply(entry.record);
            } else {
                self.salvage(entry.record);
            }
        }
    }

    fn fail(&mut self, anomaly: Anomaly) {
        if self.health == Health::Clean {
            self.health = Health::FailClosed(anomaly);
        }
    }

    /// Remembers a pin found in a record replay could not trust.
    ///
    /// Deliberately CREDULOUS, including for a record that failed its own
    /// checksum. Salvage feeds only [`MetaStore::rebuild`]'s conservative
    /// superset, so its only possible effect is retaining a blob. Believing a
    /// corrupt record's digest over-retains whatever it names, which §10.7 G1
    /// calls the safe direction; disbelieving it deletes a blob that something
    /// was protecting. Between over-retention and data loss there is no
    /// symmetry to preserve.
    ///
    /// Removals are never salvaged. The generation sequence is exactly what
    /// was lost, so a `RemovePin` behind the damage cannot be shown to
    /// postdate the pin it removes, and honouring it would drop protection on
    /// an ordering nobody can establish.
    fn salvage(&mut self, record: Record) {
        if let Record::AddPin(pin) = record {
            self.salvaged.insert(pin);
        }
    }

    fn apply(&mut self, record: Record) {
        match record {
            Record::AddPin(pin) => {
                self.pins.insert(pin);
            }
            Record::RemovePin {
                digest,
                owner_id,
                root_kind,
            } => {
                self.pins.retain(|p| {
                    !(p.digest == digest && p.owner_id == owner_id && p.root_kind == root_kind)
                });
            }
            Record::AddLease(lease) => {
                self.leases
                    .retain(|l| !(l.digest == lease.digest && l.holder_id == lease.holder_id));
                self.leases.insert(lease);
            }
            Record::RemoveLease { digest, holder_id } => {
                self.leases
                    .retain(|l| !(l.digest == digest && l.holder_id == holder_id));
            }
        }
    }

    fn append(&mut self, record: Record) -> Result<(), CacheError> {
        // Held across the whole append, so a pin cannot land inside a
        // collector's deletion loop (§10.7 G2).
        let lock = RecordsLock::acquire(self.cache_root())?;
        self.append_locked(record, &lock)
    }

    /// Appends under a lock the caller already holds.
    ///
    /// Registering a document writes one record per root, and taking the lock
    /// per record would leave a window between them: a collector running
    /// halfway through would find the later roots unprotected and delete them.
    /// The whole registration is one locked span instead.
    fn append_locked(&mut self, record: Record, _lock: &RecordsLock) -> Result<(), CacheError> {
        // Appending to a journal whose replay stopped early writes a
        // generation number the surviving tail already uses, so the next
        // replay meets a fresh anomaly in a new place. Refusing keeps the
        // damage where it is. Nothing is being collected meanwhile, so there
        // is nothing that urgently needs protecting.
        if let Health::FailClosed(anomaly) = &self.health {
            return Err(CacheError::FailClosed {
                reason: anomaly.to_string(),
            });
        }
        let generation = self.generation + 1;
        let line = JournalLine {
            generation,
            checksum: checksum_of(generation, &record),
            record: record.clone(),
        };
        let encoded = serde_json::to_string(&line).map_err(|e| CacheError::Io {
            path: self.path.clone(),
            source: std::io::Error::other(e),
        })?;
        let io_err = |source| CacheError::Io {
            path: self.path.clone(),
            source,
        };
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(io_err)?;
        // One write, then fsync. §10.7 G1 requires the record to be DURABLY
        // present before the protection it grants is relied on; a buffered
        // write would let a pin be believed and then lost.
        file.write_all(format!("{encoded}\n").as_bytes())
            .map_err(io_err)?;
        file.sync_all().map_err(io_err)?;
        self.generation = generation;
        self.apply(record);
        Ok(())
    }

    pub fn health(&self) -> &Health {
        &self.health
    }

    /// The cache root this store's journal sits under.
    fn cache_root(&self) -> &Path {
        // `path` is `<root>/meta/journal`, so the root is two levels up. It is
        // derived rather than stored so the two can never disagree.
        self.path
            .parent()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."))
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn pins(&self) -> impl Iterator<Item = &Pin> {
        self.pins.iter()
    }

    pub fn leases(&self) -> impl Iterator<Item = &Lease> {
        self.leases.iter()
    }

    pub fn add_pin(&mut self, pin: Pin) -> Result<(), CacheError> {
        self.append(Record::AddPin(pin))
    }

    pub fn remove_pin(
        &mut self,
        digest: &str,
        owner_id: &str,
        root_kind: RootKind,
    ) -> Result<(), CacheError> {
        self.append(Record::RemovePin {
            digest: digest.to_owned(),
            owner_id: owner_id.to_owned(),
            root_kind,
        })
    }

    pub fn add_lease(&mut self, lease: Lease) -> Result<(), CacheError> {
        self.append(Record::AddLease(lease))
    }

    pub fn remove_lease(&mut self, digest: &str, holder_id: &str) -> Result<(), CacheError> {
        self.append(Record::RemoveLease {
            digest: digest.to_owned(),
            holder_id: holder_id.to_owned(),
        })
    }

    /// The root set a document currently contributes (§10.7 G2's "registered
    /// document whose current root set includes it").
    pub fn register_document(
        &mut self,
        owner_id: &str,
        roots: &[(String, RootKind)],
        created_at: &str,
    ) -> Result<(), CacheError> {
        // One locked span for the whole root set. Per-record locking would
        // leave a collector free to run between two roots and take the ones
        // not yet written.
        let lock = RecordsLock::acquire(self.cache_root())?;
        for (digest, root_kind) in roots {
            self.append_locked(
                Record::AddPin(Pin {
                    digest: digest.clone(),
                    owner_id: owner_id.to_owned(),
                    root_kind: *root_kind,
                    created_at: created_at.to_owned(),
                }),
                &lock,
            )?;
        }
        Ok(())
    }

    /// Releases a document's pins, EXCEPT its manual ones.
    ///
    /// §10.7: "`manual` pins survive document deregistration". A user who
    /// pinned a blob by hand did not pin it for the duration of one document's
    /// registration.
    pub fn deregister_document(&mut self, owner_id: &str) -> Result<(), CacheError> {
        let doomed: Vec<Pin> = self
            .pins
            .iter()
            .filter(|p| p.owner_id == owner_id && p.root_kind != RootKind::Manual)
            .cloned()
            .collect();
        let lock = RecordsLock::acquire(self.cache_root())?;
        for pin in doomed {
            self.append_locked(
                Record::RemovePin {
                    digest: pin.digest,
                    owner_id: pin.owner_id,
                    root_kind: pin.root_kind,
                },
                &lock,
            )?;
        }
        Ok(())
    }

    /// Rewrites the journal from a CONSERVATIVE SUPERSET and clears
    /// fail-closed mode (§10.7 "Rebuild").
    ///
    /// The superset is everything currently believed pinned, plus every pin
    /// SALVAGED from behind the damage, plus everything the caller recomputed.
    /// Nothing is dropped: §10.7 requires previously pinned digests belonging
    /// to documents that cannot now be read to stay protected, and this store
    /// cannot tell "the caller knows this is unreachable" from "the caller
    /// could not open the document".
    ///
    /// The salvaged half is not an optimisation. Replay stops applying at the
    /// first anomaly, so without it a rebuild would write a journal missing
    /// every pin recorded behind the damage and then re-enable collection,
    /// which deletes precisely the blobs those pins protected.
    ///
    /// Collection resumes only after this returns cleanly.
    pub fn rebuild(&mut self, recomputed: &[Pin]) -> Result<(), CacheError> {
        // A rebuild REPLACES the journal, so a collector reading it midway
        // would see neither the old set nor the new one.
        let _lock = RecordsLock::acquire(self.cache_root())?;
        let mut superset: BTreeSet<Pin> = self.pins.clone();
        superset.extend(self.salvaged.iter().cloned());
        superset.extend(recomputed.iter().cloned());
        // Leases are NOT carried across a rebuild. They are advisory and
        // time-bound, and a rebuild follows an anomaly in which a lease's own
        // record may be the thing that was lost; re-acquiring is the holder's
        // stated duty under §10.7 and costs a re-admission of identical bytes.
        let io_err = |source| CacheError::Io {
            path: self.path.clone(),
            source,
        };
        let mut body = String::new();
        for (index, pin) in superset.iter().enumerate() {
            let generation = index as u64 + 1;
            let record = Record::AddPin(pin.clone());
            let line = JournalLine {
                generation,
                checksum: checksum_of(generation, &record),
                record,
            };
            body.push_str(&serde_json::to_string(&line).map_err(|e| CacheError::Io {
                path: self.path.clone(),
                source: std::io::Error::other(e),
            })?);
            body.push('\n');
        }
        // Written to a sibling and renamed, so an interrupted rebuild leaves
        // the OLD journal intact rather than a truncated new one. A rebuild
        // that crashed halfway and left nothing would drop protection, which
        // is the one outcome §10.7 does not tolerate.
        let staging = self.path.with_extension("rebuild");
        {
            let mut file = fs::File::create(&staging).map_err(io_err)?;
            file.write_all(body.as_bytes()).map_err(io_err)?;
            file.sync_all().map_err(io_err)?;
        }
        fs::rename(&staging, &self.path).map_err(io_err)?;

        self.pins = superset;
        self.salvaged.clear();
        self.leases.clear();
        self.generation = self.pins.len() as u64;
        self.health = Health::Clean;
        Ok(())
    }
}

/// What one collection pass did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectReport {
    /// Digests whose files were removed, ascending.
    pub collected: Vec<String>,
    /// Digests that were candidates but are protected, ascending.
    pub retained: Vec<String>,
    /// Present when nothing was collected because the store is fail-closed.
    pub fail_closed: Option<Anomaly>,
    /// The mode the collector started in (§10.7: observable at startup).
    pub mode: PlatformMode,
}

/// The §10.7 collector.
pub struct Collector<'a> {
    store: &'a CacheStore,
    meta: &'a mut MetaStore,
    mode: PlatformMode,
    grace_seconds: i64,
}

impl<'a> Collector<'a> {
    pub fn new(store: &'a CacheStore, meta: &'a mut MetaStore, mode: PlatformMode) -> Self {
        Self {
            store,
            meta,
            mode,
            grace_seconds: DEFAULT_CROSS_BOOT_GRACE_SECONDS,
        }
    }

    /// Overrides §10.7's default grace `M`.
    pub fn with_grace_seconds(mut self, grace_seconds: i64) -> Self {
        self.grace_seconds = grace_seconds;
        self
    }

    /// Every digest currently protected by a pin or an unexpired lease.
    pub fn protected(&self, now: &ClockReading) -> BTreeSet<String> {
        let mut set: BTreeSet<String> = self.meta.pins().map(|p| p.digest.clone()).collect();
        for lease in self.meta.leases() {
            if !lease_is_expired(lease, now, self.grace_seconds) {
                set.insert(lease.digest.clone());
            }
        }
        set
    }

    /// Runs one collection pass.
    ///
    /// §10.7 G3: content is never inspected, decoded or rewritten. The only
    /// operation performed on a collected blob is unlinking it — this pass
    /// does not open a single blob file.
    pub fn collect(&mut self, now: &ClockReading) -> Result<CollectReport, CacheError> {
        // Taken BEFORE the protected set is computed and held until the last
        // deletion, so no protection record can appear inside the window
        // between the two (§10.7 G2).
        let _lock = RecordsLock::acquire(self.meta.cache_root())?;
        if let Health::FailClosed(anomaly) = self.meta.health().clone() {
            let retained = self.store.digests()?;
            return Ok(CollectReport {
                collected: Vec::new(),
                retained,
                fail_closed: Some(anomaly),
                mode: self.mode.clone(),
            });
        }
        let protected = self.protected(now);
        let mut collected = Vec::new();
        let mut retained = Vec::new();
        for digest in self.store.digests()? {
            if protected.contains(&digest) {
                retained.push(digest);
            } else if self.store.remove_blob(&digest)? {
                collected.push(digest);
            }
        }
        Ok(CollectReport {
            collected,
            retained,
            fail_closed: None,
            mode: self.mode.clone(),
        })
    }
}
