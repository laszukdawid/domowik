package cli

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"sort"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// ops.go implements the issue-0027 operations commands.
//
// `domowik ops` is bound by the ED-0023 no-execution contract: every fact
// it reports comes from the parse-only persist.Inspect seam. It never
// constructs operations, replays a snapshot, applies anything, or writes.
//
// `domowik compact` is PLAN-ONLY BY DEFAULT: it computes and displays the
// retention decision and its undo/recovery effects without touching the
// target. Only `--execute` writes, through the §9.7 atomic-replacement
// save path with a §9.2 baseline over the target's exact bytes, after an
// in-process projection-equivalence proof of the candidate.
//
// Exit codes: 0 success (including a plan), 1 runtime/package/refusal,
// 2 usage. Success output goes only to stdout; diagnostics and
// usage-on-failure go only to stderr.

// ---------------------------------------------------------------------------
// shared flag handling

type commandFlags struct {
	json            bool
	retainBatches   int
	hasRetain       bool
	assertLocalOnly bool
	execute         bool
	positional      []string
}

// parseCommandFlags accepts the issue-0027 flag set in any position;
// anything beginning with `-` that is not recognized is a usage error.
func parseCommandFlags(args []string) (*commandFlags, []string, error) {
	f := &commandFlags{retainBatches: -1}
	for i := 0; i < len(args); i++ {
		arg := args[i]
		switch arg {
		case "--json":
			f.json = true
		case "--retain-batches":
			if i+1 >= len(args) {
				return nil, nil, fmt.Errorf("--retain-batches requires a value")
			}
			i++
			if _, err := fmt.Sscanf(args[i], "%d", &f.retainBatches); err != nil ||
				f.retainBatches < 0 {
				return nil, nil, fmt.Errorf("--retain-batches requires a non-negative integer, got %q", args[i])
			}
			f.hasRetain = true
		case "--assert-local-only":
			f.assertLocalOnly = true
		case "--execute":
			f.execute = true
		default:
			if len(arg) > 0 && arg[0] == '-' {
				return nil, nil, fmt.Errorf("unknown flag %q", arg)
			}
			f.positional = append(f.positional, arg)
		}
	}
	return f, f.positional, nil
}

func reportPersistError(stderr io.Writer, cmd string, err error) {
	var perr *persist.Error
	if errors.As(err, &perr) {
		fmt.Fprintf(stderr, "domowik %s: %s [%s/%s]\n", cmd, perr, perr.Category, perr.Stage)
		return
	}
	fmt.Fprintf(stderr, "domowik %s: %v\n", cmd, err)
}

// ---------------------------------------------------------------------------
// domowik ops

type opRecord struct {
	OperationID string `json:"operationId"`
	Kind        string `json:"kind"`
	EntityID    string `json:"entityId"`
	Payload     bool   `json:"payload"`
}

// positionRecord reports one stored write position. causalPastSize is not
// restated: it is the sum of the referenced batch's frontier, carried once
// in batchPositions (FS §5.1 S3).
type positionRecord struct {
	Coordinate  string `json:"coordinate"`
	Batch       string `json:"batch"`
	CounterBase int64  `json:"counterBase"`
	Actor       string `json:"actor"`
	OpIndex     int64  `json:"operationIndex"`
}

type batchPositionRecord struct {
	Batch    string           `json:"batch"`
	Frontier map[string]int64 `json:"frontier,omitempty"`
}

type snapshotRecord struct {
	Present     bool             `json:"present"`
	EntityCount int              `json:"entityCount"`
	Frontier    map[string]int64 `json:"coverageFrontier,omitempty"`
	// Legacy reports that the positions below came from ED-0038's reading
	// CONVENTION applied to pre-ED-0037 members, not from the file. V5
	// exactness is not claimed for them.
	Legacy           bool                  `json:"legacyConvention,omitempty"`
	BatchPositions   []batchPositionRecord `json:"batchPositions,omitempty"`
	WinnerPositions  []positionRecord      `json:"winnerPositions,omitempty"`
	CreatedPositions []positionRecord      `json:"createdPositions,omitempty"`
	Tombstones       []positionRecord      `json:"tombstones,omitempty"`
	RecentBatchIDs   []string              `json:"recentBatchIds,omitempty"`
	UnknownMembers   []string              `json:"unknownMembers,omitempty"`
}

