// Package sizecorpus implements issue 0035 Part A: deterministic
// generation, normative self-checking, and thin-package measurement of the
// ASSET-STRATEGY §10.9 size-corpus scenarios `empty-v1` and
// `unfurnished-1000-v1`.
//
// Normative basis and ownership:
//
//   - ASSET-STRATEGY §10.9 defines the scenarios, geometry formulas,
//     authoritative payload values, seeds/RNG primitives, the generator
//     self-check obligation, the reference thumbnail encoder vector, the
//     reproducibility tiers, and the provisional budgets/targets.
//   - ED-0025 (EXECUTIVE-DECISIONS.md) resolves two §10.9 ambiguities and is
//     PROVISIONAL pending an issue 0021 amendment: ring instances are numbered
//     globally g ∈ 0..39 (level-major, ring-major); each g ∈ 0..38 carries one
//     door on the middle sub-wall of each edge (bottom, right, top, left), for
//     the normative 156 openings; and the one-entry style registry is a
//     generator-local vocabulary containing only "S0", never a wire object.
//   - FORMAT-SPEC §2.1 E4 fixes the derived-tag UUID layout (version nibble
//     UUIDv8, RFC 9562 variant bits); this generator owns the hash/input
//     encoding and accounts for the six overwritten bits when pinning values.
//
// Determinism contract (Tier W of §10.9): identical generator version +
// toolchain + reference-PNG encoder environment reproduce byte-identical
// packages. The reference PNG depends on a zlib-1.3.2-equivalent DEFLATE and
// is hard-checked against the pinned §10.9 vector; environments whose output
// differs fail loudly (no silent acceptance). No claim is made across writers,
// containers, or DEFLATE implementations. History framing is Part B and stays
// out: Part A packages carry a complete snapshot and zero retained operation
// entries, and history cost is never folded into headline budgets.
//
// Correctness/benchmark separation (Performance Policy): everything in this
// package is deterministic. No test asserts timing; measured sizes live only
// in committed evidence records under docs/evidence/0035-size-corpus/.
package sizecorpus
