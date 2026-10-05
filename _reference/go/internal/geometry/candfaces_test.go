package geometry

import (
	"math"
	"testing"
)

func croomBands(x0, y0, x1, y1, t float64) [][2][2]float64 {
	var segs [][2][2]float64
	addRect := func(ax, ay, bx, by float64) {
		pts := [][2]float64{{ax, ay}, {bx, ay}, {bx, by}, {ax, by}}
		for i := range pts {
			j := (i + 1) % len(pts)
			segs = append(segs, [2][2]float64{pts[i], pts[j]})
		}
	}
	addRect(x0, y0, x1, y0+t)
	addRect(x0, y1-t, x1, y1)
	addRect(x0, y0+t, x0+t, y1-t)
	addRect(x1-t, y0+t, x1, y1-t)
	return segs
}

func TestCandFacesSingleRoom(t *testing.T) {
	arr, err := CandFaces(croomBands(0, 0, 12, 12, 2))
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 1 || arr.Faces[0].Parent != -1 || len(arr.Faces[0].Holes) != 0 {
		t.Fatalf("single room structure wrong: %+v", arr.Faces)
	}
	want := [][2]float64{{2, 2}, {10, 2}, {10, 10}, {2, 10}}
	for i, v := range arr.Faces[0].Outer {
		if v != want[i] {
			t.Fatalf("room ring mismatch at %d: %v", i, v)
		}
	}
}

func TestCandFacesDisjoint(t *testing.T) {
	segs := croomBands(0, 0, 12, 12, 2)
	segs = append(segs, croomBands(40, 0, 52, 12, 2)...)
	arr, err := CandFaces(segs)
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 2 || len(arr.Exterior.Holes) != 2 {
		t.Fatalf("disjoint rooms: faces=%d extHoles=%d", len(arr.Faces), len(arr.Exterior.Holes))
	}
}

func TestCandFacesPinchSplitting(t *testing.T) {
	segs := croomBands(0, 0, 30, 30, 2)
	addRect := func(ax, ay, bx, by float64) {
		pts := [][2]float64{{ax, ay}, {bx, ay}, {bx, by}, {ax, by}}
		for i := range pts {
			j := (i + 1) % len(pts)
			segs = append(segs, [2][2]float64{pts[i], pts[j]})
		}
	}
	addRect(10, 2, 18, 18)
	addRect(18, 18, 26, 28)
	arr, err := CandFaces(segs)
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 2 {
		t.Fatalf("pinch must split into 2 faces, got %d", len(arr.Faces))
	}
	for i, f := range arr.Faces {
		if f.Parent != -1 {
			t.Fatalf("pinch face %d must not nest", i)
		}
	}
}

func TestCandFacesCourtyardIsland(t *testing.T) {
	segs := croomBands(0, 0, 30, 30, 2)
	addRect := func(ax, ay, bx, by float64) {
		pts := [][2]float64{{ax, ay}, {bx, ay}, {bx, by}, {ax, by}}
		for i := range pts {
			j := (i + 1) % len(pts)
			segs = append(segs, [2][2]float64{pts[i], pts[j]})
		}
	}
	// Hollow island enclosing (12,16)x(12,18)-ish interior.
	addRect(10, 10, 20, 12)
	addRect(10, 16, 20, 18)
	addRect(10, 12, 12, 16)
	addRect(18, 12, 20, 16)
	arr, err := CandFaces(segs)
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 2 {
		t.Fatalf("courtyard+island expected 2 faces, got %d", len(arr.Faces))
	}
	courtyard := arr.Faces[0]
	if len(courtyard.Holes) != 1 {
		t.Fatalf("courtyard must carry the island loop as hole")
	}
	island := arr.Faces[1]
	if island.Parent != 0 {
		t.Fatalf("island must nest inside courtyard")
	}
	if len(arr.Exterior.Holes) != 1 {
		t.Fatalf("exterior carries the enclosure loop")
	}
}

func TestCandFacesLargeTranslation(t *testing.T) {
	segs := croomBands(4e6, 4e6, 4e6+12, 4e6+12, 2)
	arr, err := CandFaces(segs)
	if err != nil {
		t.Fatal(err)
	}
	if len(arr.Faces) != 1 {
		t.Fatalf("translated room structure wrong")
	}
}

func TestCandFacesDomainExceeded(t *testing.T) {
	segs := [][2][2]float64{{{0, 0}, {1e9, 0}}, {{1e9, 0}, {1e9, 1}}, {{1e9, 1}, {0, 0}}}
	if _, err := CandFaces(segs); err == nil {
		t.Fatal("near-B input must be rejected by face domain narrowing")
	}
	bad := [][2][2]float64{{[2]float64{0, 0}, [2]float64{2.5, 0}}, {[2]float64{2.5, 0}, [2]float64{1, 2}}, {[2]float64{1, 2}, [2]float64{0, 0}}}
	if _, err := CandFaces(bad); err == nil {
		t.Fatal("non-integral input must be rejected by face domain narrowing")
	}
	nonfinite := [][2][2]float64{{[2]float64{0, 0}, [2]float64{math.NaN(), 0}}, {[2]float64{1, 0}, [2]float64{1, 2}}, {[2]float64{1, 2}, [2]float64{0, 0}}}
	if _, err := CandFaces(nonfinite); err == nil {
		t.Fatal("non-finite input must reject")
	}
}

func TestCandFacesNonAxisRejected(t *testing.T) {
	diag := [][2][2]float64{{{0, 0}, {3, 3}}, {{3, 3}, {3, 6}}, {{3, 6}, {0, 0}}}
	if _, err := CandFaces(diag); err == nil {
		t.Fatal("non-axis integral segment must be rejected by domain narrowing")
	}
}