type batchRecord struct {
	Entry          string           `json:"entry"`
	BatchID        string           `json:"batchId"`
	ActorID        string           `json:"actorId"`
	CounterBase    int64            `json:"counterBase"`
	EndCounter     int64            `json:"endCounter"`
	Frontier       map[string]int64 `json:"frontier,omitempty"`
	Operations     []opRecord       `json:"operations"`
	UnknownMembers []string         `json:"unknownMembers,omitempty"`
}

type actorRange struct {
	First int64 `json:"first"`
	Last  int64 `json:"last"`
}

type actorRecord struct {
	ActorID        string       `json:"actorId"`
	BatchCount     int          `json:"batchCount"`
	OperationCount int          `json:"operationCount"`
	Ranges         []actorRange `json:"ranges"`
}

type packageRecord struct {
	Path          string `json:"path"`
	FormatVersion string `json:"formatVersion"`
	Profile       string `json:"profile"`
	SHA256        string `json:"sha256"`
}

type opsReport struct {
	Schema        string         `json:"schema"`
	Package       packageRecord  `json:"package"`
	AssetRoot     []string       `json:"assetRoot,omitempty"`
	Snapshot      snapshotRecord `json:"snapshot"`
	Batches       []batchRecord  `json:"batches"`
	Actors        []actorRecord  `json:"actors"`
	MissingAssets []string       `json:"missingAssets,omitempty"`
	Diagnostics   []string       `json:"diagnostics,omitempty"`
}

func buildOpsReport(insp *persist.Inspection, pkg packageRecord) *opsReport {
	pkg.FormatVersion = insp.FormatVersion
	pkg.Profile = insp.Profile
	r := &opsReport{
		Schema:        opsSchemaID,
		Package:       pkg,
		AssetRoot:     insp.AssetRoot,
		MissingAssets: insp.MissingAssets,
		Diagnostics:   insp.Diagnostics,
	}
	sr := snapshotRecord{
		Present:        insp.Snapshot.Present,
		EntityCount:    insp.Snapshot.EntityCount,
		Frontier:       insp.Snapshot.Frontier,
		RecentBatchIDs: insp.Snapshot.RecentBatchIDs,
		UnknownMembers: insp.Snapshot.UnknownMembers,
		Legacy:         insp.Snapshot.Legacy,
	}
	positions := func(list []persist.KeyedPositionSummary) []positionRecord {
		out := make([]positionRecord, 0, len(list))
		for _, k := range list {
			out = append(out, positionRecord{
				Coordinate:  k.Entity,
				Batch:       k.Position.Batch,
				CounterBase: k.Position.CounterBase,
				Actor:       k.Position.Actor,
				OpIndex:     k.Position.OpIndex,
			})
		}
		return out
	}
	sr.WinnerPositions = positions(insp.Snapshot.WinnerPositions)
	sr.CreatedPositions = positions(insp.Snapshot.CreatedPositions)
	sr.Tombstones = positions(insp.Snapshot.Tombstones)
	sr.BatchPositions = make([]batchPositionRecord, 0, len(insp.Snapshot.BatchPositions))
	for _, bp := range insp.Snapshot.BatchPositions {
		sr.BatchPositions = append(sr.BatchPositions, batchPositionRecord{Batch: bp.Batch, Frontier: bp.Frontier})
	}
	r.Snapshot = sr
	r.Batches = make([]batchRecord, 0, len(insp.Batches))
	r.Actors = []actorRecord{}

	for _, b := range insp.Batches {
		br := batchRecord{
			Entry:          b.EntryName,
			BatchID:        b.BatchID,
			ActorID:        b.ActorID,
			CounterBase:    b.CounterBase,
			EndCounter:     b.EndCounter,
			Frontier:       b.Frontier,
			UnknownMembers: b.UnknownMembers,
		}
		for _, op := range b.Operations {
			br.Operations = append(br.Operations, opRecord{
				OperationID: op.OperationID,
				Kind:        op.Kind,
				EntityID:    op.EntityID,
				Payload:     op.HasPayload,
			})
		}
		r.Batches = append(r.Batches, br)
	}
	r.Actors = deriveActors(insp)
	return r
}

