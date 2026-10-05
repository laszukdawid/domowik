package sizecorpus

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"

	"domowik.dev/domowik/internal/document"
)

// Scenario names (ASSET-STRATEGY §10.9 Part A).
const (
	ScenarioEmpty       = "empty-v1"
	ScenarioUnfurnished = "unfurnished-1000-v1"
)

// Geometry constants of §10.9. All coordinates are exact integer
// millimetres; every value below is formula-determined and none is measured.
const (
	ringsPerLevel   = 10
	originStepXMm   = 15000 // ring origin x = 15000 * (r mod 5)
	originStepYMm   = 12000 // ring origin y = 12000 * (r div 5)
	ringWidthMm     = 12000
	ringHeightMm    = 9000
	levelStepMm     = 3000 // elevationMm = 3000 * l0, l0 in 0..3 (G2; ED-0025 context)
	edgeCount       = 4    // bottom, right, top, left
	subsPerEdge     = 3    // splits at one third / two thirds along traversal
	joinsPerEdge    = 2
	doorRingsGlobal = 39 // g in 0..38 (ED-0025)

	wallThicknessMm = 200
	wallHeightMm    = 2500
	floorThickMm    = 200

	doorWidthMm  = 900
	doorHeightMm = 2100
	doorSillMm   = 0
	doorDepthMm  = 120
	doorSwingDeg = 90
	doorU        = "0.5" // literal payload value per §10.9

	// styleVocabulary is the generator-local corpus vocabulary of ED-0025:
	// the §10.9 one-entry document style registry manifests only as this
	// set of resolvable tokens; nothing is serialized on the wire beyond
	// styleRefs values that must resolve against it.
	styleVocabulary = "S0"
)

var edgeNames = [edgeCount]string{"bottom", "right", "top", "left"}

// EntityObject is one generated authoritative entity: its identity path, its
// kind, and its complete JSON object text per FORMAT-SPEC §2.3.
type EntityObject struct {
	Path string
	Kind document.Kind
	JSON []byte
}

// Totals counts generated entities per kind.
type Totals struct {
	Levels   int `json:"levels"`
	Walls    int `json:"walls"`
	Joins    int `json:"joins"`
	Openings int `json:"openings"`
	Rooms    int `json:"rooms"`
}

// TotalEntities sums the per-kind totals.
func (t Totals) TotalEntities() int {
	return t.Levels + t.Walls + t.Joins + t.Openings + t.Rooms
}

// Definition is the canonical scenario definition whose digest binds the
// committed measurement records to the exact construction inputs.
type Definition struct {
	Name        string
	Totals      Totals
	TargetKiB   int
	SeedHex     string
	DoorRule    string
	EDReference string
}

// Definition returns the canonical definition for a scenario name.
func DefinitionFor(scenario string) (Definition, error) {
	switch scenario {
	case ScenarioEmpty:
		return Definition{
			Name:        ScenarioEmpty,
			Totals:      Totals{Levels: 1},
			TargetKiB:   16,
			SeedHex:     SeedEmptyHex,
			DoorRule:    "none: single-level scenario with zero non-level entities",
			EDReference: "ASSET-STRATEGY §10.9",
		}, nil
	case ScenarioUnfurnished:
		return Definition{
			Name:        ScenarioUnfurnished,
			Totals:      Totals{Levels: 4, Walls: 480, Joins: 320, Openings: 156, Rooms: 40},
			TargetKiB:   256,
			SeedHex:     SeedUnfurnishedHex,
			DoorRule:    "global ring instances g=(level-1)*10+r in 0..38 carry one door on each edge-middle sub-wall in bottom,right,top,left order (ED-0025)",
			EDReference: "ASSET-STRATEGY §10.9 as clarified by ED-0025 (provisional pending issue 0021 amendment)",
		}, nil
	default:
		return Definition{}, fmt.Errorf("unknown scenario %q", scenario)
	}
}

// CanonicalJSON renders the definition digest input in exactly the agreed
// schema {scenarioName, seedHex, counts, geometryParams, targetKiB}
// (deterministic Go map key ordering).
func (d Definition) CanonicalJSON() []byte {
	doc := map[string]interface{}{
		"scenarioName": d.Name,
		"seedHex":      d.SeedHex,
		"counts":       d.totalsMap(),
		"targetKiB":    d.TargetKiB,
		"geometryParams": map[string]interface{}{
			"ringsPerLevel":          ringsPerLevel,
			"originStepXMm":          originStepXMm,
			"originStepYMm":          originStepYMm,
			"ringSizeMm":             []int{ringWidthMm, ringHeightMm},
			"elevationStepMm":        levelStepMm,
			"edgesInOrder":           edgeNames[:],
			"splitsAlongEdge":        "one third and two thirds of edge length, measured from edge start along traversal",
			"traversal":              "clockwise: bottom L->R, right B->T, top R->L, left T->B",
			"wallThicknessMm":        wallThicknessMm,
			"wallHeightMm":           wallHeightMm,
			"floorThicknessMm":       floorThickMm,
			"defaultWallHeightMm":    wallHeightMm,
			"defaultCeilingHeightMm": wallHeightMm,
			"doorRule":               d.DoorRule,
			"openingIdentityOrder":   "level, ring, then edge order",
		},
	}
	b, err := json.Marshal(doc)
	if err != nil {
		panic(err) // map of primitives cannot fail
	}
	return b
}

