package sizecorpus

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os/exec"
	"strconv"
	"strings"
)

// This file scopes the committed-artifact regeneration gate to the reference
// environment the committed records themselves name (ED-0031).
//
// The predicate is derived from the evidence, never restated as a second
// pinned constant, so it cannot drift away from the records it guards. Every
// fact a record duplicates is cross-checked, and a missing record, an
// unparsable field, an unknown field, or two records disagreeing is an error
// rather than a degraded comparison: a corrupt reference claim must not be
// able to widen or narrow the gate silently.
//
// The logic lives in a test file because it is dev-only evidence-gate
// machinery; it deliberately adds nothing to the package's product API.

// referenceIdentity is the complete set of live inputs that determine the
// bytes of the environment-bearing evidence records: the captured host
// environment, the zlib runtime behind the reference preview encoder, and
// the exact python3 identity that encoder runs under. Any single difference
// changes at least one committed record, so conformance is all-or-nothing.
type referenceIdentity struct {
	Env            Environment
	ZlibRuntime    string
	PythonIdentity string
}

// envBearingRawRecords are the raw provenance outputs whose entire content is
// a statement about the measurement host.
var envBearingRawRecords = []string{
	"raw/environment.txt",
	"raw/go-version.txt",
	"raw/python3-zlib.txt",
}

type recordClass int

const (
	recordEnvBearing recordClass = iota + 1
	recordHostIndependent
)

// classifyRecord assigns a committed record to the environment-bearing
// allowlist or to the host-independent set. The allowlist is derived from
// Plans() plus the raw provenance outputs so it cannot drift from the
// artifact plan, and an unrecognized record name is an error: a future
// record must be classified deliberately rather than inherit either
// treatment by accident.
func classifyRecord(rel string) (recordClass, error) {
	for _, plan := range Plans() {
		if rel == plan.BudgetsRel || rel == plan.InventorRel {
			return recordEnvBearing, nil
		}
	}
	for _, raw := range envBearingRawRecords {
		if rel == raw {
			return recordEnvBearing, nil
		}
	}
	if rel == "encoding-candidates.json" {
		return recordHostIndependent, nil
	}
	return 0, fmt.Errorf("unclassified evidence record %q: classify it as environment-bearing or host-independent in classifyRecord before it can be gated", rel)
}

// comparisonPlan is the decision of which committed records are compared
// byte-for-byte against regeneration and which are omitted because the live
// host does not match the environment they name.
type comparisonPlan struct {
	Compare     []string
	Omit        []string
	Divergences []string
}

// planRecordComparison is a pure decision over the two identities, so both
// branches are reachable in tests without the reference host.
func planRecordComparison(recorded, live referenceIdentity, rels []string) (comparisonPlan, error) {
	plan := comparisonPlan{Divergences: identityDivergences(recorded, live)}
	for _, rel := range rels {
		class, err := classifyRecord(rel)
		if err != nil {
			return comparisonPlan{}, err
		}
		if class == recordEnvBearing && len(plan.Divergences) > 0 {
			plan.Omit = append(plan.Omit, rel)
			continue
		}
		plan.Compare = append(plan.Compare, rel)
	}
	sortStrings(plan.Compare)
	sortStrings(plan.Omit)
	return plan, nil
}

