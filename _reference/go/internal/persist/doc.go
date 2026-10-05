// Package persist implements Domowik container persistence and recovery
// for issue 0029: strict ZIP-family package read/write under the
// normative container profile (FORMAT-SPEC §7), the §9.3 security-limit
// enforcement ordering, snapshot-frontier retention (§5.1 S1–S5, §5.2),
// and atomic replacement exclusively through the §9.7 S0–S5 save state
// machine with F1–F9 fault-injection support.
//
// Security ordering is structural, not advisory: no stdlib archive/zip
// helper participates in the read path. The bounded ZIP reader in this
// package locates structures itself so that limits are enforced BEFORE
// excessive allocation or extraction:
//
//	T1 trailing/ambiguous data  -> reject before any interpretation
//	L6 entry count              -> before central-directory enumeration
//	L7 computed reserve         -> before any proportional allocation
//	profile/name validation     -> before content processing
//	L2 physical input counter   -> on every raw byte pulled from disk
//	declared-vs-observed L3/L4  -> checked against actual growth before
//	                               any byte is yielded to a decoder or
//	                               caller
//	L15/L16 ratio guards        -> evaluated incrementally before
//	                               decompressed output is released,
//	                               saturating arithmetic throughout
//	digest verification         -> completes before semantic decode of
//	                               any structural entry
//
// Platform assumptions (FORMAT-SPEC §9.7): the S4 replace primitive is
// dispatched per platform — POSIX rename(2) on Unix profiles,
// ReplaceFileW on Windows/NTFS — but a power-loss durability CLAIM holds
// only for validated platform profiles (Linux/ext4: same-directory
// rename + file fsync + parent-directory fsync) and only after the fault
// matrix passes there; every other platform is process-crash-only and no
// NTFS durability claim is made by this implementation. External
// modification detection follows BEHAVIOR-SPEC §9.2: load-time content
// baselines plus save-time comparison with explicit confirmation or
// save-as required on mismatch (race boundary declared by that spec).
// Large-entry behavior: PackageView.StreamEntry provides bounded
// sequential extraction with incremental digests (§7.1 C1/C2); the
// inline blob map of a Load is bounded by ratified caps and is the v0.1
// projection surface pending asset-store admission integration (C6).
// Fault injection is deterministic hook-based; mocked failures never
// establish power-loss durability.
//
// All numeric values are named FORMAT-SPEC §9.3 quantities or named
// provisional writer-policy parameters (ED-0021); see limits.go.
package persist