func (d Definition) totalsMap() map[string]int {
	return map[string]int{
		"levels": d.Totals.Levels, "walls": d.Totals.Walls, "joins": d.Totals.Joins,
		"openings": d.Totals.Openings, "rooms": d.Totals.Rooms,
		"totalEntities": d.Totals.TotalEntities(),
	}
}

// DefinitionDigest is sha256 over Definition.CanonicalJSON.
func (d Definition) DefinitionDigest() string {
	sum := sha256.Sum256(d.CanonicalJSON())
	return "sha256:" + hex.EncodeToString(sum[:])
}

// Document is a fully constructed scenario: entity objects plus the
// delivered projection state and the derived operation identities.
type Document struct {
	Definition Definition
	Objects    []EntityObject
	OpIDs      []document.UUID // parallel to Objects: op/create/<entityPath>
	State      document.State
	ActorID    document.UUID
	BatchID    document.UUID
	OpCount    int
}

// Build constructs the scenario deterministically and delivers it through
// the document core (single actor, single creation batch, counters from 1),
// exercising the authoritative schema/framing/replay path. It does not run
// the normative self-check; call SelfCheck before relying on the result.
func Build(scenario string) (*Document, error) {
	def, err := DefinitionFor(scenario)
	if err != nil {
		return nil, err
	}
	seed, err := seedBytes(scenario)
	if err != nil {
		return nil, err
	}
	d := &Document{Definition: def}
	if !def.hasWalls() {
		if err := d.buildEmpty(seed); err != nil {
			return nil, err
		}
	} else if err := d.buildUnfurnished(seed); err != nil {
		return nil, err
	}
	d.ActorID = TagUUID(seed, "actor/create")
	d.BatchID = TagUUID(seed, "batch/create")
	if err := d.deliver(seed); err != nil {
		return nil, err
	}
	return d, nil
}

func (d Definition) hasWalls() bool { return d.Totals.Walls > 0 }

func (d *Document) add(path string, kind document.Kind, format string, args ...interface{}) {
	d.Objects = append(d.Objects, EntityObject{
		Path: path,
		Kind: kind,
		JSON: fmt.Appendf(nil, format, args...),
	})
}

// buildEmpty implements §10.9 `empty-v1`: exactly one level named "Level 1",
// elevationMm 0, floorThicknessMm 200, defaults 2500/2500, background anchor
// absent; zero entities of every other type.
func (d *Document) buildEmpty(seed []byte) error {
	lid := TagUUID(seed, "level/1")
	d.add("level/1", document.KindLevel,
		`{"id":"%s","type":"level","name":"Level 1","elevationMm":0,"floorThicknessMm":%d,"defaultWallHeightMm":%d,"defaultCeilingHeightMm":%d}`,
		lid, floorThickMm, wallHeightMm, wallHeightMm)
	return nil
}

type point struct{ x, y int64 }

// edgeGeometry returns one ring edge's traversal start, end, and the two
// interior split points ordered along traversal (distances L/3 then 2L/3).
func edgeGeometry(ox, oy int64, e int) (start, end point, splits [2]point) {
	switch e {
	case 0: // bottom L->R
		start, end = point{ox, oy}, point{ox + ringWidthMm, oy}
	case 1: // right B->T
		start, end = point{ox + ringWidthMm, oy}, point{ox + ringWidthMm, oy + ringHeightMm}
	case 2: // top R->L
		start, end = point{ox + ringWidthMm, oy + ringHeightMm}, point{ox, oy + ringHeightMm}
	case 3: // left T->B
		start, end = point{ox, oy + ringHeightMm}, point{ox, oy}
	}
	at := func(distNum, distDen int64) point {
		dx := end.x - start.x
		dy := end.y - start.y
		return point{start.x + dx*distNum/distDen, start.y + dy*distNum/distDen}
	}
	splits[0] = at(1, 3)
	splits[1] = at(2, 3)
	return start, end, splits
}