// identityDivergences names every field in which the live host differs from
// the recorded reference. An empty result is conformance.
func identityDivergences(recorded, live referenceIdentity) []string {
	var out []string
	add := func(field, want, got string) {
		if want != got {
			out = append(out, fmt.Sprintf("%s: reference %q != live %q", field, want, got))
		}
	}
	add("os", recorded.Env.OS, live.Env.OS)
	add("kernel", recorded.Env.Kernel, live.Env.Kernel)
	add("cpuModel", recorded.Env.CPUModel, live.Env.CPUModel)
	add("logicalCpus", strconv.Itoa(recorded.Env.LogicalCPUs), strconv.Itoa(live.Env.LogicalCPUs))
	add("memTotalKiB", strconv.FormatInt(recorded.Env.MemTotalKiB, 10), strconv.FormatInt(live.Env.MemTotalKiB, 10))
	add("filesystem", recorded.Env.Filesystem, live.Env.Filesystem)
	add("goVersion", recorded.Env.GoVersion, live.Env.GoVersion)
	add("goos", recorded.Env.GOOS, live.Env.GOOS)
	add("goarch", recorded.Env.GOARCH, live.Env.GOARCH)
	add("gotoolchain", recorded.Env.GOTOOLCHAIN, live.Env.GOTOOLCHAIN)
	add("zlibRuntimeVersion", recorded.ZlibRuntime, live.ZlibRuntime)
	add("python3Identity", recorded.PythonIdentity, live.PythonIdentity)
	return out
}

// mismatchedRecords reports which of rels differ between the committed tree
// and regeneration.
func mismatchedRecords(rels []string, committed, regenerated map[string][]byte) []string {
	var bad []string
	for _, rel := range rels {
		if !bytes.Equal(committed[rel], regenerated[rel]) {
			bad = append(bad, rel)
		}
	}
	return bad
}

// liveIdentity captures the identity of the running host.
func liveIdentity() (referenceIdentity, error) {
	zlib, err := ZlibRuntimeVersion()
	if err != nil {
		return referenceIdentity{}, err
	}
	python, err := pythonIdentity()
	if err != nil {
		return referenceIdentity{}, err
	}
	return referenceIdentity{Env: Capture(), ZlibRuntime: zlib, PythonIdentity: python}, nil
}

// pythonIdentity reports the exact `python3 -VV` identity of the reference
// preview encoder. Unlike execPython it returns an error when the command
// fails, so a broken interpreter can never be laundered into a string and
// then reported as a mere environment difference.
func pythonIdentity() (string, error) {
	out, err := exec.Command("python3", "-VV").Output()
	if err != nil {
		return "", fmt.Errorf("query python3 identity: %w", err)
	}
	identity := strings.TrimSpace(string(out))
	if identity == "" {
		return "", fmt.Errorf("python3 -VV produced no identity output")
	}
	return identity, nil
}

// claim is one record's statement of one fact, kept with its source so a
// disagreement can name both sides.
type claim struct {
	source string
	value  string
}

// agree collapses duplicate claims about one fact, failing closed when the
// records disagree, when nothing states the fact, or when the stated value is
// empty.
func agree(field string, claims []claim) (string, error) {
	if len(claims) == 0 {
		return "", fmt.Errorf("no committed record states %s", field)
	}
	for _, c := range claims[1:] {
		if c.value != claims[0].value {
			return "", fmt.Errorf("committed records disagree on %s: %s states %q, %s states %q",
				field, claims[0].source, claims[0].value, c.source, c.value)
		}
	}
	if claims[0].value == "" {
		return "", fmt.Errorf("committed record %s states an empty %s", claims[0].source, field)
	}
	return claims[0].value, nil
}

// strictUnmarshal decodes a committed record and rejects unknown fields, so a
// record carrying content this gate does not understand fails closed instead
// of being partially validated. The record must be exactly one JSON document:
// a second decode has to report io.EOF, which is the complete-document check
// (dec.More reports on the current array or object, not on the stream).
func strictUnmarshal(rel string, data []byte, v interface{}) error {
	if err := rejectDuplicateJSONKeys(rel, data); err != nil {
		return err
	}
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.DisallowUnknownFields()
	if err := dec.Decode(v); err != nil {
		return fmt.Errorf("record %s: %w", rel, err)
	}
	var trailing json.RawMessage
	err := dec.Decode(&trailing)
	switch {
	case errors.Is(err, io.EOF):
		return nil
	case err == nil:
		return fmt.Errorf("record %s: a second JSON document follows the first", rel)
	default:
		return fmt.Errorf("record %s: trailing content after the JSON document: %w", rel, err)
	}
}

