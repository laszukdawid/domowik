package sizecorpus

import (
	"encoding/json"
	"fmt"
	"strconv"
	"strings"

	"domowik.dev/domowik/internal/document"
)

// checkGeometry asserts the §10.9/ED-0025 construction properties directly
// from the emitted coordinates, independently of the formulas that produced
// them: closed ring loops of exact perimeter, splits exactly at one third
// and two thirds of each edge (hence strictly interior), join endpoint
// coincidence with matching roles, strict R4.1.2 opening fit on each host,
// and room seeds at their ring centres.
func checkGeometry(d *Document) error {
	walls := map[string]wallRec{}
	idToPath := map[string]string{}
	openings := map[string]openingRec{}
	for i := range d.Objects {
		o := &d.Objects[i]
		switch o.Kind {
		case document.KindWall:
			var raw struct {
				ID        string  `json:"id"`
				StartMm   xyPair  `json:"startMm"`
				EndMm     xyPair  `json:"endMm"`
				Thickness float64 `json:"thicknessMm"`
			}
			if err := json.Unmarshal(o.JSON, &raw); err != nil {
				return fmt.Errorf("entity %s: %v", o.Path, err)
			}
			l, r, edge, k, perr := parseWallPath(o.Path)
			if perr != nil {
				return fmt.Errorf("entity %s: %v", o.Path, perr)
			}
			w := wallRec{id: raw.ID, l: l, r: r, edge: edge, k: k,
				start: pointValue{raw.StartMm[0], raw.StartMm[1]},
				end:   pointValue{raw.EndMm[0], raw.EndMm[1]}, thickness: raw.Thickness}
			walls[o.Path] = w
			idToPath[raw.ID] = o.Path
		case document.KindOpening:
			var raw struct {
				HostWallID string  `json:"hostWallId"`
				U          float64 `json:"u"`
				WidthMm    float64 `json:"widthMm"`
			}
			if err := json.Unmarshal(o.JSON, &raw); err != nil {
				return fmt.Errorf("entity %s: %v", o.Path, err)
			}
			openings[o.Path] = openingRec{path: o.Path, hostID: raw.HostWallID, u: raw.U, width: raw.WidthMm}
		case document.KindRoomRecord:
			var raw struct {
				SeedMm xyPair `json:"seedMm"`
			}
			if err := json.Unmarshal(o.JSON, &raw); err != nil {
				return fmt.Errorf("entity %s: %v", o.Path, err)
			}
			l, r, perr := parseRoomPath(o.Path)
			if perr != nil {
				return fmt.Errorf("entity %s: %v", o.Path, perr)
			}
			if err := checkRoomSeed(o.Path, l, r, raw.SeedMm); err != nil {
				return err
			}
		}
	}

	if err := checkRingLoops(walls); err != nil {
		return err
	}
	if err := checkJoins(d, walls); err != nil {
		return err
	}
	for _, op := range openings {
		hostPath, ok := idToPath[op.hostID]
		if !ok {
			return fmt.Errorf("entity %s: unknown-host", op.path)
		}
		host := walls[hostPath]
		L := axisLength(host.start, host.end)
		half := op.width / 2
		from := op.u*L - half
		to := op.u*L + half
		if !(from > 0 && to < L) {
			return fmt.Errorf("entity %s: R4.1.2 strict horizontal fit violated on host length %.0f", op.path, L)
		}
	}
	return nil
}

func checkRingLoops(walls map[string]wallRec) error {
	rings := map[[2]int]bool{}
	for p := range walls {
		w := walls[p]
		rings[[2]int{w.l, w.r}] = true
	}
	for key := range rings {
		perimeter := 0.0
		prevEnd := walls[fmt.Sprintf("wall/%d/%d/bottom/0", key[0], key[1])].start
		firstStart := prevEnd
		for e := 0; e < edgeCount; e++ {
			edgeLen := subsEdgeLength(e)
			edgeStart := walls[fmt.Sprintf("wall/%d/%d/%s/0", key[0], key[1], edgeNames[e])].start
			for k := 0; k < subsPerEdge; k++ {
				p := fmt.Sprintf("wall/%d/%d/%s/%d", key[0], key[1], edgeNames[e], k)
				w, ok := walls[p]
				if !ok {
					return fmt.Errorf("ring l=%d r=%d: missing %s", key[0], key[1], p)
				}
				if w.start != prevEnd {
					return fmt.Errorf("entity %s: chain break at (%v, previous end %v)", p, w.start, prevEnd)
				}
				length := axisLength(w.start, w.end)
				if length <= 0 || w.thickness != wallThicknessMm {
					return fmt.Errorf("entity %s: zero-length-wall or wrong thickness", p)
				}
				perimeter += length
				if k >= 1 {
					// Sub-wall k starts exactly at split k of the edge:
					// distances one third and two thirds along traversal,
					// hence strictly interior (§10.9 splits).
					fromStart := distFrom(edgeStart, w.start)
					third := float64(edgeLen / 3)
					if fromStart != third && fromStart != 2*third {
						return fmt.Errorf("entity %s: split distance %.0f not one/two thirds of edge", p, fromStart)
					}
					if !(fromStart > 0 && fromStart < float64(edgeLen)) {
						return fmt.Errorf("entity %s: split not strictly interior", p)
					}
				}
				prevEnd = w.end
			}
		}
		if perimeter != float64(2*(ringWidthMm+ringHeightMm)) || prevEnd != firstStart {
			return fmt.Errorf("ring l=%d r=%d: not closed (perimeter %.0f)", key[0], key[1], perimeter)
		}
	}
	return nil
}