// deriveActors folds reserved ranges per actor from the framing
// summaries: disjoint ascending [first,last] pairs, as §7.6/B3 define.
func deriveActors(insp *persist.Inspection) []actorRecord {
	byActor := map[string][]actorRange{}
	for _, b := range insp.Batches {
		byActor[b.ActorID] = append(byActor[b.ActorID], actorRange{
			First: b.CounterBase,
			Last:  b.EndCounter,
		})
	}
	actors := make([]string, 0, len(byActor))
	for a := range byActor {
		actors = append(actors, a)
	}
	sort.Strings(actors)
	counts := map[string]int{}
	for _, b := range insp.Batches {
		counts[b.ActorID] += len(b.Operations)
	}
	out := make([]actorRecord, 0, len(actors))
	for _, a := range actors {
		rs := byActor[a]
		sort.Slice(rs, func(i, j int) bool { return rs[i].First < rs[j].First })
		out = append(out, actorRecord{
			ActorID:        a,
			BatchCount:     len(rs),
			OperationCount: counts[a],
			Ranges:         rs,
		})
	}
	return out
}

// emitJSON encodes v to stdout with a stable layout; encoder failures are
// diagnostics on the INJECTED stderr stream only — the function never
// touches process-level streams, preserving caller stream separation.
const opsSchemaID = "domowik.ops.v1"

func emitJSON(stdout, stderr io.Writer, v any) int {
	enc := json.NewEncoder(stdout)
	enc.SetIndent("", "  ")
	if err := enc.Encode(v); err != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", err)
		return 1
	}
	return 0
}

func runOps(args []string, stdout, stderr io.Writer) int {
	flags, pos, ferr := parseCommandFlags(args)
	if ferr != nil || len(pos) != 1 {
		fmt.Fprintf(stderr, "domowik ops: %v\n", firstErr(ferr, "exactly one FILE argument is required"))
		fmt.Fprint(stderr, usage)
		return 2
	}
	insp, err := persist.InspectFile(pos[0])
	if err != nil {
		reportPersistError(stderr, "ops", err)
		return 1
	}
	digest, derr := persist.BaselineTarget(pos[0])
	if derr != nil {
		reportPersistError(stderr, "ops", derr)
		return 1
	}
	pkg := packageRecord{Path: pos[0], SHA256: digest.SHA256}
	report := buildOpsReport(insp, pkg)
	if flags.json {
		return emitJSON(stdout, stderr, report)
	}
	writeOpsText(stdout, report)
	return 0
}

