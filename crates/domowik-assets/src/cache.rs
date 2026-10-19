//! The local content cache — ASSET-STRATEGY §10.6.
//!
//! A digest-keyed store OUTSIDE any package. It is the middle step of §10.6's
//! lookup order (embedded blob → local content cache → policy-gated remote),
//! and the place a thin package's blobs live.
//!
//! **Crash behaviour is by construction, not by hope.** Issue 0029 owns the
//! package's §9.7 atomic replace, and that replace covers exactly one file: it
//! has no cache root and cannot acquire one without becoming a transaction
//! manager across two stores. So this store carries its own argument, the one
//! §10.7 G1/G4 states — a crash yields OVER-RETENTION and never premature
//! deletion:
//!
//! - Bytes are streamed into a staging file under `incoming/`, verified, and
//!   only then renamed to their digest path. A crash mid-admission leaves a
//!   staging file, which is detectable garbage: it is not under a digest name,
//!   so nothing can ever read it as a blob.
//! - A blob therefore never appears at a digest path unless its bytes hashed
//!   to that digest. There is no window in which a truncated file is reachable
//!   by name.
//! - Deletion is the collector's business ([`crate::gc`]) and is gated on
//!   durable protection records, so an interrupted collection over-retains.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Where staging files live before they earn a digest name.
const INCOMING_DIR: &str = "incoming";

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "{digest} is not a well-formed sha256:<64 lowercase hex> digest (ASSET-STRATEGY §10.1)"
    )]
    MalformedDigest { digest: String },
    #[error("admission refused: bytes hash to {actual}, not the expected {expected}")]
    DigestMismatch { expected: String, actual: String },
    #[error("admission refused: {digest} is {actual} bytes, but {declared} were declared")]
    LengthMismatch {
        digest: String,
        declared: u64,
        actual: u64,
    },
    #[error("admission refused: {digest} exceeds the {limit}-byte admission ceiling (L5)")]
    TooLarge { digest: String, limit: u64 },
    #[error(
        "the protection records are fail-closed ({reason}); rebuild them before writing new ones"
    )]
    FailClosed { reason: String },
    #[error("another collection holds {path}; clear it if no collector is running")]
    Locked { path: PathBuf },
}

fn io_at(path: &Path) -> impl FnOnce(io::Error) -> CacheError + '_ {
    move |source| CacheError::Io {
        path: path.to_owned(),
        source,
    }
}

/// What an admission attempt was allowed to write.
///
/// §10.8's verification row names L5 (FORMAT-SPEC §9.3) as the admission
/// ceiling "on all paths" — a cache that streams without a bound turns a
/// hostile or merely enormous asset into a full disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionLimits {
    /// The per-asset byte ceiling. Named `L5` in FORMAT-SPEC §9.3.
    pub max_blob_bytes: u64,
}

/// The result of admitting bytes that were already present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// The bytes were written and renamed into place.
    Stored,
    /// The digest was already present, so nothing was written. §10.6:
    /// "writers deduplicate before write."
    AlreadyPresent,
}

/// A digest-keyed blob store on the filesystem.
#[derive(Debug, Clone)]
pub struct CacheStore {
    root: PathBuf,
    limits: AdmissionLimits,
}

impl CacheStore {
    /// Opens (creating if needed) the store rooted at `root`.
    pub fn open(root: impl Into<PathBuf>, limits: AdmissionLimits) -> Result<Self, CacheError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(io_at(&root))?;
        let incoming = root.join(INCOMING_DIR);
        fs::create_dir_all(&incoming).map_err(io_at(&incoming))?;
        Ok(Self { root, limits })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The on-disk path for `digest`, or `None` when the digest is malformed.
    ///
    /// Shares §10.3's mapping with the package rather than inventing a second
    /// one, so a blob's name is the same in both stores and a thin↔portable
    /// conversion is a move, not a rewrite. `blob_entry_name` rejects anything
    /// that is not fixed-width lowercase hex, which is what keeps a hostile
    /// "digest" from naming a path.
    pub fn path_for(&self, digest: &str) -> Option<PathBuf> {
        let name = domowik_container::blob_entry_name(digest)?;
        Some(self.root.join(name))
    }

    fn require_path(&self, digest: &str) -> Result<PathBuf, CacheError> {
        self.path_for(digest)
            .ok_or_else(|| CacheError::MalformedDigest {
                digest: digest.to_owned(),
            })
    }

    /// Whether the store holds `digest`.
    ///
    /// Presence only — this does NOT re-hash. A caller that needs proof calls
    /// [`CacheStore::verify`]. Saying so matters because §10.6's lookup order
    /// runs on every open and re-hashing every blob there would make opening a
    /// document scale with the size of the cache.
    pub fn contains(&self, digest: &str) -> bool {
        self.path_for(digest).is_some_and(|p| p.is_file())
    }

    /// The stored size of `digest`, or `None` when it is absent.
    pub fn stored_bytes(&self, digest: &str) -> Option<u64> {
        let path = self.path_for(digest)?;
        fs::metadata(path).ok().map(|m| m.len())
    }

