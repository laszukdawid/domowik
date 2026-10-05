package persist

import (
	"crypto/sha256"
	"encoding/hex"
	"io"
	"os"
)

// baseline.go implements BEHAVIOR-SPEC §9.2 external-modification
// detection: the writer records a baseline over the target's EXACT bytes
// at load time; explicit save refuses silent overwrite when the target's
// current bytes differ, unless the caller supplies explicit user
// confirmation (or performs a save-as). Race boundary declared by the
// spec: a modification landing after this comparison but before the
// replace is outside detection capability and falls under the
// concurrent-writer non-guarantee.

// TargetBaseline records the content digest of one target generation.
type TargetBaseline struct {
	SHA256 string // lowercase hex over the target's exact bytes
}

// BaselineTarget streams the target's bytes into the baseline digest.
func BaselineTarget(path string) (*TargetBaseline, error) {
	f, err := os.Open(path)
	if err != nil {
		if os.IsNotExist(err) {
			return &TargetBaseline{SHA256: ""}, nil // absent target: empty baseline
		}
		return nil, fail(CatMalformedContainer, StageSave, "baseline read failed: %v", err)
	}
	defer f.Close()
	h := sha256.New()
	if _, cerr := io.Copy(h, f); cerr != nil {
		return nil, fail(CatMalformedContainer, StageSave, "baseline hash failed: %v", cerr)
	}
	return &TargetBaseline{SHA256: hex.EncodeToString(h.Sum(nil))}, nil
}

// checkExternalModification enforces §9.2 at explicit save time:
//
//   - target absent: nothing to compare; proceed (F1 territory);
//   - baseline present and matching current bytes: proceed;
//   - baseline present and differing: refuse unless in.BaselineConfirmed;
//   - NO baseline recorded while a target exists: refuse — saving would
//     silently overwrite an unknown generation; confirmation or save-as
//     is required.
func checkExternalModification(target string, in *WriteInput) *Error {
	fi, err := os.Stat(target)
	if err != nil || fi.IsDir() {
		if err == nil {
			return fail(CatMalformedContainer, StageSave,
				"save target %q is a directory", target)
		}
		return nil // no prior target
	}
	current, berr := BaselineTarget(target)
	if berr != nil {
		return berr.(*Error)
	}
	switch {
	case in.Baseline != nil && in.Baseline.SHA256 == current.SHA256:
		return nil
	case in.BaselineConfirmed:
		return nil // explicit user confirmation per §9.2
	default:
		detail := "target bytes differ from the recorded load-time baseline (BEHAVIOR-SPEC §9.2); pass BaselineConfirmed or use save-as"
		if in.Baseline == nil || in.Baseline.SHA256 == "" {
			detail = "no load-time baseline was recorded for an existing target (BEHAVIOR-SPEC §9.2); pass BaselineConfirmed or use save-as"
		}
		return fail(CatSafeWriteRefusal, StageSave, "%s", detail)
	}
}