func writeOpsText(stdout io.Writer, r *opsReport) {
	fmt.Fprintf(stdout, "package %s format %s profile %s sha256 %s\n",
		r.Package.Path, r.Package.FormatVersion, r.Package.Profile, r.Package.SHA256)
	if len(r.AssetRoot) > 0 {
		fmt.Fprintf(stdout, "asset root: %d entr%s\n", len(r.AssetRoot), pluralY(r.AssetRoot))
	}
	s := r.Snapshot
	if s.Present {
		fmt.Fprintf(stdout, "snapshot present: entities %d, coverage frontier %d actor%s",
			s.EntityCount, len(s.Frontier), pluralS(len(s.Frontier)))
		fmt.Fprintf(stdout, ", winner positions %d, creation positions %d, tombstones %d, referenced batches %d, recent batch ids %d\n",
			len(s.WinnerPositions), len(s.CreatedPositions), len(s.Tombstones),
			len(s.BatchPositions), len(s.RecentBatchIDs))
		if s.Legacy {
			fmt.Fprintf(stdout, "  legacy snapshot: positions below are ED-0038's reading convention, not stored data; V5 exactness is not claimed\n")
		}
		for _, a := range sortedKeysOf(s.Frontier) {
			fmt.Fprintf(stdout, "  coverage %s -> %d\n", a, s.Frontier[a])
		}
		for _, bp := range s.BatchPositions {
			fmt.Fprintf(stdout, "  batch %s frontier %d actor%s\n", bp.Batch, len(bp.Frontier), pluralS(len(bp.Frontier)))
		}
		for _, k := range s.WinnerPositions {
			fmt.Fprintf(stdout, "  winner %s batch %s op %d\n", k.Coordinate, k.Batch, k.OpIndex)
		}
		for _, k := range s.CreatedPositions {
			fmt.Fprintf(stdout, "  created %s batch %s op %d\n", k.Coordinate, k.Batch, k.OpIndex)
		}
		for _, k := range s.Tombstones {
			fmt.Fprintf(stdout, "  tombstone %s deleting batch %s op %d\n", k.Coordinate, k.Batch, k.OpIndex)
		}
	} else {
		fmt.Fprintf(stdout, "snapshot absent\n")
	}
	fmt.Fprintf(stdout, "batches: %d\n", len(r.Batches))
	for _, b := range r.Batches {
		fmt.Fprintf(stdout, "  %s batch %s actor %s range [%d..%d] operations %d\n",
			b.Entry, b.BatchID, b.ActorID, b.CounterBase, b.EndCounter, len(b.Operations))
		for _, pair := range sortedPairs(b.Frontier) {
			fmt.Fprintf(stdout, "    observed frontier %s -> %d\n", pair[0], pair[1])
		}
		for _, op := range b.Operations {
			fmt.Fprintf(stdout, "    op %s %s entity %s payload %t\n",
				op.OperationID, op.Kind, op.EntityID, op.Payload)
		}
	}
	fmt.Fprintf(stdout, "actors: %d\n", len(r.Actors))
	for _, a := range r.Actors {
		fmt.Fprintf(stdout, "  %s", a.ActorID)
		for i, rg := range a.Ranges {
			if i > 0 {
				fmt.Fprint(stdout, ",")
			}
			fmt.Fprintf(stdout, " [%d..%d]", rg.First, rg.Last)
		}
		fmt.Fprint(stdout, "\n")
	}
	for _, m := range r.MissingAssets {
		fmt.Fprintf(stdout, "missing asset: %s\n", m)
	}
	for _, d := range r.Diagnostics {
		fmt.Fprintf(stdout, "diagnostic: %s\n", d)
	}
}

func sortedKeysOf(m map[string]int64) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

func sortedPairs(m map[string]int64) [][2]any {
	out := make([][2]any, 0, len(m))
	for _, k := range sortedKeysOf(m) {
		out = append(out, [2]any{k, m[k]})
	}
	return out
}

func pluralY(items []string) string {
	if len(items) == 1 {
		return "y"
	}
	return "ies"
}

func pluralS(n int) string {
	if n == 1 {
		return ""
	}
	return "s"
}

func firstErr(err error, fallback string) error {
	if err != nil {
		return err
	}
	return errors.New(fallback)
}

// ---------------------------------------------------------------------------
// domowik compact

const noSyncProtocol = "no acknowledged snapshot-transfer/collaboration protocol exists in this version; packages whose history involves more than one actor cannot be proven local-only"

type retainedBatchRef struct {
	BatchID string `json:"batchId"`
	Entry   string `json:"entry"`
}

type compactPlan struct {
	Phase         string `json:"phase"` // always "preflight": emitted before any write
	Target        string `json:"target"`
	RetainBatches int    `json:"retainBatches"`
	// FoldedThroughFrontier records, per actor, the highest counter that
	// folding covers: every folded batch of that actor ends at or below
	// it, and the rewritten snapshot watermark must equal or exceed it.
	FoldedThroughFrontier map[string]int64   `json:"foldedThroughFrontier,omitempty"`
	RetainedBatches       []retainedBatchRef `json:"retainedBatches"`
	CurrentBatches        int                `json:"currentBatches"`
	CurrentOperations     int                `json:"currentOperations"`
	KeptBatches           int                `json:"keptBatches"`
	FoldedBatches         int                `json:"foldedBatches"`
	FoldedEntries         []string           `json:"foldedEntries"`
	UndoRedoEffect        string             `json:"undoRedoEffect"`
	RecoveryEffect        string             `json:"recoveryEffect"`
	LocalOnlyVerified     bool               `json:"localOnlyVerified"`
	AssertLocalOnly       bool               `json:"assertLocalOnly"`
	Write                 bool               `json:"write"`
}

