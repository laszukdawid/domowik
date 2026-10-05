package persist

import (
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
)

// recovery.go: recovery-state observability (§9.7). Discovery/admission
// treats every artifact as untrusted input under §9.1: candidates are
// admitted only after full reader validation. Same-device artifacts
// order solely by the greatest monotonic sequence number embedded in the
// artifact name; timestamps are display-only and never determine order.

// ArtifactInfo describes one discovered temporary/recovery candidate.
type ArtifactInfo struct {
	Path string
	Seq  uint64 // monotonic per-device sequence from the name
}

// Candidate is one validated recovery candidate.
type Candidate struct {
	Artifact ArtifactInfo
	Category Category // state-stale-candidate or state-valid-target scope
}

// RecoveryReport carries the machine-readable RS categories for one
// target plus the deterministic actions taken.
type RecoveryReport struct {
	TargetCategory   Category    // RS1/RS5/RS7 classification of the target
	Diagnostics      []string    // per-artifact categories (RS3/RS6 etc.)
	Candidates       []Candidate // valid, never auto-promoted
	Ambiguous        bool        // RS8: same-sequence collision
	DurabilityCaveat bool
}

// DiscoverArtifacts lists leftover candidates for a target, ordered by
// descending sequence (greatest first). Names not matching the writer's
// L30 scheme are reported as ignorable foreign leftovers.
func DiscoverArtifacts(target string) []ArtifactInfo {
	dir := filepath.Dir(target)
	base := filepath.Base(target)
	prefix := base + tempArtifactInfix
	dirents, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	var out []ArtifactInfo
	for _, de := range dirents {
		name := de.Name()
		if !strings.HasPrefix(name, prefix) {
			continue
		}
		seqText := strings.TrimPrefix(name, prefix)
		if i := strings.IndexByte(seqText, '-'); i >= 0 {
			seqText = seqText[:i]
		}
		seq, serr := strconv.ParseUint(seqText, 10, 64)
		if serr != nil {
			continue // foreign leftover; ignorable and cleanable
		}
		out = append(out, ArtifactInfo{Path: filepath.Join(dir, name), Seq: seq})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Seq > out[j].Seq })
	// L30 named quantity: admission caps candidate count; overflow beyond
	// it excludes the oldest by sequence (never auto-deleted).
	if len(out) > RecoveryArtifactCountMax {
		out = out[:RecoveryArtifactCountMax]
	}
	return out
}

// Recover classifies the target and its artifacts into the RS1–RS8
// observable states with their machine-readable categories.
func Recover(target string) (*RecoveryReport, *Error) {
	report := &RecoveryReport{Diagnostics: []string{}, Candidates: []Candidate{}}

	targetView, terr := LoadFile(target)
	switch {
	case terr == nil:
		report.TargetCategory = StateValidTarget // RS1/RS2
		_ = targetView
	default:
		report.TargetCategory = StateCorruptTarget // RS5: fall to artifact precedence
		report.Diagnostics = append(report.Diagnostics,
			string(StateCorruptTarget)+": "+terr.Error())
	}

	artifacts := DiscoverArtifacts(target)
	seenSeq := map[uint64]string{}
	for _, art := range artifacts {
		if prev, dup := seenSeq[art.Seq]; dup {
			// RS8: same-sequence collision requires explicit user choice.
			report.Ambiguous = true
			report.Diagnostics = append(report.Diagnostics,
				string(StateAmbiguousRecovery)+": "+prev+" and "+art.Path+" share a sequence")
			continue
		}
		seenSeq[art.Seq] = art.Path
		_, lerr := LoadFile(art.Path)
		if lerr != nil {
			// RS3/RS6: partial or corrupt candidate — excluded from
			// precedence, reported, auto-cleaned only if provably not a
			// package.
			report.Diagnostics = append(report.Diagnostics,
				string(StateInvalidCandidate)+": "+lerr.Error())
			continue
		}
		report.Candidates = append(report.Candidates, Candidate{
			Artifact: art,
			Category: StateValidTarget,
		})
	}
	if report.TargetCategory == StateCorruptTarget &&
		len(report.Candidates) == 0 && !report.Ambiguous && !anyCandidateMentioned(report) {
		report.TargetCategory = StateNoRecovery // RS7
	}
	return report, nil
}

func anyCandidateMentioned(r *RecoveryReport) bool {
	for _, d := range r.Diagnostics {
		if strings.Contains(d, string(StateInvalidCandidate)) {
			return true
		}
	}
	return false
}

// CleanLeftovers removes only provably-not-a-package leftovers (RS3
// auto-clean rule): files whose leading bytes lack a local-header
// signature. Anything else survives until explicit user action (F8:
// cleanup failure leaves artifacts valid-or-excluded, never partially
// promoted).
func CleanLeftovers(target string) (removed []string, kept []string) {
	for _, art := range DiscoverArtifacts(target) {
		sig := make([]byte, 4)
		f, err := os.Open(art.Path)
		if err != nil {
			kept = append(kept, art.Path)
			continue
		}
		_, rerr := f.Read(sig)
		f.Close()
		if rerr != nil || string(sig) != "PK\x03\x04" {
			if rmErr := os.Remove(art.Path); rmErr == nil {
				removed = append(removed, art.Path)
				continue
			}
		}
		kept = append(kept, art.Path)
	}
	return removed, kept
}
