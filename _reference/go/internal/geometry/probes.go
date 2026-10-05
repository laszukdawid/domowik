package geometry

import "sync/atomic"

// Spike-local probe statistics recorded as evidence (H-register /
// probe-escalation counts). They carry no pass/fail authority.
var (
	statProbeEscalations atomic.Int64
	statProbeBudget      atomic.Int64
)

// ProbeEscalations reports how often the bounded nudge had to shrink
// before classifying cleanly.
func ProbeEscalations() int64 { return statProbeEscalations.Load() }

// ProbeBudgetRejects reports deterministic rejections after the nudge
// halving budget was exhausted (narrow-feature configurations).
func ProbeBudgetRejects() int64 { return statProbeBudget.Load() }

const maxProbeHalvings = 8

// isBoundaryErr reports whether err is the recoverable
// probe-on-boundary condition.
func isBoundaryErr(err error) bool {
	se, ok := err.(*SpikeError)
	return ok && se.Code == "probe-on-boundary"
}

// probeWithRetryB is the closure-parameterized variant: the point
// builder is passed explicitly, keeping the helper race-free for
// concurrent benchmarks.
func probeWithRetryB(
	eps xnum,
	build func(e xnum) (oPt, oPt),
	classify func(pL, pR oPt) (bool, bool, error),
) (inL, inR bool, finalEps xnum, err error) {
	e := eps
	for k := 0; ; k++ {
		pL, pR := build(e)
		inL, inR, err = classify(pL, pR)
		if err == nil {
			if k > 0 {
				statProbeEscalations.Add(1)
			}
			return inL, inR, e, nil
		}
		if !isBoundaryErr(err) || k == maxProbeHalvings {
			statProbeBudget.Add(1)
			return false, false, e, err
		}
		e = xhalf(e)
	}
}
