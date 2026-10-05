//! Byte-level ZIP assembly for the issue-0028 fixture corpus.
//!
//! This is NOT a ZIP library. It exists to emit archives that are DELIBERATELY
//! INVALID — truncated end-of-central-directory records, contradictory sizes
//! across the local and central headers, spanned disk numbers, encrypted-entry
//! flags, NUL bytes in names — which a correct writer such as the `zip` crate
//! cannot produce, because producing them is precisely what it prevents.
//!
//! Nothing here reads, validates, or interprets the Domowik format. Every
//! field is written under caller control so a hostile fixture can contradict
//! exactly one thing and nothing else.
//!
//! Ported from `internal/corpusgen/zipwriter.go` under issue 0062. The
//! serialization is byte-for-byte identical to the Go original; the committed
//! corpus artifacts are the test of that claim.

/// Compression method: the entry is stored uncompressed.
pub const METHOD_STORE: u16 = 0;
/// Compression method: the entry carries a raw DEFLATE stream.
///
/// The generator does not compress. An entry using this method must supply its
/// stream through [`Entry::stored_override`]; see `PINNED_DEFLATE_STREAMS` in
/// the generator for why.
pub const METHOD_DEFLATE: u16 = 8;

const SIG_LOCAL: u32 = 0x0403_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_EOCD: u32 = 0x0605_4b50;
/// Zip64 end-of-central-directory signature, used by hostile fixtures only.
pub const SIG_EOCD64: u32 = 0x0606_4b50;
/// Zip64 end-of-central-directory LOCATOR signature, used by hostile fixtures.
pub const SIG_LOC64: u32 = 0x0706_4b50;

const LOCAL_HEADER_LEN: usize = 30;
const CENTRAL_HEADER_LEN: usize = 46;

/// The end-of-central-directory record is a fixed 22 bytes when no archive
/// comment is present: signature(0), thisDiskNumber(4), cdStartDisk(6),
/// entriesOnThisDisk(8), totalEntries(10), cdSize(12), cdOffset(16),
/// commentLength(20). These names exist so hostile EOCD patches address a
/// FIELD rather than an offset counted from the end of the file.
pub const EOCD_FIXED_LEN: usize = 22;
/// Offset of `thisDiskNumber` within the fixed EOCD record.
pub const EOCD_DISK_NUMBER_OFFSET: usize = 4;

/// Patches a finished archive in place, so a fixture can corrupt the
/// end-of-central-directory record after every offset has been computed
/// honestly.
pub type PatchEocd<'a> = &'a dyn Fn(&mut Vec<u8>);

/// One physical archive member under full byte-level control.
///
/// Fields map one-to-one onto the ZIP local and central header fields. The
/// `*_override` fields are applied AFTER canonical field computation so a
/// fixture can contradict a single declared value while everything else stays
/// self-consistent.
#[derive(Clone, Debug, Default)]
pub struct Entry {
    /// Entry name, written verbatim into both headers.
    ///
    /// RAW BYTES, not a `String`, because the point of this writer is to emit
    /// names a conforming writer would refuse: NUL bytes, backslashes, drive
    /// letters, and byte sequences that are not valid UTF-8 at all. Go strings
    /// carry arbitrary bytes, so the Go original could author such a fixture;
    /// a `String` here could not, and that would have been a capability the
    /// port silently dropped.
    pub name: Vec<u8>,
    /// The logical payload. CRC and uncompressed size derive from it.
    pub data: Vec<u8>,
    /// [`METHOD_STORE`] or [`METHOD_DEFLATE`].
    pub method: u16,
    /// General-purpose bit flags. Bit 11 (UTF-8) is added unless
    /// [`Entry::suppress_utf8_flag`] is set.
    pub flags: u16,
    /// "Version needed to extract", both headers.
    pub version_needed: u16,
    /// "Version made by", central header only.
    pub version_made_by: u16,
    /// Internal file attributes, central header only.
    pub int_attrs: u16,
    /// External file attributes, central header only.
    pub ext_attrs: u32,
    /// Extra field, written into both headers.
    pub extra: Vec<u8>,
    /// Entry comment, central header only.
    pub comment: String,
    /// "Disk number start", central header only.
    pub disk_start: u16,

    /// Replaces the computed CRC-32 in BOTH headers.
    pub override_crc: Option<u32>,
    /// Replaces the computed uncompressed size in BOTH headers.
    pub override_uncomp: Option<u32>,
    /// Replaces the computed compressed size in BOTH headers.
    pub override_comp: Option<u32>,
    /// A contradicting uncompressed size written ONLY in the local header.
    pub local_only_uncomp: Option<u32>,
    /// A contradicting CRC written ONLY in the central header.
    pub central_only_crc: Option<u32>,
    /// Omit the UTF-8 general-purpose flag bit.
    pub suppress_utf8_flag: bool,

    /// Writes exactly these bytes as the stored stream while declaring
    /// sizes and CRC derived from [`Entry::data`].
    pub stored_override: Option<Vec<u8>>,
}

impl Entry {
    /// A stored entry carrying `data` under `name`, every other field canonical.
    pub fn new(name: impl Into<Vec<u8>>, data: impl Into<Vec<u8>>) -> Self {
        Entry {
            name: name.into(),
            data: data.into(),
            ..Entry::default()
        }
    }
}

struct CentralRecord<'a> {
    entry: &'a Entry,
    offset: u32,
    crc: u32,
    usize_: u32,
    csize: u32,
}