func between(rel, field, s, prefix, suffix string) (string, error) {
	i := strings.Index(s, prefix)
	if i < 0 {
		return "", fmt.Errorf("record %s: %s does not contain %q", rel, field, prefix)
	}
	rest := s[i+len(prefix):]
	j := strings.Index(rest, suffix)
	if j < 0 {
		return "", fmt.Errorf("record %s: %s does not contain %q after %q", rel, field, suffix, prefix)
	}
	return rest[:j], nil
}

// parseRecordedIdentity derives the reference environment from the committed
// records themselves and cross-checks every duplicated claim.
func parseRecordedIdentity(committed map[string][]byte) (referenceIdentity, error) {
	required := append([]string{}, envBearingRawRecords...)
	for _, plan := range Plans() {
		required = append(required, plan.BudgetsRel, plan.InventorRel)
	}
	for _, rel := range required {
		if _, ok := committed[rel]; !ok {
			return referenceIdentity{}, fmt.Errorf("committed record %s absent: the reference environment cannot be established", rel)
		}
	}

	var (
		osName, kernel, cpuModel, filesystem       []claim
		logicalCPUs, memTotalKiB                   []claim
		goVersion, goos, goarch, gotoolchain, zlib []claim
		python                                     []claim
	)

	for _, plan := range Plans() {
		rel := plan.BudgetsRel
		var rec BudgetRecord
		if err := strictUnmarshal(rel, committed[rel], &rec); err != nil {
			return referenceIdentity{}, err
		}
		env := rec.Environment
		osName = append(osName, claim{rel, env.OS})
		kernel = append(kernel, claim{rel, env.Kernel})
		cpuModel = append(cpuModel, claim{rel, env.CPUModel})
		filesystem = append(filesystem, claim{rel, env.Filesystem})
		logicalCPUs = append(logicalCPUs, claim{rel, strconv.Itoa(env.LogicalCPUs)})
		memTotalKiB = append(memTotalKiB, claim{rel, strconv.FormatInt(env.MemTotalKiB, 10)})
		goVersion = append(goVersion, claim{rel, env.GoVersion})
		goos = append(goos, claim{rel, env.GOOS})
		goarch = append(goarch, claim{rel, env.GOARCH})
		gotoolchain = append(gotoolchain, claim{rel, env.GOTOOLCHAIN})

		// The compression-library sentence states the Go and zlib versions a
		// second time; both are cross-checked rather than trusted.
		lib := rec.Versions.CompressionLibrary
		libGo, err := between(rel, "versions.compressionLibrary", lib, "Go compress/flate (", ")")
		if err != nil {
			return referenceIdentity{}, err
		}
		goVersion = append(goVersion, claim{rel + " versions.compressionLibrary", libGo})
		libZlib, err := between(rel, "versions.compressionLibrary", lib, "python3 stdlib zlib ", " for the reference thumbnail")
		if err != nil {
			return referenceIdentity{}, err
		}
		zlib = append(zlib, claim{rel + " versions.compressionLibrary", libZlib})
	}

	for _, plan := range Plans() {
		rel := plan.InventorRel
		var inv PreviewInventory
		if err := strictUnmarshal(rel, committed[rel], &inv); err != nil {
			return referenceIdentity{}, err
		}
		found := false
		for _, entry := range inv.Entries {
			if entry.Encoder == nil {
				continue
			}
			zlib = append(zlib, claim{rel + " " + entry.Namespace, entry.Encoder.ZlibRuntimeVersion})
			found = true
		}
		if !found {
			return referenceIdentity{}, fmt.Errorf("record %s: no entry states an encoder zlib runtime version", rel)
		}
	}

	rawEnv, err := parseRawEnvironment(committed["raw/environment.txt"])
	if err != nil {
		return referenceIdentity{}, err
	}
	const relEnv = "raw/environment.txt"
	osName = append(osName, claim{relEnv, rawEnv["os"]})
	kernel = append(kernel, claim{relEnv, rawEnv["kernel"]})
	cpuModel = append(cpuModel, claim{relEnv, rawEnv["cpu"]})
	filesystem = append(filesystem, claim{relEnv, rawEnv["filesystem"]})
	logicalCPUs = append(logicalCPUs, claim{relEnv, rawEnv["cpus"]})
	memTotalKiB = append(memTotalKiB, claim{relEnv, rawEnv["memTotalKiB"]})
	goVersion = append(goVersion, claim{relEnv, rawEnv["go"]})
	goos = append(goos, claim{relEnv, rawEnv["goos"]})
	goarch = append(goarch, claim{relEnv, rawEnv["goarch"]})
	gotoolchain = append(gotoolchain, claim{relEnv, rawEnv["GOTOOLCHAIN"]})

	rawGo, err := parseKeyValueRecord("raw/go-version.txt", committed["raw/go-version.txt"],
		[]string{"goVersion", "goos", "goarch"})
	if err != nil {
		return referenceIdentity{}, err
	}
	goVersion = append(goVersion, claim{"raw/go-version.txt", rawGo["goVersion"]})
	goos = append(goos, claim{"raw/go-version.txt", rawGo["goos"]})
	goarch = append(goarch, claim{"raw/go-version.txt", rawGo["goarch"]})

	pythonIdent, rawPy, err := parseRawPython(committed["raw/python3-zlib.txt"])
	if err != nil {
		return referenceIdentity{}, err
	}
	python = append(python, claim{"raw/python3-zlib.txt", pythonIdent})
	zlib = append(zlib, claim{"raw/python3-zlib.txt", rawPy["zlibRuntimeVersion"]})

	// The raw python record also restates the pinned §10.9 thumbnail vector.
	// It is host-independent, so a disagreement is a corrupt record, not an
	// environment difference.
	if got := rawPy["thumbnailSha256"]; got != refPNGSHA256Hex {
		return referenceIdentity{}, fmt.Errorf("raw/python3-zlib.txt: thumbnailSha256 %q != pinned §10.9 vector %q", got, refPNGSHA256Hex)
	}
	if got := rawPy["thumbnailBytes"]; got != strconv.Itoa(refPNGLength) {
		return referenceIdentity{}, fmt.Errorf("raw/python3-zlib.txt: thumbnailBytes %q != pinned §10.9 vector %d", got, refPNGLength)
	}
	if got := rawEnv["generator"]; got != GeneratorVersion {
		return referenceIdentity{}, fmt.Errorf("raw/environment.txt: generator %q != %q", got, GeneratorVersion)
	}

	id := referenceIdentity{}
	fields := []struct {
		name   string
		claims []claim
		assign func(string) error
	}{
		{"os", osName, func(v string) error { id.Env.OS = v; return nil }},
		{"kernel", kernel, func(v string) error { id.Env.Kernel = v; return nil }},
		{"cpuModel", cpuModel, func(v string) error { id.Env.CPUModel = v; return nil }},
		{"filesystem", filesystem, func(v string) error { id.Env.Filesystem = v; return nil }},
		{"goVersion", goVersion, func(v string) error { id.Env.GoVersion = v; return nil }},
		{"goos", goos, func(v string) error { id.Env.GOOS = v; return nil }},
		{"goarch", goarch, func(v string) error { id.Env.GOARCH = v; return nil }},
		{"gotoolchain", gotoolchain, func(v string) error { id.Env.GOTOOLCHAIN = v; return nil }},
		{"zlibRuntimeVersion", zlib, func(v string) error { id.ZlibRuntime = v; return nil }},
		{"python3Identity", python, func(v string) error { id.PythonIdentity = v; return nil }},
		{"logicalCpus", logicalCPUs, func(v string) error {
			n, err := strconv.Atoi(v)
			if err != nil {
				return fmt.Errorf("logicalCpus %q: %w", v, err)
			}
			id.Env.LogicalCPUs = n
			return nil
		}},
		{"memTotalKiB", memTotalKiB, func(v string) error {
			n, err := strconv.ParseInt(v, 10, 64)
			if err != nil {
				return fmt.Errorf("memTotalKiB %q: %w", v, err)
			}
			id.Env.MemTotalKiB = n
			return nil
		}},
	}
	for _, f := range fields {
		v, err := agree(f.name, f.claims)
		if err != nil {
			return referenceIdentity{}, err
		}
		if err := f.assign(v); err != nil {
			return referenceIdentity{}, fmt.Errorf("committed records state an unusable %s: %w", f.name, err)
		}
	}
	return id, nil
}

