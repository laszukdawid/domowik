package persist

import (
	"io"
	"os"
	"path/filepath"
	"time"
)

// save.go: the §9.7 S0–S5 atomic-replacement state machine. This is the
// ONLY write path to a target package; no append, partial overwrite, or
// in-place mutation exists. Fault injection is deterministic via hooks;
// mocked failures exercise process-level semantics only and never
// establish power-loss durability (§9.7 platform profiles).

// SavePhase names one state-machine step for observability and injection.
type SavePhase int

const (
	S0TempCreated SavePhase = iota
	S1Written
	S2Validated
	S3Synced
	S4Replaced
	S5DirSynced
)

func (p SavePhase) String() string {
	return [...]string{"S0", "S1", "S2", "S3", "S4", "S5"}[int(p)]
}

// SaveHooks replaces platform primitives for deterministic fault
// injection. Nil fields use the real implementations.
type SaveHooks struct {
	// TempName produces the collision-safe unique candidate name given
	// the default; returning an error aborts at S0 (X1 class).
	TempName func(defaultName string) (string, error)
	// Writer wraps the candidate stream during S1 (X2/X3 short-write,
	// ENOSPC, EIO injection at exact byte offsets).
	Writer func(w io.Writer) io.Writer
	// Validate runs the S2 full reader-equivalent validation of the
	// completed candidate file.
	Validate func(path string) *Error
	// SyncFile is the S3 content sync primitive.
	SyncFile func(f *os.File) error
	// Replace is the S4 atomic same-filesystem replacement primitive.
	Replace func(tmpPath, targetPath string) error
	// SyncDir is the S5 directory-metadata sync primitive.
	SyncDir func(dir string) error
	// AfterPhase observes each completed step; a returned error aborts
	// with that step's documented failure semantics.
	AfterPhase func(p SavePhase) error
}

// SaveResult reports observable save outcomes, including the F6/S5
// durability caveat when a directory sync failed or is unsupported.
type SaveResult struct {
	DurabilityCaveat bool
	Phases           []SavePhase
}

// tempArtifactPrefix is the L30 artifact naming scheme: temporary
// candidates live beside the target as <target>.tmp-<19-digit-seq>-<rand>
// so discovery can classify leftovers deterministically and sequence
// numbers order same-device artifacts (BEHAVIOR-SPEC §9.3).
const tempArtifactInfix = ".tmp-"

// SaveDocument writes view-derived input through the full S0–S5 machine,
// including S2 reader-equivalent candidate validation via LoadFile. The
// candidate is STREAMED to disk entry-by-entry (S1); it is never
// materialized as one in-memory buffer.
func SaveDocument(target string, mode os.FileMode, in *WriteInput, hooks *SaveHooks) (*SaveResult, error) {
	if derr := checkExternalModification(target, in); derr != nil {
		return nil, derr
	}
	pkg, berr := buildEntrySet(in)
	if berr != nil {
		return nil, berr
	}
	hooks = withValidation(hooks, func(path string) *Error {
		view, lerr := LoadFile(path)
		if lerr != nil {
			if fe, ok := lerr.(*Error); ok {
				return &Error{Category: CatSafeWriteRefusal, Stage: StageSave,
					Detail: "candidate-invalid: " + fe.Error()}
			}
			return &Error{Category: CatSafeWriteRefusal, Stage: StageSave,
				Detail: "candidate-invalid: " + lerr.Error()}
		}
		// LoadFile keeps the candidate open for StreamEntry; release it
		// on EVERY exit path so repeated saves do not leak descriptors.
		defer view.Close()
		return nil
	})
	return saveEntries(target, mode, pkg, hooks)
}

func withValidation(hooks *SaveHooks, validate func(string) *Error) *SaveHooks {
	if hooks == nil {
		hooks = &SaveHooks{}
	}
	if hooks.Validate == nil {
		hooks.Validate = validate
	}
	return hooks
}

