package predicates

// Evidence-only H-6 runtime instrumentation for the Phase 0 performance
// criterion (tracker 0030). Output and error semantics of Orientation are
// unchanged by this file: the same validation and decision logic runs on
// every call. The shipped production path does gain two nil-counter
// comparisons per decision (see orientFast's impact note); that disclosed
// performance impact is measured by BenchmarkOrientationAdaptive under
// the registered protocol. Counters are never package globals;
// corpus-constructor filterSign probes never route through this file, so
// they are structurally uncountable here.

type h6Stats struct {
	decisions     int // successful observable sign decisions
	filterToExact int // runtime filter failures that reached orientExact
}

func (s h6Stats) fallbackRate() float64 {
	if s.decisions == 0 {
		return 0
	}
	return float64(s.filterToExact) / float64(s.decisions)
}

// orientationWithStats runs the production Orientation decision path and
// returns the sign together with the per-call H-6 counters.
func orientationWithStats(a, b, c Point) (Sign, h6Stats, error) {
	if err := validateTriangle(a, b, c); err != nil {
		return 0, h6Stats{}, err
	}
	var st h6Stats
	s := orientFastStats(a, b, c, &st)
	return s, st, nil
}