// buildUnfurnished implements §10.9 `unfurnished-1000-v1` under ED-0025:
// four levels (names "Level 1".."Level 4", elevationMm 3000*l0), ten rings
// per level, twelve sub-walls and eight joins per ring instance, doors per
// the ED-0025 global-instance rule, and one room record per ring instance.
func (d *Document) buildUnfurnished(seed []byte) error {
	type wallRef struct {
		id         document.UUID
		start, end point
		length     int64
		edge       int
	}
	middlesByInstance := make([][edgeCount]wallRef, d.Definition.Totals.Levels*ringsPerLevel)

	for l0 := 0; l0 < d.Definition.Totals.Levels; l0++ {
		l := l0 + 1
		lid := TagUUID(seed, "level/"+fmt.Sprint(l))
		d.add("level/"+fmt.Sprint(l), document.KindLevel,
			`{"id":"%s","type":"level","name":"Level %d","elevationMm":%d,"floorThicknessMm":%d,"defaultWallHeightMm":%d,"defaultCeilingHeightMm":%d}`,
			lid, l, levelStepMm*l0, floorThickMm, wallHeightMm, wallHeightMm)

		for r := 0; r < ringsPerLevel; r++ {
			g := l0*ringsPerLevel + r
			ox := int64(r%5) * originStepXMm
			oy := int64(r/5) * originStepYMm

			var subs [edgeCount][subsPerEdge]wallRef
			for e := 0; e < edgeCount; e++ {
				start, end, splits := edgeGeometry(ox, oy, e)
				seq := [subsPerEdge + 1]point{start, splits[0], splits[1], end}
				for k := 0; k < subsPerEdge; k++ {
					path := fmt.Sprintf("wall/%d/%d/%s/%d", l, r, edgeNames[e], k)
					id := TagUUID(seed, path)
					d.add(path, document.KindWall,
						`{"id":"%s","type":"wall","levelId":"%s","startMm":[%d,%d],"endMm":[%d,%d],"thicknessMm":%d,"heightMm":%d,"styleRefs":["%s"]}`,
						id, lid, seq[k].x, seq[k].y, seq[k+1].x, seq[k+1].y, wallThicknessMm, wallHeightMm, styleVocabulary)
					subs[e][k] = wallRef{id: id, start: seq[k], end: seq[k+1], length: segLen(seq[k], seq[k+1]), edge: e}
				}
				middlesByInstance[g][e] = subs[e][1]
			}
			for e := 0; e < edgeCount; e++ {
				for k := 0; k < joinsPerEdge; k++ {
					path := fmt.Sprintf("join/%d/%d/%s/%d", l, r, edgeNames[e], k)
					id := TagUUID(seed, path)
					d.add(path, document.KindWallJoin,
						`{"id":"%s","type":"wallJoin","members":[{"wallId":"%s","role":"End"},{"wallId":"%s","role":"Start"}]}`,
						id, subs[e][k].id, subs[e][k+1].id)
				}
			}
			if g < doorRingsGlobal {
				for e := 0; e < edgeCount; e++ {
					host := middlesByInstance[g][e]
					path := fmt.Sprintf("opening/%d/%d/%s", l, r, edgeNames[e])
					id := TagUUID(seed, path)
					d.add(path, document.KindOpening,
						`{"id":"%s","type":"opening","kind":"door","hostWallId":"%s","u":%s,"widthMm":%d,"heightMm":%d,"sillMm":%d,"depthMm":%d,"side":"Left","hinge":"nearStart","opensToward":"Left","swingDeg":%d,"styleRefs":["%s"]}`,
						id, host.id, doorU, doorWidthMm, doorHeightMm, doorSillMm, doorDepthMm, doorSwingDeg, styleVocabulary)
				}
			}
			roomID := TagUUID(seed, fmt.Sprintf("room/%d/%d", l, r))
			d.add(fmt.Sprintf("room/%d/%d", l, r), document.KindRoomRecord,
				`{"id":"%s","type":"roomRecord","levelId":"%s","seedMm":[%d,%d]}`,
				roomID, lid, ox+ringWidthMm/2, oy+ringHeightMm/2)
		}
	}
	return nil
}

func segLen(a, b point) int64 {
	dx := b.x - a.x
	dy := b.y - a.y
	if dx < 0 {
		dx = -dx
	}
	if dy < 0 {
		dy = -dy
	}
	return dx + dy // axis-aligned corpus edges only
}

// deliver applies all creation objects through the document core.
func (d *Document) deliver(seed []byte) error {
	proj := document.New()
	ops := make([]*document.Operation, 0, len(d.Objects))
	for _, o := range d.Objects {
		opID := TagUUID(seed, "op/create/"+o.Path)
		op, err := document.NewCreateOp(opID, json.RawMessage(o.JSON))
		if err != nil {
			return fmt.Errorf("entity %s: %w", o.Path, err)
		}
		d.OpIDs = append(d.OpIDs, opID)
		ops = append(ops, op)
	}
	batch, err := document.NewBatch(d.BatchID, d.ActorID, 1, nil, ops...)
	if err != nil {
		return fmt.Errorf("creation batch: %w", err)
	}
	if err := proj.Deliver(batch); err != nil {
		return fmt.Errorf("delivery: %w", err)
	}
	d.State = proj.State()
	d.OpCount = len(ops)
	return nil
}
