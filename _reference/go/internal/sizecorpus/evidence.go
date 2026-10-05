package sizecorpus

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"domowik.dev/domowik/internal/persist"
)

// EvidenceRoot is the repository-relative directory holding the committed
// Part A artifacts.
const EvidenceRoot = "docs/evidence/0035-size-corpus"

// ScenarioPlan pairs a scenario name with its committed artifact names.
type ScenarioPlan struct {
	Name        string
	PackageRel  string // relative to EvidenceRoot
	BudgetsRel  string
	InventorRel string
}

// Plans returns the deterministic artifact plan of both Part A scenarios.
func Plans() []ScenarioPlan {
	return []ScenarioPlan{
		{Name: ScenarioEmpty, PackageRel: "packages/empty-v1.domo",
			BudgetsRel: "budget-empty-v1.json", InventorRel: "preview-inventory-empty-v1.json"},
		{Name: ScenarioUnfurnished, PackageRel: "packages/unfurnished-1000-v1.domo",
			BudgetsRel: "budget-unfurnished-1000-v1.json", InventorRel: "preview-inventory-unfurnished-1000-v1.json"},
	}
}

// Generated holds every produced artifact in memory so tests can verify
// byte-stability before anything is written into the tree.
type Generated struct {
	Packages   map[string][]byte // package rel path -> bytes
	Records    map[string][]byte // record rel path -> bytes
	Thumbnails map[string][]byte // scenario -> reference PNG
}

// GenerateAll runs the full Part A pipeline: normative self-check per
// scenario (mandatory before reliance), thin-package measurement, and record
// assembly. It performs no I/O against the repository.
func GenerateAll() (*Generated, error) {
	if err := SelfCheckRNGVector(); err != nil {
		return nil, err
	}
	png, err := ReferenceThumbnail()
	if err != nil {
		return nil, fmt.Errorf("reference thumbnail unavailable: %w", err)
	}
	zlibVer, err := ZlibRuntimeVersion()
	if err != nil {
		return nil, err
	}
	env := Capture()
	gen := &Generated{
		Packages:   map[string][]byte{},
		Records:    map[string][]byte{},
		Thumbnails: map[string][]byte{ScenarioEmpty: png, ScenarioUnfurnished: png},
	}
	for _, plan := range Plans() {
		def, err := DefinitionFor(plan.Name)
		if err != nil {
			return nil, err
		}
		doc, err := Build(plan.Name)
		if err != nil {
			return nil, fmt.Errorf("scenario %s build: %w", plan.Name, err)
		}
		if err := SelfCheck(doc); err != nil {
			return nil, fmt.Errorf("scenario %s self-check failed: %w", plan.Name, err)
		}
		m, err := MeasureThinPackage(doc, png)
		if err != nil {
			return nil, fmt.Errorf("scenario %s measurement: %w", plan.Name, err)
		}
		gen.Packages[plan.PackageRel] = mustPackageBytes(doc, png)
		budget := BuildBudgetRecord(def, m, env, plan.PackageRel, zlibVer)
		gen.Records[plan.BudgetsRel], err = MarshalRecordJSON(budget)
		if err != nil {
			return nil, err
		}
		inv := BuildPreviewInventory(def, png, zlibVer)
		gen.Records[plan.InventorRel], err = MarshalRecordJSON(inv)
		if err != nil {
			return nil, err
		}
	}
	gen.Records["encoding-candidates.json"], err = MarshalRecordJSON(EncodingCandidatesRecord())
	if err != nil {
		return nil, err
	}
	rawEnv, rawPy, rawGo := rawOutputs(env, png, zlibVer)
	gen.Records["raw/environment.txt"] = rawEnv
	gen.Records["raw/python3-zlib.txt"] = rawPy
	gen.Records["raw/go-version.txt"] = rawGo
	return gen, nil
}

// mustPackageBytes re-runs the writer to capture the exact package bytes
// (MeasureThinPackage exposes only the decomposition).
func mustPackageBytes(d *Document, png []byte) []byte {
	pkg, perr := persist.BuildPackage(WriteInputFor(d, png))
	if perr != nil {
		panic(perr)
	}
	return pkg
}

func rawOutputs(env Environment, png []byte, zlibVer string) (envTxt, pyTxt, goTxt []byte) {
	var b strings.Builder
	fmt.Fprintf(&b, "environment captured by generator %s\n", GeneratorVersion)
	fmt.Fprintf(&b, "os=%s\nkernel=%s\ncpu=%s\ncpus=%d\nmemTotalKiB=%d\nfilesystem=%s\n", env.OS, env.Kernel, env.CPUModel, env.LogicalCPUs, env.MemTotalKiB, env.Filesystem)
	fmt.Fprintf(&b, "go=%s goos=%s goarch=%s GOTOOLCHAIN=%s\n", env.GoVersion, env.GOOS, env.GOARCH, env.GOTOOLCHAIN)
	envTxt = []byte(b.String())

	pyOut := execPython("-VV")
	var p strings.Builder
	fmt.Fprintf(&p, "%szlibRuntimeVersion=%s\nthumbnailSha256=%s\nthumbnailBytes=%d\n",
		pyOut, zlibVer, hexString(sha256Of(png)), len(png))
	pyTxt = []byte(p.String())

	goTxt = []byte(fmt.Sprintf("goVersion=%s\ngoos=%s\ngoarch=%s\n", runtime.Version(), runtime.GOOS, runtime.GOARCH))
	return envTxt, pyTxt, goTxt
}

// WriteTree materializes the generated artifacts under root, creating
// directories as needed. Callers own gating (the gated test is the only
// in-tree caller).
func WriteTree(root string, gen *Generated) error {
	for rel, data := range gen.Packages {
		if err := writeFile(filepath.Join(root, rel), data); err != nil {
			return err
		}
	}
	names := make([]string, 0, len(gen.Records))
	for rel := range gen.Records {
		names = append(names, rel)
	}
	sortStrings(names)
	for _, rel := range names {
		if err := writeFile(filepath.Join(root, rel), gen.Records[rel]); err != nil {
			return err
		}
	}
	return nil
}

func writeFile(path string, data []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	return os.WriteFile(path, data, 0o644)
}

func sortStrings(s []string) {
	for i := 1; i < len(s); i++ {
		for j := i; j > 0 && s[j] < s[j-1]; j-- {
			s[j], s[j-1] = s[j-1], s[j]
		}
	}
}