type commitReport struct {
	Phase            string `json:"phase"`   // "commit"
	Outcome          string `json:"outcome"` // "success"
	Target           string `json:"target"`
	DurabilityCaveat bool   `json:"durabilityCaveat"`
}

const undoRedoNote = "session undo/redo stacks are device-local and never persisted (BEHAVIOR-SPEC R8.4.2); folding changes no stack on disk, and a live session would surface R8.4.3 truncation diagnostics for stacks resolving against folded batches"
const recoveryNote = "recovery artifacts are separate files and are never modified or deleted by compaction; the replacement is a new target generation under BEHAVIOR-SPEC §9.2"

func runCompact(args []string, stdout, stderr io.Writer) int {
	flags, pos, ferr := parseCommandFlags(args)
	if ferr != nil || len(pos) != 1 {
		fmt.Fprintf(stderr, "domowik compact: %v\n", firstErr(ferr, "exactly one FILE argument is required"))
		fmt.Fprint(stderr, usage)
		return 2
	}
	opts := compactOptions{
		target:          pos[0],
		json:            flags.json,
		retainBatches:   flags.retainBatches,
		assertLocalOnly: flags.assertLocalOnly,
		execute:         flags.execute,
	}
	return compactTarget(opts, stdout, stderr, nil)
}

// injectable persistence seams for deterministic fault-injection proofs;
// production values are the real persist primitives.
var (
	inspectFileFn  = persist.InspectFile
	loadFileFn     = persist.LoadFile
	baselineFn     = persist.BaselineTarget
	buildPackageFn = persist.BuildPackage
	saveDocFn      = persist.SaveDocument
)

type compactOptions struct {
	target          string
	json            bool
	retainBatches   int // -1: flag absent
	assertLocalOnly bool
	execute         bool
}