// fieldSetter returns a setter that accepts only the allowed keys and refuses
// a repeated key, so an unreviewed provenance field cannot be ignored and a
// duplicated one cannot silently overwrite the earlier claim.
func fieldSetter(rel string, fields map[string]string, allowed []string) func(string, string) error {
	permitted := make(map[string]bool, len(allowed))
	for _, k := range allowed {
		permitted[k] = true
	}
	return func(k, v string) error {
		if !permitted[k] {
			return fmt.Errorf("record %s: unknown field %q", rel, k)
		}
		if _, dup := fields[k]; dup {
			return fmt.Errorf("record %s: duplicate field %q", rel, k)
		}
		fields[k] = v
		return nil
	}
}

// requireExactFields rejects a record that does not state every allowed field.
func requireExactFields(rel string, fields map[string]string, allowed []string) error {
	for _, k := range allowed {
		if _, ok := fields[k]; !ok {
			return fmt.Errorf("record %s: field %q absent", rel, k)
		}
	}
	return nil
}

// rawEnvironmentFields is the exact field set of raw/environment.txt.
var rawEnvironmentFields = []string{
	"generator", "os", "kernel", "cpu", "cpus", "memTotalKiB", "filesystem",
	"go", "goos", "goarch", "GOTOOLCHAIN",
}

