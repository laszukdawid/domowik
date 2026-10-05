# Issue 0005 — Rulings on zero-member detection and unreachable diagnostics

**Status:** FINAL  
**Worker:** w0005  
**Reviewer:** r0005  
**Date:** 2026-08-31

These rulings amend FORMAT-SPEC specification text only. No production code, taxonomy tokens, or corpus material is changed.

---

## Task 1: Zero-valued frontier member detection stage

### Ruling

**The v0.1 format detects zero-valued frontier members at schema validation (`StageSchema`/`StageFraming`), emitting `invalid-field-syntax`.**

### Reasoning

§9.5 governs diagnostic precedence and speaks in STAGES, not schema languages: "the outermost failing stage wins and diagnostics name that stage." F1's conditional clause "when JSON schema constraints can detect the zero member" names the SCHEMA-VALIDATION STAGE of the reader's pipeline, not a requirement for JSON Schema documents.

Domowik defines `StageSchema` at `internal/persist/errors.go:76` and already emits `fail(CatInvalidFieldSyntax, StageSchema, ...)` at `inspect.go:323, :425, :445, :465` for field-level value rejections. A zero-valued frontier counter is the same kind of check.

Therefore under §9.5's outermost-stage rule, the PRIMARY diagnostic is **`invalid-field-syntax` at schema validation**. The `malformed-framing` fallback applies only where a reader genuinely cannot detect zero before frontier construction.

### FORMAT-SPEC amendment (§4.4 F1 Zero prohibition, lines 426-442)

**Replace** lines 426-433 (the conditional clause):

```
The PRIMARY rejection diagnostic is `invalid-field-syntax` (precedence: 
schema validation, §9.5 stage ordering) when JSON schema constraints can 
detect the zero member; `malformed-framing` (precedence: frontier 
construction) when schema validation does not constrain counter minimums. 
Which stage detects zero is a §7.6 schema-design question owned by 
issue 0005; the semantic prohibition and the precedence rule (§9.5 
outermost-failing-stage-wins) bind regardless of that choice.
```

**With** the ruled stage and reasoning:

```
The PRIMARY rejection diagnostic is `invalid-field-syntax` at the schema-
validation stage (§9.5 outermost-stage precedence). "Schema validation" 
denotes the pipeline stage where field-level value constraints are 
enforced—`StageSchema` for snapshots and `StageFraming` for batches in 
v0.1's implementation (`internal/persist/errors.go:76`, already used for 
field syntax at `inspect.go:323/:425/:445/:465`)—not a requirement for 
JSON Schema documents. A reader that cannot detect zero at this stage 
emits `malformed-framing` at frontier construction as a fallback.
```

### Handoff to issue 0028

Zero-member detection is ruled. The diagnostic token is **`invalid-field-syntax`** at `StageSchema` (snapshots) or `StageFraming` (batches). Issue 0028 may now write the hostile fixture per §9.6:
- Package with frontier containing `{"<valid-uuid>": 0}`
- Expected outcome: reject with `invalid-field-syntax`

---

## Task 2a: `ambiguous-manifest` unreachability ruling

### Ruling

**`ambiguous-manifest` is UNREACHABLE under v0.1's closed-namespace profile. The token remains in §9.4 taxonomy and production code unchanged. Removal requires §9.4 and issue 0028 owner decision with compatibility assessment.**

### Unreachability finding

The v0.1 closed namespace (§7.3 lines 1117-1131) defines exactly ONE manifest name: `domowik.json`. Two scenarios that might trigger `ambiguous-manifest`:

1. **Two entries both named `domowik.json`:** §7.3 line 1113 states "duplicate logical names always reject (`duplicate-logical-name`)". This rejection occurs at name validation (outermost stage per §9.5), BEFORE §7.4's manifest parsing where `ambiguous-manifest` is defined.

2. **Two manifests with different names:** IMPOSSIBLE. The closed namespace has exactly one manifest entry name. §7.3 line 1128: "Any other entry name, or a name malformed for its namespace, rejects (`invalid-entry-name`)."

**Precedence chain (per §9.5):** Name validation (§7.3) → manifest parsing (§7.4). The outermost failing stage wins, so `duplicate-logical-name` always fires before `ambiguous-manifest` can be reached.

**Conclusion:** `ambiguous-manifest` cannot fire under v0.1 rules. This is the ED-0041 shape: a rule that cannot fire is indistinguishable from a rule that is not there.

### FORMAT-SPEC clarification (§7.4 lines 1135-1137)

**Current:**
```
The manifest `domowik.json` is a JSON object. Exactly one manifest exists;
zero manifests reject `missing-manifest`, multiple independent of naming
reject `ambiguous-manifest` (§9.4). Fields:
```