// buildEntrySet assembles the deterministic entry set for streaming
// emission, applying the same envelope and preservation checks as
// BuildPackage without serializing the container first.
func buildEntrySet(in *WriteInput) ([]zipWriterEntry, *Error) {
	if len(in.RefusalReasons) > 0 {
		return nil, fail(CatSafeWriteRefusal, StageSave,
			"unpreservable optional data would be stripped on write-back: %v", in.RefusalReasons)
	}
	if in.Profile != profileThin && in.Profile != profilePortable {
		return nil, fail(CatInvalidFieldSyntax, StageSave, "profile must be thin or portable")
	}
	totalOps := 0
	for _, b := range in.RetainedBatches {
		if b == nil || len(b.Operations) == 0 {
			return nil, fail(CatMalformedFraming, StageSave, "retained batch set contains an absent or empty batch")
		}
		totalOps += len(b.Operations)
	}
	if totalOps > LimitTotalRetainedOps || len(in.RetainedBatches) > LimitBatchesPerPackage {
		return nil, fail(CatSafeWriteRefusal, StageSave,
			"retained-operation envelope exceeds L23; compaction required before write")
	}
	recentIDs := make([]string, 0, len(in.RetainedBatches))
	for _, b := range in.RetainedBatches {
		recentIDs = append(recentIDs, b.BatchID.String())
	}
	snapshotRaw, serr := buildSnapshotJSON(in.State, recentIDs)
	if serr != nil {
		return nil, serr
	}
	if uint64(len(snapshotRaw)) > LimitSnapshotMetadataByte {
		return nil, fail(CatLimitSnapshotMeta, StageSave,
			"snapshot section exceeds the L23 metadata ceiling")
	}
	var entries []zipWriterEntry
	blobEntries, rootFromBlobs, berr := blobZipEntries(in)
	if berr != nil {
		return nil, berr
	}
	entries = append(entries, blobEntries...)
	if len(in.DescriptorsRaw) > 0 {
		if derr := parseDescriptors(in.DescriptorsRaw); derr != nil {
			return nil, derr
		}
		entries = append(entries, zipWriterEntry{
			name: "assets/descriptors.json", data: in.DescriptorsRaw, method: methodDeflate})
	}
	for _, b := range in.RetainedBatches {
		raw, genErr := buildBatchJSON(b)
		if genErr != nil {
			return nil, genErr
		}
		entries = append(entries, zipWriterEntry{
			name:   batchEntryName(b.CounterBase, b.ActorID.String()),
			data:   raw,
			method: methodStore,
		})
	}
	entries = append(entries, zipWriterEntry{
		name: "snapshot/state.json", data: snapshotRaw, method: methodDeflate})
	previewNames := make([]string, 0, len(in.PreviewEntries))
	for name := range in.PreviewEntries {
		previewNames = append(previewNames, name)
	}
	sortStrings(previewNames)
	for _, name := range previewNames {
		ns, nerr := classifyNamespace(name)
		if nerr != nil || (ns != nsPreviewData && ns != nsPreviewIndex) {
			return nil, fail(CatInvalidEntryName, StageSave, "preview entry %q outside its namespace", name)
		}
		entries = append(entries, zipWriterEntry{name: name, data: in.PreviewEntries[name], method: methodStore})
	}
	sortEntrySlice(entries)
	withPlaceholder := append(append([]zipWriterEntry{}, entries...),
		zipWriterEntry{name: "domowik.json"})
	sortEntrySlice(withPlaceholder)
	roots, rerr := mergeAssetRoots(in.AssetRoot, rootFromBlobs)
	if rerr != nil {
		return nil, rerr
	}
	manifestRaw, merr := buildManifestJSON(in.Profile, roots, withPlaceholder)
	if merr != nil {
		return nil, merr
	}
	all := append(entries, zipWriterEntry{name: "domowik.json", data: manifestRaw, method: methodDeflate})
	sortEntrySlice(all)
	return all, nil
}

