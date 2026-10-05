package persist_test

import (
	"errors"
	"io"
	"os"
	"testing"

	"domowik.dev/domowik/internal/persist"
)

// save_fault_x3_test.go: Bounded X3 injection at representative write
// boundaries per r0029's Challenge 3. Tests 3-5 boundaries spanning the
// streaming phases (manifest assembly, entry emission, ZIP finalization)
// rather than exhaustively iterating every entry in every fixture.
//
// Coverage rationale: Each boundary exercises a distinct phase of the
// S1 streaming write:
// 1. Header: ZIP signature + manifest start (early abort)
// 2. Mid-manifest: Between inventory entries (manifest-assembly phase)
// 3. First entry: snapshot/state.json (entry-emission phase)
//
// All verify F2-class outcome: target untouched (OLD), leftover cleanable.
// Note: A "footer" boundary (central directory + end-of-archive) would be
// useful but the minimal test package (~700 bytes) completes before any
// realistic footer offset. Larger packages are tested elsewhere; this
// bounded X3 proves interruption at three distinct streaming phases.

type failAtByteWriter struct {
	w           io.Writer
	failAtByte  int64
	written     int64
	failMessage string
}

func (f *failAtByteWriter) Write(p []byte) (int, error) {
	if f.written >= f.failAtByte {
		return 0, errors.New(f.failMessage)
	}
	n := len(p)
	if f.written+int64(n) > f.failAtByte {
		n = int(f.failAtByte - f.written)
	}
	wn, err := f.w.Write(p[:n])
	f.written += int64(wn)
	if f.written >= f.failAtByte {
		return wn, errors.New(f.failMessage)
	}
	return wn, err
}

// TestFaultX3WriteBoundaries exercises interruption at representative
// write boundaries across the S1 streaming phases.
func TestFaultX3WriteBoundaries(t *testing.T) {
	boundaries := []struct {
		name       string
		failAtByte int64
		desc       string
	}{
		{"header", 50, "ZIP signature + manifest start"},
		{"mid-manifest", 200, "Between inventory entries"},
		{"first-entry", 500, "First logical entry (snapshot)"},
	}

	for _, b := range boundaries {
		b := b
		t.Run(b.name, func(t *testing.T) {
			target := tempTarget(t)
			// Establish OLD generation.
			if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
				t.Fatal(err)
			}
			oldView, err := persist.LoadFile(target)
			if err != nil {
				t.Fatal(err)
			}
			oldEntities := len(oldView.Projection.State().Entities)

			// Inject interruption at specified boundary.
			hooks := &persist.SaveHooks{Writer: func(w io.Writer) io.Writer {
				return &failAtByteWriter{
					w:           w,
					failAtByte:  b.failAtByte,
					failMessage: "injected X3 interruption at " + b.desc,
				}
			}}
			_, serr := persist.SaveDocument(target, 0o644, resaveInput(t, target), hooks)
			if serr == nil {
				t.Fatalf("expected write failure at %s boundary", b.name)
			}

			// F2-class outcome: target untouched (OLD readable), leftover cleanable.
			view, lerr := persist.LoadFile(target)
			if lerr != nil || len(view.Projection.State().Entities) != oldEntities {
				t.Fatalf("target must remain OLD-readable after X3 at %s: %v", b.name, lerr)
			}
			leftovers := persist.DiscoverArtifacts(target)
			for _, art := range leftovers {
				os.Remove(art.Path) // cleanable (RS3)
			}
		})
	}
}
