package cli

import (
	"crypto/rand"
	"errors"
	"flag"
	"fmt"
	"io"
	"math"
	"os"
	"sort"
	"strings"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// domowik apply delivers an external operation script into a stored
// document package. The trust model (issue 0015) separates the UNTRUSTED
// external script schema — command content only — from the trusted
// stored-operation schema: actor identity, counters, batch identity where
// omitted, and causal frontiers are always system-assigned here, never
// read from the script.
//
// Pipeline (Agreement v3):
//
//	P1 parse + normalize (apply_script.go)
//	P2 classify duplicates against the retained log and within the script
//	P3 assign contiguous counters/frontiers to surviving candidates only
//	P4 pre-deliver envelope gates (L23 totals over candidates+retained)
//	P5 one Deliver call: whole-invocation transaction; any rejection rolls
//	   every candidate back before any effect; stale discards are recorded
//	   diagnostics, not errors
//	P6 write-back only when at least one candidate entered the log; an
//	   all-skipped invocation performs no write at all
const applyUsage = `Usage:
  domowik apply --document <path.domo> (--script <path> | --stdin) [flags]

Applies an operation script to a document package. Scripts carry command
content only (batches/operations with explicit operationId); actor id,
counters, and frontier are assigned by domowik and must not appear in the
script. A declared batchId already applied with identical content is
skipped (idempotent redelivery); with differing content the invocation is
rejected as malformed-framing.

Flags:
  --document <path>   target .domo package (required)
  --script <path>     script file, or "-" ... ignored, use --stdin
  --stdin             read the script from standard input
  --dry-run            parse, validate, and report without writing anything
  --actor <uuid>       override the assigned actor epoch (default: fresh per invocation)
  -h, --help           show this help

Exit codes: 0 success (including skipped duplicates and dry-run),
1 rejected or failed apply, 2 usage error.
`

// applyMintUUID is the injectable identity source for system-assigned
// actor epochs and omitted batchIds. Production uses crypto/rand v4;
// deterministic tests install a counter-backed source so no test depends
// on randomness or time (Agreement v3 point b).
var applyMintUUID = func() document.UUID {
	var u document.UUID
	if _, err := rand.Read(u[:]); err != nil {
		panic(fmt.Sprintf("domowik: apply: identity entropy unavailable: %v", err))
	}
	u[6] = (u[6] & 0x0F) | 0x40 // RFC 9562 version nibble
	u[8] = (u[8] & 0x3F) | 0x80 // RFC 9562 variant bits
	return u
}

// applyBaselineTarget is the injectable seam over persist.BaselineTarget
// (BEHAVIOR-SPEC §9.2 external-modification detection). It is called ONLY
// in the P6 write section; deterministic tests substitute it to prove that
// a --dry-run invocation never touches the save-side baseline path.
var applyBaselineTarget = persist.BaselineTarget

// stdinReader is the process standard-input source for --stdin; a
// package-level variable so deterministic tests can substitute it.
var stdinReader io.Reader = os.Stdin

// runApply executes `domowik apply` and returns the process exit code.
func runApply(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("apply", flag.ContinueOnError)
	fs.SetOutput(stderr)
	fs.Usage = func() { fmt.Fprint(stderr, applyUsage) }
	docPath := fs.String("document", "", "target .domo package")
	scriptPath := fs.String("script", "", "operation script path")
	useStdin := fs.Bool("stdin", false, "read the script from standard input")
	dryRun := fs.Bool("dry-run", false, "report projected changes without writing")
	actorText := fs.String("actor", "", "actor epoch UUID override")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	if fs.NArg() > 0 {
		fmt.Fprintf(stderr, "domowik: apply: unexpected argument %q\n\n", fs.Arg(0))
		fmt.Fprint(stderr, applyUsage)
		return 2
	}

	opts, uerr := applyOptions(fs, *docPath, *scriptPath, *useStdin, *dryRun, *actorText)
	if uerr != nil {
		fmt.Fprintf(stderr, "domowik: apply: %s\n\n", uerr.Error())
		fmt.Fprint(stderr, applyUsage)
		return 2
	}
	code, err := func() (int, error) {
		var scriptIn io.Reader = stdinReader
		if !opts.stdin {
			f, ferr := os.Open(opts.scriptPath)
			if ferr != nil {
				return 1, applyErr("malformed-container", "script %q could not be opened", opts.scriptPath)
			}
			defer f.Close()
			scriptIn = f
		}
		return runApplyPipeline(opts, scriptIn, stdout)
	}()
	if err != nil {
		fmt.Fprintf(stderr, "domowik: apply: %s\n", err.Error())
		if code == 0 {
			code = 1
		}
	}
	return code
}

type applyOpts struct {
	docPath    string
	scriptPath string // "" with stdin
	stdin      bool
	dryRun     bool
	actor      document.UUID // zero: mint per invocation
}

func applyOptions(fs *flag.FlagSet, docPath, scriptPath string, stdin bool, dryRun bool, actorText string) (applyOpts, error) {
	if docPath == "" {
		return applyOpts{}, applyErr(codeUsage, "--document is required")
	}
	if stdin && scriptPath != "" {
		return applyOpts{}, applyErr(codeUsage, "--stdin and --script are mutually exclusive")
	}
	if !stdin && scriptPath == "" {
		return applyOpts{}, applyErr(codeUsage, "one of --script or --stdin is required")
	}
	opts := applyOpts{docPath: docPath, scriptPath: scriptPath, stdin: stdin, dryRun: dryRun}
	if actorText != "" {
		actor, err := document.ParseUUID(actorText)
		if err != nil || actor == (document.UUID{}) {
			return applyOpts{}, applyErr(codeUsage, "--actor must be a canonical RFC 9562 UUID")
		}
		opts.actor = actor
	}
	return opts, nil
}

// readScriptInput reads the whole script bounded by L2 (total physical
// input bytes): one byte beyond the cap rejects before allocation can
// follow hostile input.
func readScriptInput(r io.Reader) ([]byte, error) {
	data, err := io.ReadAll(io.LimitReader(r, persist.LimitTotalPhysicalBytes+1))
	if err != nil {
		return nil, applyErr("malformed-container", "script could not be read: %v", err)
	}
	if int64(len(data)) > persist.LimitTotalPhysicalBytes {
		return nil, applyErr("limit-total-physical", "script exceeds %d input bytes", persist.LimitTotalPhysicalBytes)
	}
	return data, nil
}

type candidateBatch struct {
	batchID document.UUID
	ops     []*document.Operation
	label   string // deterministic report label, e.g. "batch 2 (uuid)"
}

type applyReport struct {
	appliedBatches int
	skippedBatches int
	staleBatches   int
	created        []document.UUID
	deleted        []document.UUID
	modified       []document.UUID
	warnings       []string
	diagnostics    []string
	lines          []string // buffered per-batch outcome notes
}

// runApplyPipeline executes phases P2–P6 of the approved agreement and
// reports outcomes on stdout. All human output is buffered and emitted
// only on a terminal success/dry-run outcome, so a failed invocation
// never mixes streams. A non-nil error carries an exact rejection token
// for stderr.
func runApplyPipeline(opts applyOpts, scriptIn io.Reader, stdout io.Writer) (int, error) {
	// P1: read + parse + normalize the untrusted script.
	data, rerr := readScriptInput(scriptIn)
	if rerr != nil {
		return 1, rerr
	}
	script, perr := parseScript(data)
	if perr != nil {
		return 1, perr
	}

	// Load the target (read-only). Everything below this line either
	// finishes in memory or goes through the guarded save seam; the
	// save-side baseline is acquired only in the P6 write section, so a
	// dry-run never touches that path at all.
	view, lerr := persist.LoadFile(opts.docPath)
	if lerr != nil {
		return 1, applyPersistError(lerr)
	}
	proj := view.Projection
	stateBefore := proj.State()

	// P2: classify duplicates BEFORE any counter math (no skipped batch
	// may consume a counter slot).
	retainedByID := make(map[document.UUID]*document.Batch, len(view.Retained))
	for _, b := range view.Retained {
		retainedByID[b.BatchID] = b
	}
	candidates := make([]*candidateBatch, 0, len(script))
	report := &applyReport{}
	for i, sb := range script {
		label := fmt.Sprintf("batch %d", i+1)
		if sb.declaredStr != "" {
			label += " (" + sb.declaredStr + ")"
		}
		// Within-script duplicates: identical content collapses onto the
		// first occurrence; differing content is a framing reject.
		dup := false
		for _, prev := range candidates {
			if prev.batchID != (document.UUID{}) && prev.batchID == sb.declaredID {
				if sameCommandContent(prev.ops, sb.operations) {
					dup = true
					break
				}
				return 1, applyErr(codeMalformedFraming,
					"%s repeats batchId %s with differing command content",
					label, sb.declaredID.String())
			}
		}
		if dup {
			continue
		}
		// Stored-identical screen: same declared batchId in the retained
		// log with identical normalized command content => skip silently
		// but report (I2/V1); differing content => framing anomaly.
		if sb.declaredID != (document.UUID{}) {
			if stored, seen := retainedByID[sb.declaredID]; seen {
				if sameCommandContent(stored.Operations, sb.operations) {
					report.skippedBatches++
					report.lines = append(report.lines,
						fmt.Sprintf("%s: skipped (duplicate of applied batch)", label))
					continue
				}
				return 1, applyErr(codeMalformedFraming,
					"%s matches retained batchId %s but carries differing command content",
					label, sb.declaredID.String())
			}
		}
		id := sb.declaredID
		if id == (document.UUID{}) {
			id = applyMintUUID()
		}
		candidates = append(candidates, &candidateBatch{batchID: id, ops: sb.operations, label: label})
	}

	// P4 (before assignment/delivery): envelope gates over candidates plus
	// retained history; also the early safe-write refusal gate so an
	// unpreservable package fails before any in-memory mutation decision.
	retainedOps := 0
	for _, b := range view.Retained {
		retainedOps += len(b.Operations)
	}
	candidateOps := 0
	for _, c := range candidates {
		candidateOps += len(c.ops)
	}
	if candidateOps+retainedOps > persist.LimitTotalRetainedOps {
		return 1, applyErr("limit-operation-count",
			"post-apply retained operations (%d existing + %d new) would exceed %d (L23)",
			retainedOps, candidateOps, persist.LimitTotalRetainedOps)
	}
	if len(candidates)+len(view.Retained) > persist.LimitBatchesPerPackage {
		return 1, applyErr("limit-operation-count",
			"post-apply batch count (%d existing + %d new) would exceed %d (L23)",
			len(view.Retained), len(candidates), persist.LimitBatchesPerPackage)
	}
	refusals := collectRefusalReasons(view)
	if !opts.dryRun && len(refusals) > 0 {
		return 1, applyErr("safe-write-refusal",
			"target carries unpreservable optional data; refusing write-back: %s",
			strings.Join(refusals, "; "))
	}

	// P3: assign contiguous counters and honest frontiers to surviving
	// candidates only.
	actor := opts.actor
	if actor == (document.UUID{}) {
		actor = applyMintUUID()
	}
	watermark := stateBefore.Frontier[actor]
	base := watermark + 1
	batches := make([]*document.Batch, 0, len(candidates))
	for _, c := range candidates {
		if uint64(base)+uint64(len(c.ops))-1 > math.MaxInt64 {
			return 1, applyErr("limit-operation-count",
				"assigned counter range ending at %d overflows signed 64-bit (L13)",
				uint64(base)+uint64(len(c.ops))-1)
		}
		frontier := make(map[document.UUID]int64, len(stateBefore.Frontier)+1)
		for a, n := range stateBefore.Frontier {
			frontier[a] = n
		}
		if base > 1 {
			frontier[actor] = base - 1 // own prior reservations observed honestly (F1/B5)
		}
		b, berr := document.NewBatch(c.batchID, actor, base, frontier, c.ops...)
		if berr != nil {
			return 1, applyErr(codeMalformedFraming, "candidate batch %s failed framing: %v", c.batchID.String(), berr)
		}
		batches = append(batches, b)
		base += int64(len(c.ops))
	}

	// P5: ONE Deliver call = whole-invocation transaction. Any rejection
	// rolls every candidate back before any effect; stale discards are
	// diagnostics, never errors.
	diagBefore := len(proj.Diagnostics())
	if len(batches) > 0 {
		if derr := proj.Deliver(batches...); derr != nil {
			return 1, applyDeliverError(derr)
		}
	}
	report.appliedBatches = len(batches)

	// Diff projection entities and collect new diagnostics/warnings.
	beforeSet := make(map[document.UUID]document.Entity, len(stateBefore.Entities))
	for _, e := range stateBefore.Entities {
		beforeSet[e.EntityID()] = e
	}
	afterState := proj.State()
	afterSet := make(map[document.UUID]document.Entity, len(afterState.Entities))
	for _, e := range afterState.Entities {
		afterSet[e.EntityID()] = e
	}
	for _, e := range afterState.Entities {
		prev, ok := beforeSet[e.EntityID()]
		if !ok {
			report.created = append(report.created, e.EntityID())
			continue
		}
		prevRaw, perr := document.EncodeEntity(prev)
		nowRaw, nerr := document.EncodeEntity(e)
		if perr != nil || nerr != nil || string(prevRaw) != string(nowRaw) {
			report.modified = append(report.modified, e.EntityID())
		}
	}
	for _, e := range stateBefore.Entities {
		if _, ok := afterSet[e.EntityID()]; !ok {
			report.deleted = append(report.deleted, e.EntityID())
		}
	}
	sortUUIDSlice(report.created)
	sortUUIDSlice(report.modified)
	sortUUIDSlice(report.deleted)

	newDiags := proj.Diagnostics()[diagBefore:]
	staleByID := map[document.UUID]bool{}
	for _, d := range newDiags {
		line := fmt.Sprintf("%s: batch %s: %s", d.Code, d.BatchID.String(), d.Detail)
		report.diagnostics = append(report.diagnostics, line)
		if d.Code == document.DiagStaleBatchDiscard {
			report.staleBatches++
			report.appliedBatches--
			staleByID[d.BatchID] = true
		}
	}
	// Per-batch outcomes, buffered so a failed invocation never mixes
	// streams (Agreement v3 finding 1).
	for i, b := range batches {
		if staleByID[b.BatchID] {
			report.lines = append(report.lines, fmt.Sprintf("%s: discarded stale (target absent or tombstoned at its canonical position; I5)", candidates[i].label))
			continue
		}
		report.lines = append(report.lines, fmt.Sprintf("%s: applied (%d ops)", candidates[i].label, len(b.Operations)))
	}
	report.warnings = append(report.warnings, missingAssetWarnings(view)...)

	if opts.dryRun {
		printReport(opts, report, stdout)
		fmt.Fprintf(stdout, "dry-run complete: no writes performed\n")
		return 0, nil
	}
	if len(batches) == 0 {
		// All-skip invocation: zero effective change, zero writes (I2/V1).
		printReport(opts, report, stdout)
		return 0, nil
	}

	// P6: guarded write-back through the save seam with external-
	// modification detection.
	mode := os.FileMode(0o644)
	if fi, serr := os.Stat(opts.docPath); serr == nil {
		mode = fi.Mode().Perm()
	}
	baseline, berr := applyBaselineTarget(opts.docPath)
	if berr != nil {
		return 1, applyPersistError(berr)
	}
	input := &persist.WriteInput{
		Profile:         view.Profile,
		State:           proj.State(),
		RetainedBatches: proj.Log(),
		AssetRoot:       view.AssetRoot,
		DescriptorsRaw:  view.DescriptorsRaw,
		Blobs:           view.Blobs,
		PreviewEntries:  view.Preview,
		RefusalReasons:  refusals,
		Baseline:        baseline,
	}
	if _, serr := persist.SaveDocument(opts.docPath, mode, input, nil); serr != nil {
		return 1, applyPersistError(serr)
	}
	printReport(opts, report, stdout)
	fmt.Fprintf(stdout, "saved %s\n", opts.docPath)
	return 0, nil
}

func sortUUIDSlice(ids []document.UUID) {
	document.SortUUIDs(ids)
}

func collectRefusalReasons(view *persist.PackageView) []string {
	var out []string
	out = append(out, view.ManifestUnknown...)
	out = append(out, view.SnapshotUnknown...)
	for name, members := range view.BatchUnknown {
		out = append(out, fmt.Sprintf("%s: %s", name, strings.Join(members, ", ")))
	}
	sort.Strings(out)
	return out
}

func missingAssetWarnings(view *persist.PackageView) []string {
	out := make([]string, 0, len(view.MissingAssets))
	for _, digest := range view.MissingAssets {
		out = append(out, fmt.Sprintf("missing asset treated as absent blob: %s", digest))
	}
	out = append(out, view.Diagnostics...)
	return out
}

func applyPersistError(err error) error {
	if pe, ok := err.(*persist.Error); ok {
		return applyErr(string(pe.Category), "%s (%s)", pe.Detail, pe.Stage)
	}
	return applyErr("malformed-container", "%v", err)
}

func applyDeliverError(err error) error {
	var bre *document.BatchRejectedError
	if errors.As(err, &bre) {
		var fe *document.FieldError
		if errors.As(bre.Err, &fe) {
			// Preserve the underlying document diagnostic code (e.g.
			// unknown-field, nonfinite-value, zero-length-wall) instead of
			// collapsing every rejection to invalid-field-syntax.
			return applyErr(fe.Code, "batch %s rejected before any effect: %s",
				bre.BatchID.String(), fe.Detail)
		}
		return applyErr(codeInvalidFieldSyntax, "batch %s rejected before any effect: %v",
			bre.BatchID.String(), bre.Err)
	}
	switch {
	case errors.Is(err, document.ErrMalformedFraming):
		return applyErr(codeMalformedFraming, "%v", err)
	case errors.Is(err, document.ErrCausalPrerequisiteMissing):
		return applyErr("causal-prerequisite-missing", "%v", err)
	default:
		return applyErr("internal-failure", "%v", err)
	}
}

func printReport(opts applyOpts, r *applyReport, stdout io.Writer) {
	head := "apply"
	if opts.dryRun {
		head = "apply (dry-run)"
	}
	fmt.Fprintf(stdout, "%s: %d applied, %d skipped, %d discarded stale; entities +%d -%d ~%d\n",
		head, r.appliedBatches, r.skippedBatches, r.staleBatches,
		len(r.created), len(r.deleted), len(r.modified))
	for _, l := range r.lines {
		fmt.Fprintf(stdout, "  %s\n", l)
	}
	for _, label := range []struct {
		name string
		ids  []document.UUID
	}{
		{"created", r.created},
		{"deleted", r.deleted},
		{"modified", r.modified},
	} {
		for _, id := range label.ids {
			fmt.Fprintf(stdout, "  %s: %s\n", label.name, id.String())
		}
	}
	for _, w := range r.warnings {
		fmt.Fprintf(stdout, "  warning: %s\n", w)
	}
	for _, d := range r.diagnostics {
		fmt.Fprintf(stdout, "  diagnostic: %s\n", d)
	}
}