func checkJoins(d *Document, walls map[string]wallRec) error {
	for i := range d.Objects {
		o := &d.Objects[i]
		if o.Kind != document.KindWallJoin {
			continue
		}
		l, r, edge, k, err := parseJoinPath(o.Path)
		if err != nil {
			return fmt.Errorf("entity %s: %v", o.Path, err)
		}
		var raw struct {
			Members []struct {
				WallID string `json:"wallId"`
				Role   string `json:"role"`
			} `json:"members"`
		}
		if jerr := json.Unmarshal(o.JSON, &raw); jerr != nil {
			return fmt.Errorf("entity %s: %v", o.Path, jerr)
		}
		if len(raw.Members) != 2 {
			return fmt.Errorf("entity %s: corpus joins have exactly two members", o.Path)
		}
		prePath := fmt.Sprintf("wall/%d/%d/%s/%d", l, r, edgeNames[edge], k)
		sucPath := fmt.Sprintf("wall/%d/%d/%s/%d", l, r, edgeNames[edge], k+1)
		pre, ok1 := walls[prePath]
		suc, ok2 := walls[sucPath]
		if !ok1 || !ok2 {
			return fmt.Errorf("entity %s: members do not name adjacent sub-walls", o.Path)
		}
		if raw.Members[0].WallID != pre.id || raw.Members[1].WallID != suc.id {
			return fmt.Errorf("entity %s: member order does not follow traversal order", o.Path)
		}
		if raw.Members[0].Role != "End" || raw.Members[1].Role != "Start" {
			return fmt.Errorf("entity %s: roles must be predecessor-End/successor-Start", o.Path)
		}
		if pre.end != suc.start {
			return fmt.Errorf("entity %s: shared split point differs between members", o.Path)
		}
	}
	return nil
}

func checkRoomSeed(path string, l, r int, seed xyPair) error {
	ox := int64(r%5) * originStepXMm
	oy := int64(r/5) * originStepYMm
	wantX := float64(ox) + float64(ringWidthMm)/2
	wantY := float64(oy) + float64(ringHeightMm)/2
	if seed[0] != wantX || seed[1] != wantY {
		return fmt.Errorf("entity %s: seed [%v,%v] not at ring centre [%v,%v]", path, seed[0], seed[1], wantX, wantY)
	}
	return nil
}

type xyPair [2]float64

type wallRec struct {
	id        string
	l, r, k   int
	edge      int
	start     pointValue
	end       pointValue
	thickness float64
}

type openingRec struct {
	path   string
	hostID string
	u      float64
	width  float64
}

func parseWallPath(p string) (l, r, edge, k int, err error) {
	parts := strings.Split(p, "/")
	if len(parts) != 5 || parts[0] != "wall" {
		return 0, 0, 0, 0, fmt.Errorf("not a wall path")
	}
	l, r, edge, k, err = parseLREK(parts[1], parts[2], parts[3], parts[4])
	return l, r, edge, k, err
}

func parseJoinPath(p string) (l, r, edge, k int, err error) {
	parts := strings.Split(p, "/")
	if len(parts) != 5 || parts[0] != "join" {
		return 0, 0, 0, 0, fmt.Errorf("not a join path")
	}
	return parseLREK(parts[1], parts[2], parts[3], parts[4])
}

func parseLREK(a, b, c, d string) (int, int, int, int, error) {
	l, err := strconv.Atoi(a)
	if err != nil {
		return 0, 0, 0, 0, err
	}
	r, err := strconv.Atoi(b)
	if err != nil {
		return 0, 0, 0, 0, err
	}
	e := -1
	for i, name := range edgeNames {
		if name == c {
			e = i
		}
	}
	if e < 0 {
		return 0, 0, 0, 0, fmt.Errorf("unknown edge %q", c)
	}
	k, err := strconv.Atoi(d)
	if err != nil {
		return 0, 0, 0, 0, err
	}
	return l, r, e, k, nil
}