    /// Opens `digest` for reading, without hashing it.
    pub fn open_blob(&self, digest: &str) -> Result<Option<fs::File>, CacheError> {
        let path = self.require_path(digest)?;
        match fs::File::open(&path) {
            Ok(f) => Ok(Some(f)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_at(&path)(e)),
        }
    }

    /// Re-hashes a stored blob and reports whether it still matches its name.
    ///
    /// `Ok(false)` means the file is present and WRONG — bit rot, a truncated
    /// restore from backup, a hand-edited cache. §9.6 treats that exactly like
    /// a missing blob, and the caller decides; this method does not delete,
    /// because silently discarding the only copy of something is worse than
    /// reporting it.
    pub fn verify(&self, digest: &str) -> Result<Option<bool>, CacheError> {
        let Some(mut file) = self.open_blob(digest)? else {
            return Ok(None);
        };
        let path = self.require_path(digest)?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf).map_err(io_at(&path))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(Some(format!("sha256:{:x}", hasher.finalize()) == digest))
    }

    /// Admits `reader`'s bytes under `expected`, hashing AS IT STREAMS.
    ///
    /// §10.6: "cache admission requires streaming digest verification of `B`."
    /// The bytes are never held whole in memory and never reachable under a
    /// digest name until they have hashed to it. `declared_length`, when the
    /// caller has a descriptor, enforces §10.1's "declared length ≠ actual
    /// length ⇒ rejection" on the way past.
    ///
    /// A failed admission leaves nothing behind but a removed staging file;
    /// if the process dies instead, the staging file survives and is
    /// collectable garbage that no reader can mistake for a blob.
    pub fn admit(
        &self,
        expected: &str,
        declared_length: Option<u64>,
        reader: &mut impl Read,
    ) -> Result<Admission, CacheError> {
        let final_path = self.require_path(expected)?;
        if final_path.is_file() {
            return Ok(Admission::AlreadyPresent);
        }

        let staging = self.staging_path(expected);
        let outcome = self.stream_into(&staging, expected, declared_length, reader);
        match outcome {
            Ok(()) => {}
            Err(e) => {
                // Best effort: the staging file is already harmless where it
                // is, so a failure to remove it must not mask the real error.
                let _ = fs::remove_file(&staging);
                return Err(e);
            }
        }

        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).map_err(io_at(parent))?;
        }
        // A racing admission of the SAME digest would have written the same
        // bytes, so losing the race is not an error and the rename settles it
        // either way.
        fs::rename(&staging, &final_path).map_err(io_at(&final_path))?;
        Ok(Admission::Stored)
    }

    /// Admits bytes already in memory.
    pub fn admit_bytes(&self, expected: &str, bytes: &[u8]) -> Result<Admission, CacheError> {
        self.admit(
            expected,
            Some(bytes.len() as u64),
            &mut io::Cursor::new(bytes),
        )
    }

    /// Imports bytes whose digest is not known in advance.
    ///
    /// This is what IMPORT needs, and it is a different operation from
    /// [`CacheStore::admit`]: a file a user is importing has no digest yet, so
    /// there is nothing to verify against. The bytes are streamed to staging
    /// and hashed on the way, and the result NAMES the digest they turned out
    /// to have — which is then, by construction, the digest they are stored
    /// under.
    ///
    /// The same crash property holds. The staging file is renamed only after
    /// the whole stream has been read and hashed, so a digest name never
    /// points at a partial import, and an interrupted one leaves staging
    /// rather than a blob. The row L5 ceiling still applies, still during
    /// receipt.
    pub fn import(&self, reader: &mut impl Read) -> Result<(String, Admission), CacheError> {
        let staging = self.staging_path("import");
        let digest = match self.stream_unverified(&staging, reader) {
            Ok(digest) => digest,
            Err(e) => {
                let _ = fs::remove_file(&staging);
                return Err(e);
            }
        };
        let final_path = self.require_path(&digest)?;
        if final_path.is_file() {
            // §10.6: writers deduplicate before write. The bytes are identical
            // by definition — they hash to the same digest — so the staging
            // copy is redundant rather than a conflict.
            let _ = fs::remove_file(&staging);
            return Ok((digest, Admission::AlreadyPresent));
        }
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).map_err(io_at(parent))?;
        }
        fs::rename(&staging, &final_path).map_err(io_at(&final_path))?;
        Ok((digest, Admission::Stored))
    }

    /// Imports the file at `path`, returning its digest and stored length.
    ///
    /// The length is what the store holds, not what the directory entry said
    /// before the read: a file can change underneath a reader, and the digest
    /// describes the bytes that were actually hashed.
    pub fn import_file(&self, path: &Path) -> Result<(String, u64, Admission), CacheError> {
        let mut file = fs::File::open(path).map_err(io_at(path))?;
        let (digest, admission) = self.import(&mut file)?;
        let len = self
            .stored_bytes(&digest)
            .ok_or_else(|| CacheError::MalformedDigest {
                digest: digest.clone(),
            })?;
        Ok((digest, len, admission))
    }

    /// Streams to `staging`, hashing, and returns the digest the bytes had.
    fn stream_unverified(
        &self,
        staging: &Path,
        reader: &mut impl Read,
    ) -> Result<String, CacheError> {
        let mut file = fs::File::create(staging).map_err(io_at(staging))?;
        let mut hasher = Sha256::new();
        let mut written: u64 = 0;
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf).map_err(io_at(staging))?;
            if n == 0 {
                break;
            }
            written += n as u64;
            if written > self.limits.max_blob_bytes {
                return Err(CacheError::TooLarge {
                    digest: "the imported file".to_owned(),
                    limit: self.limits.max_blob_bytes,
                });
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n]).map_err(io_at(staging))?;
        }
        file.flush().map_err(io_at(staging))?;
        file.sync_all().map_err(io_at(staging))?;
        Ok(format!("sha256:{:x}", hasher.finalize()))
    }

    fn stream_into(
        &self,
        staging: &Path,
        expected: &str,
        declared_length: Option<u64>,
        reader: &mut impl Read,
    ) -> Result<(), CacheError> {
        let mut file = fs::File::create(staging).map_err(io_at(staging))?;
        let mut hasher = Sha256::new();
        let mut written: u64 = 0;
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf).map_err(io_at(staging))?;
            if n == 0 {
                break;
            }
            written += n as u64;
            // Checked DURING receipt, not after: the point of a ceiling is to
            // stop writing, and a check at the end has already spent the disk.
            if written > self.limits.max_blob_bytes {
                return Err(CacheError::TooLarge {
                    digest: expected.to_owned(),
                    limit: self.limits.max_blob_bytes,
                });
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n]).map_err(io_at(staging))?;
        }
        file.flush().map_err(io_at(staging))?;
        // Durability before the rename. Without it the rename can be visible
        // while the contents are not, which is precisely the "reachable by
        // name but truncated" state this module claims cannot happen.
        file.sync_all().map_err(io_at(staging))?;
        drop(file);

        if let Some(declared) = declared_length {
            if declared != written {
                return Err(CacheError::LengthMismatch {
                    digest: expected.to_owned(),
                    declared,
                    actual: written,
                });
            }
        }
        let actual = format!("sha256:{:x}", hasher.finalize());
        if actual != expected {
            return Err(CacheError::DigestMismatch {
                expected: expected.to_owned(),
                actual,
            });
        }
        Ok(())
    }

    /// A staging path that cannot collide between concurrent admissions.
    ///
    /// Named by digest plus a fresh UUID rather than by digest alone: two
    /// processes admitting the same digest at once would otherwise interleave
    /// writes into one file and each rename a half-written result into place.
    fn staging_path(&self, label: &str) -> PathBuf {
        let suffix = label.strip_prefix("sha256:").unwrap_or(label);
        self.root
            .join(INCOMING_DIR)
            .join(format!("{suffix}.{}", uuid::Uuid::now_v7()))
    }

    /// Every digest the store holds, ascending.
    pub fn digests(&self) -> Result<Vec<String>, CacheError> {
        let mut out = Vec::new();
        let base = self.root.join("blob").join("sha256");
        let Ok(shards) = fs::read_dir(&base) else {
            return Ok(out);
        };
        for shard in shards.flatten() {
            let shard_name = shard.file_name().to_string_lossy().into_owned();
            let Ok(entries) = fs::read_dir(shard.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                let tail = entry.file_name().to_string_lossy().into_owned();
                let candidate = format!("sha256:{shard_name}{tail}");
                // Round-tripping through the §10.3 mapping is the filter: a
                // stray file with a name of the wrong shape maps to nothing
                // and is not reported as a blob.
                if domowik_container::blob_entry_name(&candidate)
                    .is_some_and(|n| self.root.join(n) == entry.path())
                {
                    out.push(candidate);
                }
            }
        }
        out.sort();
        Ok(out)
    }

    /// Total bytes the store holds. §10.4 and issue 0026's size criterion make
    /// this the "cache" class, distinct from package and remote bytes.
    pub fn total_bytes(&self) -> Result<u64, CacheError> {
        let mut total = 0;
        for digest in self.digests()? {
            total += self.stored_bytes(&digest).unwrap_or(0);
        }
        Ok(total)
    }

    /// Abandoned staging files, which a crash mid-admission leaves behind.
    ///
    /// They are unreachable as blobs by construction, so this is hygiene
    /// rather than correctness — but unreported hygiene debt becomes a full
    /// disk, so it is enumerable.
    pub fn abandoned_staging(&self) -> Result<Vec<PathBuf>, CacheError> {
        let dir = self.root.join(INCOMING_DIR);
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(Vec::new());
        };
        let mut out: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        out.sort();
        Ok(out)
    }

    /// Removes one blob. Only [`crate::gc`] should call this, and only through
    /// its protection checks — §10.7 G3 is why it never reads the file first.
    pub(crate) fn remove_blob(&self, digest: &str) -> Result<bool, CacheError> {
        let path = self.require_path(digest)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(io_at(&path)(e)),
        }
    }
}