// compactTarget runs the plan/execute pipeline. hooks is injectable for
// deterministic §9.7 fault-injection proofs; nil uses real primitives.
func compactTarget(opts compactOptions, stdout, stderr io.Writer, hooks *persist.SaveHooks) int {
	insp, err := inspectFileFn(opts.target)
	if err != nil {
		reportPersistError(stderr, "compact", err)
		return 1
	}

	keep := 0
	if opts.retainBatches >= 0 {
		keep = opts.retainBatches
	}
	if keep > len(insp.Batches) {
		keep = len(insp.Batches)
	}
	keptIdx, folded := selectRetention(insp.Batches, keep)

	localOnly, actorCount := inspectLocalOnly(insp)

	plan := compactPlan{
		Target:            opts.target,
		RetainBatches:     keep,
		KeptBatches:       len(keptIdx),
		CurrentBatches:    len(insp.Batches),
		FoldedBatches:     len(folded),
		UndoRedoEffect:    undoRedoNote,
		RecoveryEffect:    recoveryNote,
		LocalOnlyVerified: localOnly,
		Write:             opts.execute,
		AssertLocalOnly:   opts.assertLocalOnly,
	}
	for _, b := range insp.Batches {
		plan.CurrentOperations += len(b.Operations)
	}
	foldedThrough := map[string]int64{}
	plan.FoldedEntries = make([]string, 0, len(folded))
	for _, b := range folded {
		plan.FoldedEntries = append(plan.FoldedEntries, b.EntryName)
		if b.EndCounter > foldedThrough[b.ActorID] {
			foldedThrough[b.ActorID] = b.EndCounter
		}
	}
	plan.Phase = "preflight"
	plan.RetainedBatches = make([]retainedBatchRef, 0, len(keptIdx))
	for _, i := range keptIdx {
		plan.RetainedBatches = append(plan.RetainedBatches, retainedBatchRef{
			BatchID: insp.Batches[i].BatchID,
			Entry:   insp.Batches[i].EntryName,
		})
	}
	plan.FoldedThroughFrontier = foldedThrough
	if !opts.execute {
		if opts.json {
			return emitJSON(stdout, stderr, plan)
		}
		writeCompactText(stdout, plan, keptIdx, insp.Batches)
		fmt.Fprintf(stdout, "plan only: nothing was written; pass --execute to apply\n")
		return 0
	}

	// Execution gates, in order: the explicit operator assertion, then an
	// independently verified single-actor locality proof. The flag alone
	// is NOT protocol; it merely records who answered for the history.
	if !opts.assertLocalOnly {
		fmt.Fprintf(stderr, "domowik compact: refusing %s: execution requires --assert-local-only; locality is never assumed\n", opts.target)
		return 1
	}
	if !localOnly {
		fmt.Fprintf(stderr, "domowik compact: refusing %s: history spans %d actors; %s\n",
			opts.target, actorCount, noSyncProtocol)
		fmt.Fprintf(stderr, "the --assert-local-only flag records an assertion; it is not a transfer protocol and cannot replace one\n")
		return 1
	}

	// PREFLIGHT: the full retention consequence is emitted before any
	// write-path work. Everything after this point may fail; the plan
	// above is what the operator approved.
	if opts.json {
		if cerr := emitJSON(stdout, stderr, plan); cerr != 0 {
			return cerr
		}
	} else {
		writeCompactText(stdout, plan, keptIdx, insp.Batches)
		fmt.Fprintf(stdout, "preflight complete; writing now\n")
	}

	view, lerr := loadFileFn(opts.target)
	if lerr != nil {
		reportPersistError(stderr, "compact", lerr)
		return 1
	}
	var refusals []string
	for _, k := range view.ManifestUnknown {
		refusals = append(refusals, fmt.Sprintf("manifest member %q cannot be re-serialized", k))
	}
	for _, k := range view.SnapshotUnknown {
		refusals = append(refusals, fmt.Sprintf("snapshot member %q cannot be re-serialized", k))
	}
	for entry, members := range view.BatchUnknown {
		for _, k := range members {
			refusals = append(refusals, fmt.Sprintf("batch %q member %q cannot be re-serialized", entry, k))
		}
	}
	for _, a := range view.MissingAssets {
		refusals = append(refusals, fmt.Sprintf("asset %q is missing or unverifiable and cannot be re-embedded", a))
	}
	if len(refusals) > 0 {
		fmt.Fprintf(stderr, "domowik compact: refusing %s: safe-write refusal\n", opts.target)
		for _, r := range refusals {
			fmt.Fprintf(stderr, "  - %s\n", r)
		}
		return 1
	}

	baseline, berr := baselineFn(opts.target)
	if berr != nil {
		reportPersistError(stderr, "compact", berr)
		return 1
	}
	mode := os.FileMode(0o644)
	if fi, serr := os.Stat(opts.target); serr == nil {
		mode = fi.Mode().Perm()
	}

	keptBatches := make([]*document.Batch, 0, len(keptIdx))
	for _, i := range keptIdx {
		keptBatches = append(keptBatches, view.Retained[i])
	}
	in := &persist.WriteInput{
		Profile:         view.Profile,
		State:           view.Projection.State(),
		RetainedBatches: keptBatches,
		AssetRoot:       view.AssetRoot,
		DescriptorsRaw:  view.DescriptorsRaw,
		Blobs:           view.Blobs,
		PreviewEntries:  view.Preview,
		Baseline:        baseline,
	}

	// Projection-equivalence proof BEFORE any byte is written: the
	// candidate must load back to the exact executed projection, and its
	// S1–S5 metadata must match the planned retention.
	candidate, berr2 := buildPackageFn(in)
	if berr2 != nil {
		reportPersistError(stderr, "compact", berr2)
		return 1
	}
	reloaded, rlerr := persist.Load(candidate)
	if rlerr != nil {
		fmt.Fprintf(stderr, "domowik compact: candidate failed validation: %v\n", rlerr)
		return 1
	}
	if !projectionStatesEqual(view.Projection.State(), reloaded.Projection.State()) {
		fmt.Fprintf(stderr, "domowik compact: refusing %s: candidate projection differs from source projection\n", opts.target)
		return 1
	}
	candInsp, ierr2 := inspectBytes(candidate)
	if ierr2 != nil || !candidateMetadataMatches(candInsp, keptBatches) {
		fmt.Fprintf(stderr, "domowik compact: refusing %s: candidate S1-S5 metadata does not match the retention plan\n", opts.target)
		return 1
	}
	res, serr3 := saveDocFn(opts.target, mode, in, hooks)
	if serr3 != nil {
		reportPersistError(stderr, "compact", serr3)
		return 1
	}
	commit := commitReport{
		Phase:            "commit",
		Outcome:          "success",
		Target:           opts.target,
		DurabilityCaveat: res.DurabilityCaveat,
	}
	if opts.json {
		if cerr := emitJSON(stdout, stderr, commit); cerr != 0 {
			return cerr
		}
		return 0
	}
	if commit.DurabilityCaveat {
		fmt.Fprintf(stdout, "warning: directory sync failed or is unsupported; power-loss durability not guaranteed\n")
	}
	fmt.Fprintf(stdout, "committed: replacement written after projection-equivalence proof\n")
	return 0
}