// saveEntries runs S0–S5 while streaming the candidate through
// emitArchive (S1 stream-write).
func saveEntries(target string, mode os.FileMode, entries []zipWriterEntry, hooks *SaveHooks) (*SaveResult, error) {
	if hooks == nil {
		hooks = &SaveHooks{}
	}
	res := &SaveResult{Phases: []SavePhase{}}
	dir := filepath.Dir(target)
	base := filepath.Base(target)

	// S0 — create the temporary sibling. Regular-file targets only:
	// symlink/reparse targets are rejected, not followed.
	if fi, err := os.Lstat(target); err == nil && fi.Mode()&os.ModeSymlink != 0 {
		return nil, fail(CatMalformedContainer, StageSave,
			"target %q is a symlink; refusing rather than following", base)
	}
	defaultName := base + tempArtifactInfix + padSeq(time.Now().UnixNano()) + "-c0"
	tmpName := defaultName
	if hooks.TempName != nil {
		chosen, terr := hooks.TempName(defaultName)
		if terr != nil {
			return res, fail(CatMalformedContainer, StageSave, "temporary creation failed (X1): %v", terr)
		}
		tmpName = chosen
	}
	tmpPath := filepath.Join(dir, tmpName)
	f, err := os.OpenFile(tmpPath, os.O_CREATE|os.O_EXCL|os.O_WRONLY, mode)
	if err != nil {
		return res, fail(CatMalformedContainer, StageSave, "temporary sibling creation failed: %v", err)
	}
	leftover := func() { _ = f.Close(); os.Remove(tmpPath) }
	abort := func(stage Stage, format string, args ...any) error {
		leftover() // F8: leftovers remain ignorable/cleanable; never promoted
		return fail(CatSafeWriteRefusal, stage, format, args...)
	}
	res.Phases = append(res.Phases, S0TempCreated)
	if hooks.AfterPhase != nil {
		if err := hooks.AfterPhase(S0TempCreated); err != nil {
			return res, abort(StageSave, "aborted after S0 (F1): %v", err)
		}
	}

	// S1 — stream-write the complete candidate under the limits:
	// local headers and payloads go out entry-by-entry through the
	// (possibly fault-injecting) writer chain; the package is never
	// materialized as one buffer.
	var w io.Writer = f
	if hooks.Writer != nil {
		w = hooks.Writer(w)
	}
	if werr := emitArchive(w, entries); werr != nil {
		return res, abort(StageEntry, "candidate write interrupted (X2/X3/F2): %s", werr.Detail)
	}
	if err := f.Close(); err != nil {
		os.Remove(tmpPath)
		return res, fail(CatSafeWriteRefusal, StageEntry, "candidate close failed: %v", err)
	}
	res.Phases = append(res.Phases, S1Written)
	if hooks.AfterPhase != nil {
		if err := hooks.AfterPhase(S1Written); err != nil {
			os.Remove(tmpPath)
			return res, fail(CatSafeWriteRefusal, StageSave, "aborted after S1: %v", err)
		}
	}

	// S2 — validate the candidate with full reader-equivalent checks.
	if hooks.Validate != nil {
		if verr := hooks.Validate(tmpPath); verr != nil {
			os.Remove(tmpPath)
			return res, fail(CatSafeWriteRefusal, StageSave,
				"candidate-invalid (F3): %s", verr.Detail)
		}
	}
	res.Phases = append(res.Phases, S2Validated)
	if hooks.AfterPhase != nil {
		if err := hooks.AfterPhase(S2Validated); err != nil {
			os.Remove(tmpPath)
			return res, fail(CatSafeWriteRefusal, StageSave, "aborted after S2: %v", err)
		}
	}

	// S3 — flush/sync candidate contents (POSIX fsync; Windows
	// FlushFileBuffers equivalent via the OS sync primitive).
	syncPrimitive := func() error {
		if hooks.SyncFile != nil {
			sf, ferr := os.OpenFile(tmpPath, os.O_WRONLY, mode)
			if ferr != nil {
				return fail(CatSafeWriteRefusal, StageSave, "sync reopen failed: %v", ferr)
			}
			defer sf.Close()
			if serr := hooks.SyncFile(sf); serr != nil {
				return fail(CatSafeWriteRefusal, StageSave, "content sync failed (F4): %v", serr)
			}
			return nil
		}
		sf, ferr := os.OpenFile(tmpPath, os.O_WRONLY, mode)
		if ferr != nil {
			return fail(CatSafeWriteRefusal, StageSave, "sync reopen failed: %v", ferr)
		}
		defer sf.Close()
		if serr := sf.Sync(); serr != nil {
			return fail(CatSafeWriteRefusal, StageSave, "content sync failed (F4): %v", serr)
		}
		return nil
	}
	if err := syncPrimitive(); err != nil {
		os.Remove(tmpPath)
		return res, err
	}
	res.Phases = append(res.Phases, S3Synced)
	if hooks.AfterPhase != nil {
		if err := hooks.AfterPhase(S3Synced); err != nil {
			os.Remove(tmpPath)
			return res, fail(CatSafeWriteRefusal, StageSave, "aborted after S3 (F4): %v", err)
		}
	}

	// S4 — atomic replace within the same filesystem.
	if hooks.Replace != nil {
		if rerr := hooks.Replace(tmpPath, target); rerr != nil {
			// F5: report the error; the target is whatever the filesystem
			// left — old or new under the rename assumption. The leftover
			// candidate stays cleanable.
			return res, fail(CatSafeWriteRefusal, StageSave,
				"atomic replace failed (F5): %v; target is old-or-new per rename atomicity", rerr)
		}
	} else if err := platformReplace(tmpPath, target); err != nil {
		return res, fail(CatSafeWriteRefusal, StageSave,
			"atomic replace failed (F5): %v; target is old-or-new per the S4 primitive's atomicity", err)
	}
	res.Phases = append(res.Phases, S4Replaced)
	if hooks.AfterPhase != nil {
		if err := hooks.AfterPhase(S4Replaced); err != nil {
			// F6-class caveat path: replacement stands; no rollback.
			res.DurabilityCaveat = true
			return res, &Error{Category: CatSafeWriteRefusal, Stage: StageSave,
				Detail: "post-replace interruption acknowledged (F6): new package readable; no rollback"}
		}
	}

	// S5 — sync the parent directory where supported.
	if hooks.SyncDir != nil {
		if serr := hooks.SyncDir(dir); serr != nil {
			res.DurabilityCaveat = true // F6: result stands; durability caveated
		}
	} else if d, derr := os.Open(dir); derr == nil {
		if serr := d.Sync(); serr != nil {
			res.DurabilityCaveat = true // unsupported or failed: caveat acknowledged
		}
		d.Close()
	} else {
		res.DurabilityCaveat = true
	}
	res.Phases = append(res.Phases, S5DirSynced)
	return res, nil
}

func padSeq(v int64) string {
	neg := v < 0
	var u uint64
	if neg {
		u = uint64(-v)
	} else {
		u = uint64(v)
	}
	s := itoa64(int64(u))
	for len(s) < TempArtifactSeqDigits {
		s = "0" + s
	}
	if neg {
		s = "1" + s[len(s)-(TempArtifactSeqDigits-1):]
	}
	return s
}

// Autosave implements the F9 contract surface: a save attempt whose
// failure is reported non-blockingly and leaves the target untouched;
// the caller retries at its next trigger. It performs the identical S0–S5
// machine; nothing here mutates the unsaved-changes state on failure.
func Autosave(target string, mode os.FileMode, in *WriteInput, hooks *SaveHooks) (bool, error) {
	_, err := SaveDocument(target, mode, in, hooks)
	return err == nil, err
}