fn flags_of(e: &Entry) -> u16 {
    if e.suppress_utf8_flag {
        e.flags
    } else {
        e.flags | 0x0800
    }
}

fn put16(buf: &mut [u8], at: usize, v: u16) {
    buf[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

fn put32(buf: &mut [u8], at: usize, v: u32) {
    buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// Writes a little-endian `u32` at the start of `buf`.
///
/// Exposed because hostile fixtures assemble trailing zip64 records by hand.
pub fn put_u32(buf: &mut [u8], v: u32) {
    buf[0..4].copy_from_slice(&v.to_le_bytes());
}

/// Writes a little-endian `u64` across the first eight bytes of `buf`.
pub fn put_u64(buf: &mut [u8], v: u64) {
    buf[0..8].copy_from_slice(&v.to_le_bytes());
}

/// CRC-32/ISO-HDLC (the checksum ZIP uses) of `data`.
pub fn crc_of(data: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(data);
    h.finalize()
}

/// Serializes `entries` in slice order.
///
/// Physical order is never semantic; hostile ordering fixtures are authored
/// explicitly. `patch_eocd`, when present, is applied to the finished archive
/// so a fixture can corrupt the end-of-central-directory record after every
/// offset has been computed honestly.
///
/// # Panics
///
/// Panics when an entry declares [`METHOD_DEFLATE`] without supplying
/// [`Entry::stored_override`]. The generator does not compress: reproducing
/// the committed streams byte-for-byte requires the exact encoder that
/// produced them, so a DEFLATE entry carries its stream as a recorded input
/// instead.
pub fn write_archive(entries: &[Entry], patch_eocd: Option<PatchEocd<'_>>) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    let mut centrals: Vec<CentralRecord> = Vec::with_capacity(entries.len());

    for e in entries {
        let payload = &e.data;
        let mut crc = crc_of(payload);
        let mut usize_ = payload.len() as u32;
        let stored: &[u8] = match (&e.stored_override, e.method) {
            (Some(s), _) => s.as_slice(),
            (None, METHOD_DEFLATE) => panic!(
                "entry {:?} declares DEFLATE without a stored stream: the corpus generator \
                 does not compress, so a compressed entry must carry stored_override",
                String::from_utf8_lossy(&e.name)
            ),
            (None, _) => payload.as_slice(),
        };
        let mut csize = stored.len() as u32;
        if let Some(v) = e.override_crc {
            crc = v;
        }
        if let Some(v) = e.override_uncomp {
            usize_ = v;
        }
        if let Some(v) = e.override_comp {
            csize = v;
        }
        let flags = flags_of(e);
        let name_len = e.name.len() as u16;
        let extra_len = e.extra.len() as u16;

        let offset = buf.len() as u32;
        let mut local = [0u8; LOCAL_HEADER_LEN];
        put32(&mut local, 0, SIG_LOCAL);
        put16(&mut local, 4, e.version_needed);
        put16(&mut local, 6, flags);
        put16(&mut local, 8, e.method);
        put32(&mut local, 14, crc);
        let local_uncomp = e.local_only_uncomp.unwrap_or(usize_);
        put32(&mut local, 18, csize);
        put32(&mut local, 22, local_uncomp);
        put16(&mut local, 26, name_len);
        put16(&mut local, 28, extra_len);
        buf.extend_from_slice(&local);
        buf.extend_from_slice(&e.name);
        buf.extend_from_slice(&e.extra);
        buf.extend_from_slice(stored);

        centrals.push(CentralRecord {
            entry: e,
            offset,
            crc: e.central_only_crc.unwrap_or(crc),
            usize_,
            csize,
        });
    }

    let cd_start = buf.len() as u32;
    for c in &centrals {
        let e = c.entry;
        let flags = flags_of(e);
        let mut rec = [0u8; CENTRAL_HEADER_LEN];
        put32(&mut rec, 0, SIG_CENTRAL);
        put16(&mut rec, 4, e.version_made_by);
        put16(&mut rec, 6, e.version_needed);
        put16(&mut rec, 8, flags);
        put16(&mut rec, 10, e.method);
        put32(&mut rec, 16, c.crc);
        put32(&mut rec, 20, c.csize);
        put32(&mut rec, 24, c.usize_);
        put16(&mut rec, 28, e.name.len() as u16);
        put16(&mut rec, 30, e.extra.len() as u16);
        put16(&mut rec, 32, e.comment.len() as u16);
        put16(&mut rec, 34, e.disk_start);
        put16(&mut rec, 36, e.int_attrs);
        put32(&mut rec, 38, e.ext_attrs);
        put32(&mut rec, 42, c.offset);
        buf.extend_from_slice(&rec);
        buf.extend_from_slice(&e.name);
        buf.extend_from_slice(&e.extra);
        buf.extend_from_slice(e.comment.as_bytes());
    }
    let cd_size = buf.len() as u32 - cd_start;

    let mut eocd = [0u8; EOCD_FIXED_LEN];
    put32(&mut eocd, 0, SIG_EOCD);
    put16(&mut eocd, 8, centrals.len() as u16);
    put16(&mut eocd, 10, centrals.len() as u16);
    put32(&mut eocd, 12, cd_size);
    put32(&mut eocd, 16, cd_start);
    buf.extend_from_slice(&eocd);

    if let Some(patch) = patch_eocd {
        patch(&mut buf);
    }
    buf
}