// parseRawEnvironment reads raw/environment.txt back into its stated fields,
// accepting exactly the documented field set and no repetition.
func parseRawEnvironment(data []byte) (map[string]string, error) {
	const rel = "raw/environment.txt"
	const header = "environment captured by generator "
	lines := strings.Split(strings.TrimRight(string(data), "\n"), "\n")
	if len(lines) == 0 || !strings.HasPrefix(lines[0], header) {
		return nil, fmt.Errorf("record %s: missing %q header", rel, strings.TrimSpace(header))
	}
	fields := map[string]string{}
	set := fieldSetter(rel, fields, rawEnvironmentFields)
	if err := set("generator", strings.TrimPrefix(lines[0], header)); err != nil {
		return nil, err
	}
	for _, line := range lines[1:] {
		if line == "" {
			continue
		}
		// The toolchain line packs several pairs onto one line; every other
		// line is a single key=value whose value may contain spaces.
		if strings.HasPrefix(line, "go=") {
			for _, pair := range strings.Fields(line) {
				k, v, ok := strings.Cut(pair, "=")
				if !ok {
					return nil, fmt.Errorf("record %s: unparsable field %q", rel, pair)
				}
				if err := set(k, v); err != nil {
					return nil, err
				}
			}
			continue
		}
		k, v, ok := strings.Cut(line, "=")
		if !ok {
			return nil, fmt.Errorf("record %s: unparsable line %q", rel, line)
		}
		if err := set(k, v); err != nil {
			return nil, err
		}
	}
	if err := requireExactFields(rel, fields, rawEnvironmentFields); err != nil {
		return nil, err
	}
	return fields, nil
}

// parseKeyValueRecord reads a flat key=value provenance record, accepting
// exactly the allowed field set with no unknown and no repeated key.
func parseKeyValueRecord(rel string, data []byte, allowed []string) (map[string]string, error) {
	fields := map[string]string{}
	set := fieldSetter(rel, fields, allowed)
	for _, line := range strings.Split(strings.TrimRight(string(data), "\n"), "\n") {
		if line == "" {
			continue
		}
		k, v, ok := strings.Cut(line, "=")
		if !ok {
			return nil, fmt.Errorf("record %s: unparsable line %q", rel, line)
		}
		if err := set(k, v); err != nil {
			return nil, err
		}
	}
	if err := requireExactFields(rel, fields, allowed); err != nil {
		return nil, err
	}
	return fields, nil
}