// selectRetention chooses the retained subset per the documented policy:
// at most n batches overall, taken from the NEWEST end of the canonical
// batch order under PER-ACTOR SUFFIX CLOSURE — once an actor's batch is
// folded, every earlier batch of that actor folds with it. This keeps
// each actor's reserved ranges gap-free (B3/§7.6 framing), so the
// candidate always loads, and the snapshot watermark covers every folded
// counter as the compacted prefix. S2–S5 metadata are unaffected: the
// snapshot already folds the complete state, and its S5 recent-batch ids
// are rewritten from exactly the kept set.
func selectRetention(batches []persist.BatchSummary, n int) (keptIdx []int, folded []persist.BatchSummary) {
	droppedActors := map[string]bool{}
	keepFromEnd := make([]bool, len(batches))
	count := 0
	for i := len(batches) - 1; i >= 0; i-- {
		b := batches[i]
		if count < n && !droppedActors[b.ActorID] {
			keepFromEnd[i] = true
			count++
			continue
		}
		droppedActors[b.ActorID] = true
		folded = append([]persist.BatchSummary{b}, folded...)
	}
	for i, keep := range keepFromEnd {
		if keep {
			keptIdx = append(keptIdx, i)
		}
	}
	return keptIdx, folded
}

// inspectBytes wraps the seam for in-memory candidate verification.
func inspectBytes(data []byte) (*persist.Inspection, error) { return persist.Inspect(data) }

// inspectLocalOnly reports whether the inspected history is verifiably
// single-actor: one distinct actor id across the snapshot coverage,
// stored keys, and every reserved batch range. With more than one actor
// the package has synchronized history and no local-only claim is
// provable in this version.
func inspectLocalOnly(insp *persist.Inspection) (verified bool, actors int) {
	set := map[string]bool{}
	for a := range insp.Snapshot.Frontier {
		set[a] = true
	}
	addActor := func(w persist.PositionSummary) {
		if w.Actor != "" {
			set[w.Actor] = true
		}
	}
	for _, k := range insp.Snapshot.WinnerPositions {
		addActor(k.Position)
	}
	for _, k := range insp.Snapshot.CreatedPositions {
		addActor(k.Position)
	}
	for _, k := range insp.Snapshot.Tombstones {
		addActor(k.Position)
	}
	// §5.1's actor universe also spans the frontiers the shared table
	// carries: a referenced batch may observe an actor that appears
	// nowhere else, and a locality claim that missed it would be wrong.
	for _, bp := range insp.Snapshot.BatchPositions {
		for a := range bp.Frontier {
			set[a] = true
		}
	}
	for _, b := range insp.Batches {
		set[b.ActorID] = true
	}
	return len(set) <= 1, len(set)
}