**Amended to clarify precedence without removing the token:**
```
The manifest `domowik.json` is a JSON object. Exactly one manifest exists 
per the §7.3 closed namespace; zero manifests reject `missing-manifest`. 
The `ambiguous-manifest` token (§9.4) is unreachable under v0.1: multiple 
entries named `domowik.json` reject as `duplicate-logical-name` at name 
validation (§7.3, outermost per §9.5), and multiple manifests with 
different names cannot exist under the closed namespace. Fields:
```

### NO changes to

- **§9.4 line 1674:** `ambiguous-manifest` remains in the taxonomy table
- **`internal/persist/errors.go:22`:** `CatAmbiguousManifest` constant unchanged
- **`internal/persist/reader.go:229`:** Emission site unchanged
- **`testdata/corpus/hostile/container/ambiguous-manifest.domo`:** Fixture unchanged
- **`internal/corpusgen/generate.go:305`:** Generator unchanged
- **`testdata/corpus/EXPECTATIONS.md:1316`:** Gap record unchanged

### Routed question

**To §9.4 and issue 0028 owners:** Should `ambiguous-manifest` be removed from the v0.1 taxonomy given its unreachability under the closed-namespace profile? Removal would affect:
- FORMAT-SPEC §9.4 taxonomy table
- Production code (`CatAmbiguousManifest`, emission site)
- Corpus fixture and generator
- EXPECTATIONS gap record

**Compatibility consideration:** The token is published in §9.4. Removal changes what a conforming reader may emit. If removal proceeds, document as a format evolution and update all affected artifacts atomically.

---

## Task 2b: L29 repeated-physical-blob tolerance unreachability ruling

### Ruling

**L29 duplicate-physical-blob tolerance is UNREACHABLE under v0.1's injective blob mapping. The limit row remains in §9.3 unchanged. Removal requires owner decision with compatibility assessment.**

### Unreachability finding

§7.3 line 1125 and §9.4 line 1684 state that blob paths follow ASSET-STRATEGY §10.3's content-addressed mapping: `blob/sha256/<2-hex>/<62-hex>`, which is **injective** (§7.4 line 1206).

Two scenarios:

1. **Two physical entries with identical bytes:** They have identical SHA-256 digests → identical logical paths. §7.3 line 1113: "duplicate logical names always reject (`duplicate-logical-name`)". This rejection occurs at name validation.

2. **Two physical entries with different bytes:** They have different digests → different logical paths (injective mapping). No duplicate exists.

**Precedence chain (per §9.5):** Name validation (§7.3) → enumeration (where L29 applies). L29's tolerance "readers tolerate repeated physical occurrences of identical blobs up to L6" cannot fire: identical blobs have identical paths (scenario 1), triggering `duplicate-logical-name` rejection at the outermost stage before L29 applies.

**Conclusion:** L29 is UNREACHABLE under v0.1. Identical blobs cannot have different logical names (injective mapping), and duplicate logical names are rejected before the tolerance stage.

### FORMAT-SPEC clarification (§7.3 lines 1113-1115)

**Current:**
```
duplicate logical names always reject (`duplicate-logical-name`) while
repeated physical occurrences of identical blobs remain tolerated up to L29
without weakening that rejection.
```

**Amended to record unreachability:**
```
duplicate logical names always reject (`duplicate-logical-name`). L29's 
tolerance for "repeated physical occurrences of identical blobs" is 
unreachable under v0.1: because the blob mapping is injective (§10.3), 
identical blobs have identical logical paths, so `duplicate-logical-name` 
rejection covers all duplicate-blob cases at name validation (outermost per 
§9.5) before L29's enumeration stage.
```

### NO changes to

- **§9.3 row L29 (line 1633):** Limit row unchanged
- **§9.4 line 1673:** Tolerance clause in taxonomy table unchanged

### Routed question

**To §9.3 and §9.4 owners:** Should L29 be removed from v0.1 given its unreachability under the injective blob-mapping profile? The tolerance serves no function when identical blobs necessarily have identical paths and are rejected at name validation. If removal proceeds, update §9.4 line 1673 accordingly.

---

## Summary

**Specification changes (FORMAT-SPEC only):**
1. §4.4 F1: Zero-member detection ruled at schema-validation stage with reasoning
2. §7.4: Clarify `ambiguous-manifest` unreachability under closed namespace (token remains in §9.4)
3. §7.3: Clarify L29 unreachability under injective mapping (row remains in §9.3)

**No changes to:**
- Production code (`internal/persist/*`, taxonomy constants, emission sites)
- Corpus fixtures (`testdata/corpus/*`)
- Limit tables (§9.3 rows)
- Taxonomy tokens (§9.4 error classes)

**Routed questions:**
- `ambiguous-manifest` removal → §9.4 and issue 0028 owners
- L29 removal → §9.3 and §9.4 owners

**Handoff to issue 0028:**
Zero-member diagnostic is `invalid-field-syntax`. Hostile fixture can be written.