func parseRoomPath(p string) (l, r int, err error) {
	parts := strings.Split(p, "/")
	if len(parts) != 3 || parts[0] != "room" {
		return 0, 0, fmt.Errorf("not a room path")
	}
	l, err = strconv.Atoi(parts[1])
	if err != nil {
		return 0, 0, err
	}
	r, err = strconv.Atoi(parts[2])
	return l, r, err
}

func subsEdgeLength(e int) int {
	if e%2 == 0 {
		return ringWidthMm
	}
	return ringHeightMm
}

// axisLength is the Manhattan length of an axis-aligned segment; corpus
// edges are horizontal or vertical only, so this is exact.
func axisLength(a, b pointValue) float64 {
	dx := a.x - b.x
	dy := a.y - b.y
	if dx < 0 {
		dx = -dx
	}
	if dy < 0 {
		dy = -dy
	}
	return dx + dy
}

// distFrom measures exact Manhattan offset between two points on one
// axis-aligned edge.
func distFrom(a, b pointValue) float64 {
	return axisLength(a, b)
}

// checkOpeningPlacement verifies the ED-0025 rule exhaustively: an opening
// opening/<l>/<r>/<edge> exists if and only if the global ring instance
// g=(l-1)*10+r is below 39, and its host is exactly that edge's middle
// sub-wall wall/<l>/<r>/<edge>/1; every qualifying instance carries all
// four edge doors.
func checkOpeningPlacement(d *Document) error {
	if d.Definition.Totals.Openings == 0 && d.Definition.Totals.Walls == 0 {
		return nil // empty-v1: zero entities of every non-level kind
	}
	openings := map[string]bool{}
	hostByPath := map[string]string{}
	for i := range d.Objects {
		o := &d.Objects[i]
		switch o.Kind {
		case document.KindOpening:
			var raw struct {
				ID         string `json:"id"`
				HostWallID string `json:"hostWallId"`
			}
			if jerr := json.Unmarshal(o.JSON, &raw); jerr != nil {
				return fmt.Errorf("entity %s: %v", o.Path, jerr)
			}
			l, r, edge, perr := parseOpeningPath(o.Path)
			if perr != nil {
				return fmt.Errorf("entity %s: %v", o.Path, perr)
			}
			if g := (l-1)*ringsPerLevel + r; g >= doorRingsGlobal {
				return fmt.Errorf("entity %s: global instance %d must carry no openings", o.Path, g)
			}
			wantHostPath := fmt.Sprintf("wall/%d/%d/%s/%d", l, r, edgeNames[edge], subsPerEdge/2)
			hostPath, found := idToWallPathOf(d, raw.HostWallID)
			if !found {
				return fmt.Errorf("entity %s: host wall not found", o.Path)
			}
			if hostPath != wantHostPath {
				return fmt.Errorf("entity %s: hosts %s but ED-0025 requires middle sub-wall %s", o.Path, hostPath, wantHostPath)
			}
			openings[o.Path] = true
		case document.KindWall:
			var raw struct {
				ID string `json:"id"`
			}
			if jerr := json.Unmarshal(o.JSON, &raw); jerr == nil {
				hostByPath[o.Path] = raw.ID
			}
		}
	}
	for l := 1; l <= d.Definition.Totals.Levels; l++ {
		for r := 0; r < ringsPerLevel; r++ {
			g := (l-1)*ringsPerLevel + r
			for _, name := range edgeNames {
				path := fmt.Sprintf("opening/%d/%d/%s", l, r, name)
				if g < doorRingsGlobal && !openings[path] {
					return fmt.Errorf("missing required door %s on global instance %d", path, g)
				}
				if g >= doorRingsGlobal && openings[path] {
					return fmt.Errorf("unexpected door %s beyond the ED-0025 global-instance bound", path)
				}
			}
		}
	}
	return nil
}

func idToWallPathOf(d *Document, wallID string) (string, bool) {
	for i := range d.Objects {
		o := &d.Objects[i]
		if o.Kind != document.KindWall {
			continue
		}
		var raw struct {
			ID string `json:"id"`
		}
		if err := json.Unmarshal(o.JSON, &raw); err == nil && raw.ID == wallID {
			return o.Path, true
		}
	}
	return "", false
}

func parseOpeningPath(p string) (l, r, edge int, err error) {
	parts := strings.Split(p, "/")
	if len(parts) != 4 || parts[0] != "opening" {
		return 0, 0, 0, fmt.Errorf("not an opening path")
	}
	l, err = strconv.Atoi(parts[1])
	if err != nil {
		return 0, 0, 0, err
	}
	r, err = strconv.Atoi(parts[2])
	if err != nil {
		return 0, 0, 0, err
	}
	for i, name := range edgeNames {
		if name == parts[3] {
			return l, r, i, nil
		}
	}
	return 0, 0, 0, fmt.Errorf("unknown edge %q", parts[3])
}