func projectionStatesEqual(a, b document.State) bool {
	if len(a.Entities) != len(b.Entities) {
		return false
	}
	for i := range a.Entities {
		ra, erra := document.EncodeEntity(a.Entities[i])
		rb, errb := document.EncodeEntity(b.Entities[i])
		if erra != nil || errb != nil || string(ra) != string(rb) {
			return false
		}
	}
	eqInt := func(x, y map[document.UUID]int64) bool {
		if len(x) != len(y) {
			return false
		}
		for k, v := range x {
			if yv, ok := y[k]; !ok || yv != v {
				return false
			}
		}
		return true
	}
	eqFieldPos := func(x, y map[document.FieldKey]document.Position) bool {
		if len(x) != len(y) {
			return false
		}
		for k, v := range x {
			yv, ok := y[k]
			if !ok || yv != v {
				return false
			}
		}
		return true
	}
	eqEntityPos := func(x, y map[document.UUID]document.Position) bool {
		if len(x) != len(y) {
			return false
		}
		for k, v := range x {
			yv, ok := y[k]
			if !ok || yv != v {
				return false
			}
		}
		return true
	}
	// The shared batch table is part of the compared state, not derived
	// from it: two snapshots whose positions agree but whose referenced
	// frontiers differ resolve late-concurrent writes differently, because
	// causalPastSize is the sum of those frontiers (§5.1 S3).
	eqBatchPositions := func(x, y map[document.BatchRef]map[document.UUID]int64) bool {
		if len(x) != len(y) {
			return false
		}
		for ref, fx := range x {
			fy, ok := y[ref]
			if !ok || !eqInt(fx, fy) {
				return false
			}
		}
		return true
	}
	return eqInt(a.Frontier, b.Frontier) &&
		eqEntityPos(a.Tombstones, b.Tombstones) &&
		eqFieldPos(a.WinnerPositions, b.WinnerPositions) &&
		eqEntityPos(a.CreatedPositions, b.CreatedPositions) &&
		eqBatchPositions(a.BatchPositions, b.BatchPositions)
}

// candidateMetadataMatches proves the candidate's S1–S5 metadata through
// the parse-only seam: the snapshot section carries exactly the retained
// set's ids, and the retained batches frame identically to the plan.
func candidateMetadataMatches(insp *persist.Inspection, kept []*document.Batch) bool {
	if len(insp.Batches) != len(kept) {
		return false
	}
	want := make([]string, 0, len(kept))
	for i, b := range kept {
		want = append(want, b.BatchID.String())
		if insp.Batches[i].BatchID != want[i] {
			return false
		}
	}
	if len(insp.Snapshot.RecentBatchIDs) != len(want) {
		return false
	}
	for i, id := range want {
		if insp.Snapshot.RecentBatchIDs[i] != id {
			return false
		}
	}
	return true
}

func writeCompactText(stdout io.Writer, p compactPlan, keptIdx []int, batches []persist.BatchSummary) {
	fmt.Fprintf(stdout, "compaction plan for %s\n", p.Target)
	fmt.Fprintf(stdout, "retention: keep newest %d of %d retained batches (%d operations), fold %d batch%s\n",
		p.RetainBatches, p.CurrentBatches, p.CurrentOperations, p.FoldedBatches, pluralS(p.FoldedBatches))
	for _, e := range p.FoldedEntries {
		fmt.Fprintf(stdout, "  fold %s\n", e)
	}
	for _, i := range keptIdx {
		k := batches[i]
		fmt.Fprintf(stdout, "  keep %s batch %s range [%d..%d]\n", k.EntryName, k.BatchID, k.CounterBase, k.EndCounter)
	}
	fmt.Fprintf(stdout, "undo/redo: %s\n", p.UndoRedoEffect)
	fmt.Fprintf(stdout, "recovery: %s\n", p.RecoveryEffect)
	if p.LocalOnlyVerified {
		fmt.Fprintf(stdout, "locality: single-actor history verified locally\n")
	} else {
		fmt.Fprintf(stdout, "locality: NOT verifiable (%s)\n", noSyncProtocol)
	}
	if p.Write {
		fmt.Fprintf(stdout, "write requested: preflight only; commit result follows separately\n")
	}
}
