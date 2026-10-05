// Package assets implements the issue 0026 asset store: streaming
// content-addressed import into a local shared cache (ASSET-STRATEGY
// §10.6 lookup order, cache side), v0.1 descriptor documents (§10.2),
// pin-based cache garbage collection that is fail-closed under metadata
// anomalies (§10.7 G1–G4), and the referenced-blob universe U(P) (§10.5)
// used by portable packing.
//
// Identity is sha256:<64 lowercase hex> over the logical blob byte
// sequence B (§10.1); admission requires streaming digest verification
// and enforcement of the ratified per-blob cap L5 before any cache
// placement. Insertion writes the blob before any descriptor or pin, so
// an interrupted insertion leaves a detectable, safely collectible
// orphan rather than a partial asset (G1 direction: over-retention,
// never premature deletion).
//
// Scope boundary: this package covers the CLI-facing store surface. Pins
// are durable roots; leases (advisory, time-bound per §10.7) have no
// acquirer in v0.1 and are deliberately not implemented. Cross-process
// mutual exclusion of admission and collection is a single-writer
// assumption guarded by an in-process mutex; multi-process concurrent
// stores are outside the v0.1 CLI contract.
package assets