// exact-ring helpers for AC4-style assertions
func ringsEqual(a, b [][2]float64) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func TestCandFacesExactRings(t *testing.T) {
	arr, err := CandFaces(croomBands(0, 0, 12, 12, 2))
	if err != nil {
		t.Fatal(err)
	}
	wantOuter := [][2]float64{{2, 2}, {10, 2}, {10, 10}, {2, 10}}
	if !ringsEqual(arr.Faces[0].Outer, wantOuter) {
		t.Fatalf("room outer %v want %v", arr.Faces[0].Outer, wantOuter)
	}
	wantHole := [][2]float64{{0, 0}, {0, 12}, {12, 12}, {12, 0}}
	if len(arr.Exterior.Holes) != 1 || !ringsEqual(arr.Exterior.Holes[0], wantHole) {
		t.Fatalf("exterior hole %v", arr.Exterior.Holes)
	}
}

func TestCandFacesCourtyardExact(t *testing.T) {
	segs := croomBands(0, 0, 30, 30, 2)
	addRect := func(ax, ay, bx, by float64) {
		pts := [][2]float64{{ax, ay}, {bx, ay}, {bx, by}, {ax, by}}
		for i := range pts {
			j := (i + 1) % len(pts)
			segs = append(segs, [2][2]float64{pts[i], pts[j]})
		}
	}
	addRect(10, 10, 20, 12)
	addRect(10, 16, 20, 18)
	addRect(10, 12, 12, 16)
	addRect(18, 12, 20, 16)
	arr, err := CandFaces(segs)
	if err != nil {
		t.Fatal(err)
	}
	courtyard := arr.Faces[0]
	wantCourtyard := [][2]float64{{2, 2}, {28, 2}, {28, 28}, {2, 28}}
	if !ringsEqual(courtyard.Outer, wantCourtyard) {
		t.Fatalf("courtyard outer %v", courtyard.Outer)
	}
	wantIslandHole := [][2]float64{{10, 10}, {10, 18}, {20, 18}, {20, 10}}
	if len(courtyard.Holes) != 1 || !ringsEqual(courtyard.Holes[0], wantIslandHole) {
		t.Fatalf("island hole %v", courtyard.Holes)
	}
	island := arr.Faces[1]
	wantIsland := [][2]float64{{12, 12}, {18, 12}, {18, 16}, {12, 16}}
	if island.Parent != 0 || !ringsEqual(island.Outer, wantIsland) {
		t.Fatalf("island face %+v", island)
	}
}

func TestCandFacesReversalInvariance(t *testing.T) {
	fwd, err := CandFaces(croomBands(0, 0, 12, 12, 2))
	if err != nil {
		t.Fatal(err)
	}
	// Reverse every input segment's direction: canonical output must
	// be identical.
	var revSegs [][2][2]float64
	for _, sg := range croomBands(0, 0, 12, 12, 2) {
		revSegs = append(revSegs, [2][2]float64{sg[1], sg[0]})
	}
	rev, err := CandFaces(revSegs)
	if err != nil {
		t.Fatal(err)
	}
	if len(fwd.Faces) != len(rev.Faces) {
		t.Fatal("face counts differ under reversal")
	}
	for i := range fwd.Faces {
		if !ringsEqual(fwd.Faces[i].Outer, rev.Faces[i].Outer) ||
			fwd.Faces[i].Parent != rev.Faces[i].Parent ||
			len(fwd.Faces[i].Holes) != len(rev.Faces[i].Holes) {
			t.Fatalf("canonical output depends on segment direction at face %d", i)
		}
	}
	if len(fwd.Exterior.Holes) != len(rev.Exterior.Holes) {
		t.Fatal("exterior hole counts differ")
	}
	for i := range fwd.Exterior.Holes {
		if !ringsEqual(fwd.Exterior.Holes[i], rev.Exterior.Holes[i]) {
			t.Fatalf("exterior holes differ under reversal at %d", i)
		}
	}
}

func TestCandFacesCoincidentMultiplicity(t *testing.T) {
	// Valid butt-seam pair (opposed directions, count 2): must pass —
	// croomBands itself contains such seams.
	if _, err := CandFaces(croomBands(0, 0, 12, 12, 2)); err != nil {
		t.Fatalf("valid butt seams rejected: %v", err)
	}
	// One opposite copy forms a valid pair with the existing x=12
	// edge (count 2): must succeed.
	segs := croomBands(0, 0, 12, 12, 2)
	segs = append(segs, [2][2]float64{{12, 0}, {12, 2}})
	if _, err := CandFaces(segs); err != nil {
		t.Fatalf("valid opposed pair rejected: %v", err)
	}
	// A third coincident copy exceeds the at-most-two invariant and
	// must reject deterministically.
	segs = append(segs, [2][2]float64{{12, 0}, {12, 2}})
	if _, err := CandFaces(segs); err == nil {
		t.Fatal("three coincident fragments must reject")
	}
}

func TestCandFacesIsolatedOpposedPair(t *testing.T) {
	// Minimal exact pair: one segment plus its reversed copy — the
	// opposed-pair tier must cancel and yield no faces.
	segs := [][2][2]float64{
		{{0, 0}, {4, 0}},
		{{4, 0}, {0, 0}},
	}
	arr, err := CandFaces(segs)
	if err != nil {
		t.Fatalf("opposed pair must cancel cleanly: %v", err)
	}
	if len(arr.Faces) != 0 {
		t.Fatalf("cancelled seam must leave no faces")
	}
	// A third coincident copy exceeds the at-most-two invariant.
	segs = append(segs, [2][2]float64{{0, 0}, {4, 0}})
	if _, err := CandFaces(segs); err == nil {
		t.Fatal("third coincident fragment must reject")
	}
}