// parseRawPython splits raw/python3-zlib.txt into the python identity banner
// and its trailing key=value block. The banner is everything before the first
// key line, so a multi-line interpreter banner is read whole.
func parseRawPython(data []byte) (string, map[string]string, error) {
	const rel = "raw/python3-zlib.txt"
	lines := strings.Split(strings.TrimRight(string(data), "\n"), "\n")
	split := -1
	for i, line := range lines {
		if strings.HasPrefix(line, "zlibRuntimeVersion=") {
			split = i
			break
		}
	}
	if split < 0 {
		return "", nil, fmt.Errorf("record %s: no zlibRuntimeVersion line", rel)
	}
	// Everything before the first key line is the interpreter banner. A
	// key=value line hidden there would otherwise be absorbed into the
	// identity string and reported as an environment difference instead of
	// as the malformed record it is.
	for _, line := range lines[:split] {
		if looksLikeFieldLine(line) {
			return "", nil, fmt.Errorf("record %s: unknown field line %q precedes zlibRuntimeVersion", rel, line)
		}
	}
	identity := strings.TrimSpace(strings.Join(lines[:split], "\n"))
	if identity == "" {
		return "", nil, fmt.Errorf("record %s: no python identity precedes zlibRuntimeVersion", rel)
	}
	fields, err := parseKeyValueRecord(rel, []byte(strings.Join(lines[split:], "\n")),
		[]string{"zlibRuntimeVersion", "thumbnailSha256", "thumbnailBytes"})
	if err != nil {
		return "", nil, err
	}
	return identity, fields, nil
}

// looksLikeFieldLine reports whether a line has the key=value shape of a
// provenance field, so such a line cannot hide inside free-form text.
func looksLikeFieldLine(line string) bool {
	key, _, ok := strings.Cut(line, "=")
	if !ok || key == "" {
		return false
	}
	for i, r := range key {
		switch {
		case r >= 'a' && r <= 'z', r >= 'A' && r <= 'Z':
		case r == '_':
		case i > 0 && r >= '0' && r <= '9':
		default:
			return false
		}
	}
	return true
}

// rejectDuplicateJSONKeys refuses a record that states the same object key
// twice at any depth. encoding/json silently keeps the last value even with
// DisallowUnknownFields, so a duplicated claim would otherwise be laundered
// into one accepted value - the JSON counterpart of the duplicate provenance
// field rejected by fieldSetter.
func rejectDuplicateJSONKeys(rel string, data []byte) error {
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.UseNumber()
	return walkJSONValue(rel, dec, "")
}

func walkJSONValue(rel string, dec *json.Decoder, path string) error {
	tok, err := dec.Token()
	if err != nil {
		// Malformed input is reported by the strict decode that follows;
		// this walk only judges duplicate keys.
		return nil
	}
	delim, ok := tok.(json.Delim)
	if !ok {
		return nil
	}
	switch delim {
	case '{':
		seen := map[string]bool{}
		// dec.More is correct here: it reports on the object being walked,
		// which is what it is documented to do.
		for dec.More() {
			keyTok, err := dec.Token()
			if err != nil {
				return nil
			}
			key, ok := keyTok.(string)
			if !ok {
				return fmt.Errorf("record %s: non-string object key at %q", rel, path)
			}
			if seen[key] {
				return fmt.Errorf("record %s: duplicate key %q at %q", rel, key, path)
			}
			seen[key] = true
			if err := walkJSONValue(rel, dec, path+"/"+key); err != nil {
				return err
			}
		}
	case '[':
		for i := 0; dec.More(); i++ {
			if err := walkJSONValue(rel, dec, fmt.Sprintf("%s[%d]", path, i)); err != nil {
				return err
			}
		}
	}
	// Consume the closing delimiter.
	if _, err := dec.Token(); err != nil {
		return nil
	}
	return nil
}
