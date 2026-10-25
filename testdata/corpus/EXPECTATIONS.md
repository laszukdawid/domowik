# Corpus expectation manifest (issue 0028)

Machine-readable index of every fixture. Records live in the fenced JSON
arrays below and are validated by the Rust corpus gates in `domowik-devtools`
(`cargo test --locked -p domowik-devtools --test corpus_gates`), which enforce: unique ids, closed
vocabularies, clause citations, artifact/recipe exclusivity, artifact
existence and the 256 KiB inert cap, recipe bound expressions, and full
two-way coverage between this file and the committed fixture tree.

Field semantics, outcome vocabulary, numeric policy (ED-0019), provenance,
and safety rules: [README.md](README.md). Abbreviations: FS = FORMAT-SPEC.md,
AS = ASSET-STRATEGY.md, BE = BEHAVIOR-SPEC.md. All hostile fixtures are inert;
all records with `"implementationDeferred": true` are declarative vectors that
confer no executable conformance claim until issue 0029 enforces the contract.

## Telling executed coverage from residual blockers

Read this file, not the tests, to know what is proven. Three states exist and
they are distinct:

1. **Executed evidence.** A record with no blocking marker. Its `assert` block
   is asserted for real by the harness on every run.
2. **A blocked assertion.** A record carrying `executionBlocked`. The record
   still executes — it is not skipped — but the named `assertions` are not yet
   provable, and the marker states why, which normative clauses are in
   tension, the exact code citations, and the route that owns the resolution.
   The oracle value is never weakened to match the implementation. The harness
   pins the divergence and FAILS if the implementation becomes conformant
   while the marker remains, so removal is always a conscious act.
3. **A blocked obligation.** A record with `"outcome":
   "blocked-open-question"`, which cannot be materialized at all under the
   normative v0.1 profile.

The `residualBlockers` array in the coverage-audit fence lists every record in
state 2 or 3 with its route. The Rust corpus gates enforce the honest-coverage
contract mechanically: the marker's named assertions must exist in the
record's own `assert` block; `residualBlockers` must match the blocked record
set exactly in both directions; and no `categoryAudit` or `rowAudit` entry may
claim `"status": "covered"` on the strength of blocked records alone. A
taxonomy token covered only by a record that does not assert it is not
covered — the substitution issue 0018's release audit condemned.

## Golden

```json
[
 {
  "id": "golden/entities/level-minimal",
  "class": "golden",
  "artifact": "golden/entities/level-minimal.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.3 Tier A level"
  ],
  "notes": "required fields only",
  "sha256": "e3044c867a50b3885adbc51f3e7935a3f6c23183da69906ac5284ea66e7d19b2",
  "assert": {
   "outcome": "success",
   "entityKind": "level",
   "requiredFieldsPresent": true
  }
 },
 {
  "id": "golden/entities/level-background-images",
  "class": "golden",
  "artifact": "golden/entities/level-background-images.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.3 level backgroundImages",
   "FS §2.2 C3"
  ],
  "notes": "unordered set with anchor tuple; C3 sort tuple over (digest, stackOrdinal, translationMm, scaleUniform, rotationDeg)",
  "sha256": "f84aa73f4f327871292f4495918d455488f2705a4c2eca34da75cb9158401f4a",
  "assert": {
   "outcome": "success",
   "entityKind": "level",
   "optionalSetsPresent": true
  }
 },
 {
  "id": "golden/entities/wall-minimal",
  "class": "golden",
  "artifact": "golden/entities/wall-minimal.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.3 Tier A wall"
  ],
  "notes": "",
  "sha256": "90131b9f20f8384010876e0e09802a0ebc347ede1928fce188b8653776f4c24f",
  "assert": {
   "outcome": "success",
   "entityKind": "wall",
   "requiredFieldsPresent": true
  }
 },
 {
  "id": "golden/entities/wall-style-refs",
  "class": "golden",
  "artifact": "golden/entities/wall-style-refs.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.3 wall styleRefs",
   "FS §2.2 C3"
  ],
  "notes": "",
  "sha256": "de2ac63110ff5ab38b01e1f82862d223899dd19dab3781a6a6b8809bed579ae8",
  "assert": {
   "outcome": "success",
   "entityKind": "wall",
   "optionalSetsPresent": true
  }
 },
 {
  "id": "golden/entities/walljoin-two-members",
  "class": "golden",
  "artifact": "golden/entities/walljoin-two-members.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.3 wallJoin",
   "BE §3.3"
  ],
  "notes": "no coordinates, no levelId stored; >=2 unique members",
  "sha256": "6cbcdafeb66529937a4b1080fb5e4e677d711a24a2957d4c91c0a7f45ebb9d61",
  "assert": {
   "outcome": "success",
   "entityKind": "wallJoin",
   "memberCount": 2
  }
 },
 {
  "id": "golden/entities/opening-door",
  "class": "golden",
  "artifact": "golden/entities/opening-door.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.3 opening",
   "BE §4"
  ],
  "notes": "no levelId; effective level resolves through hostWallId at read time",
  "sha256": "320199483974375b60396bd30b3b6a25034440218f829418431fbec05d1c79b1",
  "assert": {
   "outcome": "success",
   "entityKind": "opening"
  }
 },
 {
  "id": "golden/entities/opening-window",
  "class": "golden",
  "artifact": "golden/entities/opening-window.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.3 opening",
   "BE §4"
  ],
  "notes": "swingDeg at range end 180 degrees",
  "sha256": "2904ee5b5612a3944e224c6c1f51d315425fe4a27e084367b21d7e7387311e0f",
  "assert": {
   "outcome": "success",
   "entityKind": "opening",
   "swingDegAtRangeEnd": true
  }
 },
 {
  "id": "golden/entities/roomrecord-seed-only",
  "class": "golden",
  "artifact": "golden/entities/roomrecord-seed-only.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.5",
   "BE §5.2"
  ],
  "notes": "optional display fields absent",
  "sha256": "d2ffafc896d8f9df2671d7ceb5029bbbe22356e5f8a5d4bda750dd640bdae983",
  "assert": {
   "outcome": "success",
   "entityKind": "roomRecord",
   "optionalFieldsAbsent": true
  }
 },
 {
  "id": "golden/entities/roomrecord-named",
  "class": "golden",
  "artifact": "golden/entities/roomrecord-named.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.3 roomRecord",
   "BE R5.3.3/F37d pattern"
  ],
  "notes": "absent vs empty-string distinct authoritatively",
  "sha256": "500726ce8ba0fc101728d31384fd3289ef4bacf5c17523b411889123b458651c",
  "assert": {
   "outcome": "success",
   "entityKind": "roomRecord",
   "optionalFieldsPresent": true
  }
 },
 {
  "id": "golden/entities/label-minimal",
  "class": "golden",
  "artifact": "golden/entities/label-minimal.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.3 Tier B label"
  ],
  "notes": "structural validity only; Tier B semantics deferred",
  "sha256": "c1ea78e73f1690259f3983379f07b672261330edfc9c76de75fa7afa0a1d1b90",
  "assert": {
   "outcome": "success",
   "entityKind": "label"
  }
 },
 {
  "id": "golden/entities/dimension-measured",
  "class": "golden",
  "artifact": "golden/entities/dimension-measured.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.3 Tier B dimension"
  ],
  "notes": "",
  "sha256": "63a9db35c354c18a63fd9cb403a2b658cebab1f1e8e8ea648563441c7ba3e15c",
  "assert": {
   "outcome": "success",
   "entityKind": "dimension"
  }
 },
 {
  "id": "golden/entities/asset-instance",
  "class": "golden",
  "artifact": "golden/entities/asset-instance.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.3 Tier B assetInstance",
   "AS §10.1"
  ],
  "notes": "digest identity namespace distinct from UUIDs",
  "sha256": "1f56718a6139d86cc1d3422d86907bac8d13b892d5b87ac44742b19c76c6b895",
  "assert": {
   "outcome": "success",
   "entityKind": "assetInstance"
  }
 },
 {
  "id": "golden/entities/bundle-coherent",
  "class": "golden",
  "artifact": "golden/entities/bundle-coherent.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.3 examples bundle",
   "FS §4.2 O4"
  ],
  "notes": "all referenced ids defined inside the bundle: referentially valid",
  "sha256": "a5e2a0747796d6d4a8e10b4e255be66b78e04e79ec404c08993821f0424e32f4",
  "assert": {
   "outcome": "success",
   "entityCount": 9,
   "referentiallySelfContained": true
  }
 },
 {
  "id": "golden/entities/c1-key-order-a",
  "class": "golden",
  "artifact": "golden/entities/c1-key-order-permutation-a.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.2 C1"
  ],
  "notes": "key order carries no meaning",
  "sha256": "e3044c867a50b3885adbc51f3e7935a3f6c23183da69906ac5284ea66e7d19b2",
  "assert": {
   "outcome": "success",
   "entityKind": "level",
   "keyOrderSemantic": false
  }
 },
 {
  "id": "golden/entities/c1-key-order-b",
  "class": "golden",
  "artifact": "golden/entities/c1-key-order-permutation-b.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.2 C1"
  ],
  "notes": "same entity as c1-key-order-a under sorted keys; readers MUST treat as equivalent",
  "sha256": "e97271ace6d7058c5eef572dadbc715f6e695cf82976a02673a269da3be261c2",
  "assert": {
   "outcome": "success",
   "entityKind": "level",
   "keyOrderSemantic": false
  }
 },
 {
  "id": "golden/entities/c3-walljoin-perm-a",
  "class": "golden",
  "artifact": "golden/entities/c3-walljoin-permutation-a.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.2 C3"
  ],
  "notes": "canonical order = ascending member UUID (E5)",
  "sha256": "6cbcdafeb66529937a4b1080fb5e4e677d711a24a2957d4c91c0a7f45ebb9d61",
  "assert": {
   "outcome": "success",
   "entityKind": "wallJoin",
   "setPermutationEquivalent": true
  }
 },
 {
  "id": "golden/entities/c3-walljoin-perm-b",
  "class": "golden",
  "artifact": "golden/entities/c3-walljoin-permutation-b.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.2 C3"
  ],
  "notes": "permutation of a; validators treat as equivalent while rejecting duplicates",
  "sha256": "25d84bc58fd7e215099adf0dafa9fc98663c41bc021d69da019cfc3f0bef6d95",
  "assert": {
   "outcome": "success",
   "entityKind": "wallJoin",
   "setPermutationEquivalent": true
  }
 },
 {
  "id": "golden/entities/e2-uuid-version-variants",
  "class": "golden",
  "artifact": "golden/entities/e2-uuid-version-variants.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §2.1 E2 version opacity"
  ],
  "notes": "v7/v4/v8-form ids equally valid; no rule may depend on UUID version",
  "sha256": "387b006a90fbf4dd7bd2873ad722bc4ecddd939390a6733dbd734936889e3695",
  "assert": {
   "outcome": "success",
   "distinctUuidVersions": [
    7,
    4,
    8
   ],
   "allEquallyValid": true
  }
 }
]
```

```json
[
 {
  "id": "golden/batches/single-create",
  "class": "golden",
  "artifact": "golden/batches/batch-single-create.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §4.9",
   "FS §4.1 B2/B3/B5"
  ],
  "notes": "normative batch shape; counterBase 42 reserves [42,42]",
  "sha256": "f66d173a204a564f7aa711be63ed3705eca4044695df72f2fd2547dcb9fd3b5e",
  "assert": {
   "outcome": "success",
   "operationKinds": [
    "createEntity"
   ],
   "counterBase": 42
  }
 },
 {
  "id": "golden/batches/multi-operation",
  "class": "golden",
  "artifact": "golden/batches/batch-multi-operation.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §4.2 O2/O3",
   "FS §4.1 B4"
  ],
  "notes": "create+setFields+deleteEntity in one atomic batch; setFields addresses top-level fields",
  "sha256": "087793d32fd1b475213a8c60508f26316a0e690d79f86fa659efdb247d0aa1c8",
  "assert": {
   "outcome": "success",
   "atomicBatch": true,
   "operationCount": 3
  }
 },
 {
  "id": "golden/batches/frontier-two-actors",
  "class": "golden",
  "artifact": "golden/batches/batch-frontier-two-actors.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §4.4 F1",
   "FS §4.1 B5"
  ],
  "notes": "multi-actor frontier advancing only at batch end",
  "sha256": "3d1483f291179e612bf6bc4c3ab0e39c9fbda153f583cad7eb508eabae36d6ba",
  "assert": {
   "outcome": "success",
   "frontierActorCount": 2
  }
 },
 {
  "id": "golden/snapshots/empty-projection",
  "class": "golden",
  "artifact": "golden/snapshots/snapshot-empty-projection.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §7.5",
   "FS §5.1"
  ],
  "notes": "brand-new document serializes empty projection rather than omitting the section",
  "sha256": "66449fe7bda0829d93d3d8a8ee7dca471780ed8df496ca31797f91bdd8c9dadf",
  "assert": {
   "outcome": "success",
   "entitiesEmpty": true
  }
 },
 {
  "id": "golden/snapshots/full-s1-s5",
  "class": "golden",
  "artifact": "golden/snapshots/snapshot-full-s1-s5.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §5.1 S1–S5"
  ],
  "notes": "winnerPositions and createdPositions reference the shared batchPositions table, which carries each referenced batch's own frontier; causalPastSize is the sum of that frontier and is never stored (FS 5.1 S3). createdPositions covers the tombstoned id as well as the live entities, so the live interval [creation, deleting) survives deletion (S1/S4). Re-derived by issue 0011 under ED-0043: the pre-ED-0037 shape carried winnerKeys K=(operationCounter,actorId) and a bare deletingKey, and carried no creation metadata at all",
  "sha256": "b2a3faf8c373908e81bb85394da72b23783fd53a31d22c3c8a5cd59ef807d0d7",
  "assert": {
   "outcome": "success",
   "winnerPositionsPresent": true,
   "createdPositionsPresent": true,
   "batchPositionsPresent": true,
   "tombstonesPresent": true
  }
 },
 {
  "id": "golden/assets/descriptors",
  "class": "golden",
  "artifact": "golden/assets/descriptors.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "AS §10.2"
  ],
  "notes": "full + minimal descriptors; required + optional requirement values; placeholderDimensions optional field",
  "sha256": "fe60d38f682f08552e049777428257c99eb79139565ad120db6421751911b60c",
  "assert": {
   "outcome": "success",
   "descriptorCount": 2
  }
 },
 {
  "id": "golden/assets/asset-root-sample",
  "class": "golden",
  "artifact": "golden/assets/asset-root-sample.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §7.4 I1–I4",
   "AS §10.5"
  ],
  "notes": "ascending by digest; physical path derived per AS §10.3, never stored",
  "sha256": "219c93729735e4319588c8693c94b2b3e514c15b694dd755684a71c2eeb7db81",
  "assert": {
   "outcome": "success",
   "ascendingByDigest": true
  }
 },
 {
  "id": "golden/assets/blob-tile",
  "class": "golden",
  "artifact": "golden/assets/blobs/9ca758f319e76338ee864522157a8733f1ce25b9e101e637d69a8d240b832022.bin",
  "outcome": "success",
  "category": null,
  "clauses": [
   "AS §10.1",
   "AS §10.3"
  ],
  "notes": "logical blob bytes B; sha256 over B equals the file-name digest mapping",
  "sha256": "9ca758f319e76338ee864522157a8733f1ce25b9e101e637d69a8d240b832022"
 },
 {
  "id": "golden/assets/blob-mesh",
  "class": "golden",
  "artifact": "golden/assets/blobs/524f170c49ff7c181ac9e7c36c7186b476c326499c721d51275bb0d088104e2d.bin",
  "outcome": "success",
  "category": null,
  "clauses": [
   "AS §10.1",
   "AS §10.3"
  ],
  "notes": "",
  "sha256": "524f170c49ff7c181ac9e7c36c7186b476c326499c721d51275bb0d088104e2d"
 },
 {
  "id": "golden/packages/thin-valid",
  "class": "golden",
  "artifact": "golden/packages/thin-valid.domo",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §7.2 profile",
   "FS §7.3 namespaces",
   "FS §7.4 manifest",
   "FS §7.5 snapshot placement",
   "FS §7.6 ops framing",
   "FS §7.9 thin"
  ],
  "notes": "store method; UTF-8 flag set; ascending entry order; zero-padded ops name. SUCCESS OUTCOME GATED ON UNRULED QUESTION: validity depends on whether FORMAT-SPEC permits partial-history packages (snapshot.frontier={} with counterBase=42, batch.frontier={A:41}). See gaps/partial-history-package-conformance. This is NOT the zero-valued frontier question (issue 0004 forbids {A:0}; empty {} is absence and remains legal).",
  "sha256": "2728d613927fb472def7fbea5bbe7e0739de2f0fd4a4d93fefbb18a0005ddde0"
 },
 {
  "id": "golden/packages/portable-valid",
  "class": "golden",
  "artifact": "golden/packages/portable-valid.domo",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §7.9 portable",
   "AS §10.4",
   "AS §10.3"
  ],
  "notes": "embeds every U(P) member digest-verified. SUCCESS OUTCOME GATED ON UNRULED QUESTION: validity depends on whether FORMAT-SPEC permits partial-history packages (snapshot.frontier={} with counterBase=42, batch.frontier={A:41}). See gaps/partial-history-package-conformance. This is NOT the zero-valued frontier question (issue 0004 forbids {A:0}; empty {} is absence and remains legal).",
  "sha256": "94eb8445be33fd1a0619382648633ac97a426adaa1cd191c8172c26013f209ca"
 },
 {
  "id": "golden/parser-twins/nesting-at-L11",
  "class": "golden",
  "artifact": "golden/parser-twin-nesting-depth-at-L11.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §9.3 L11"
  ],
  "notes": "depth == 128 parses (boundary succeeds). No depth-127 artifact exists and none is owed: a below-boundary case cannot fail unless this one does, so it would carry no discriminating power. Consumers needing 127 derive it from this twin's measured depth, which cannot drift from the artifact the way a third committed file could.",
  "sha256": "dbaec29ce2fb52a1a372e1da31b0d434d257fe11bebee2d31c6649710e3052a6",
  "rawByDesign": true,
  "rowRefs": [
   "L11"
  ]
 },
 {
  "id": "golden/parser-twins/number-token-at-L12",
  "class": "golden",
  "artifact": "golden/parser-twin-number-token-at-L12.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §9.3 L12"
  ],
  "notes": "64-character token is exactly at limit",
  "sha256": "b22ebd72c153beee0b0ad324190a9f799e2b3327294a2e293d6672eacc0078d7",
  "assert": {
   "outcome": "success",
   "tokenLengthAtLimit": 64
  },
  "rowRefs": [
   "L12"
  ]
 },
 {
  "id": "golden/parser-twins/integer-max-i64",
  "class": "golden",
  "artifact": "golden/parser-twin-integer-max-i64.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §9.3 L13"
  ],
  "notes": "signed 64-bit max fits",
  "sha256": "5aeea6dc4aa151d70c430db548d16a184d6a15497ba8fa1dc01b92325876a955",
  "assert": {
   "outcome": "success",
   "fitsSigned64Bit": true
  },
  "rowRefs": [
   "L13"
  ]
 }
]
```

## Adversarial — projection vectors V1–V7 (expected projections fixed by FS §4.7)

```json
[
 {
  "id": "adversarial/vectors/v1-duplicate-delivery",
  "class": "adversarial",
  "artifact": "adversarial/vectors/v1-duplicate-delivery.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §4.7 V1",
   "FS §4.5 I2"
  ],
  "notes": "second delivery ignored silently; state and frontier identical to single delivery",
  "sha256": "efa9bc67c50ded0a7a95e6d8b0fce40f77ec3c3ffe2a12cb2264755164f06a68",
  "assert": {
   "outcome": "success",
   "canonicalOutcome": "ignore-duplicate",
   "entityCountAfter": 1,
   "frontierAfter": {
    "01948f5a-ca77-7a09-9e76-5ace11b12fd2": 50
   }
  }
 },
 {
  "id": "adversarial/vectors/v2-reordered-delivery",
  "class": "adversarial",
  "artifact": "adversarial/vectors/v2-reordered-delivery.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §4.7 V2",
   "FS §4.4 F3"
  ],
  "notes": "causally-later batch held; projection equals in-order result exactly. The finalThicknessMm oracle is now ASSERTED FOR REAL against replayed projected state, and the winning write is additionally asserted as a POSITION so a coincidentally-correct value cannot pass for a correct ordering. Issue 0011's write position gives B-later causalPastSize 69 (frontier {Y:9, X:60}) against A-earlier's 59, so the causally later write wins and the projection holds 150. The executionBlocked marker and its residualBlockers entry were removed only once they had become UNTRUE - the harness's own guard fired first and said so - and the re-enabled check is mutation-proven per ED-0045",
  "sha256": "ba49e7c18146e8f5ace6f0e9aa836e49ecadae7ace0bd934119009fdafcf6fc9",
  "assert": {
   "outcome": "success",
   "canonicalOutcome": "hold-then-apply",
   "canonicalOrder": [
    "A-earlier",
    "B-later"
   ],
   "finalThicknessMm": 150
  }
 },
 {
  "id": "adversarial/vectors/v3-concurrent-set-set",
  "class": "adversarial",
  "artifact": "adversarial/vectors/v3-concurrent-set-set.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §4.7 V3",
   "FS §4.6 LWW1/LWW2"
  ],
  "notes": "lossy LWW: loser value discarded deterministically, independent of arrival order. Re-derived by issue 0011 under ED-0043 and the OUTCOME DOES NOT MOVE: X's frontier {X:69} gives causalPastSize 69 against Y's 19, so the leading component of the write position already decides for X, exactly as the pre-ED-0037 counter comparison 70 > 20 did. Only the winner's REPRESENTATION changes, from a counter/actor pair to a reference into batchPositions. ED-0037 offered the alternative of reshaping this fixture to use equal-past-size writes so the counter tie-break is exercised; issue 0011 took the re-derivation branch instead and covered the tie-break in internal/document (TestWritePositionTieBreaksAmongEqualPast), which isolates counterBase, actorId and operationIndex separately rather than blending them into one vector",
  "sha256": "638947d95d5e651f54a166ce0cdd2ef089e70210ae6d66ddec8795365d7be913",
  "assert": {
   "outcome": "success",
   "canonicalOutcome": "apply",
   "winner": {
    "field": "endMm",
    "batch": "0000000000000000070-01948f5a-ca77-7a09-9e76-5ace11b12fd2",
    "operationIndex": 0,
    "actorId": "01948f5a-ca77-7a09-9e76-5ace11b12fd2"
   },
   "loserValueDiscarded": true
  }
 },
 {
  "id": "adversarial/vectors/v4-delete-vs-set",
  "class": "adversarial",
  "artifact": "adversarial/vectors/v4-delete-vs-set.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §4.7 V4",
   "FS §4.6 LWW3"
  ],
  "notes": "delete dominates; tombstone retained. Re-derived by issue 0011 under ED-0043 and the OUTCOME DOES NOT MOVE: Y's frontier {Y:29} gives causalPastSize 29 against the delete's 79, so the set is placed before the delete and the delete then dominates (LWW3) - the same placement the pre-ED-0037 counters 30 < 80 gave. The retained datum is now the deleting POSITION, a reference into batchPositions, not a bare (operationCounter, actorId) pair",
  "sha256": "86a3a9a92278119b04606a35e9c4d77ccf804866a6dafc7d4f8428ba5223573f",
  "assert": {
   "outcome": "success",
   "tombstoneRetained": true,
   "deletingPositionBatch": "0000000000000000080-01948f5a-ca77-7a09-9e76-5ace11b12fd2",
   "concurrentWriteApplied": false
  }
 },
 {
  "id": "adversarial/vectors/v4b-stale-set-after-delete",
  "class": "adversarial",
  "artifact": "adversarial/vectors/v4b-stale-set-after-delete.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §4.7 V4b",
   "FS §4.5 I5"
  ],
  "notes": "whole batch discarded STALE; atomic; no resurrection",
  "sha256": "3395fdb42ad3bc20becfa894cb2d7ee071caa6a8dccc2a860d839ec55e728a22",
  "assert": {
   "outcome": "success",
   "canonicalOutcome": "discard-stale",
   "discardedBatchIds": [
    "01948fe2-0000-7000-8000-000000010003"
   ]
  }
 },
 {
  "id": "adversarial/vectors/v5-late-concurrent-after-compaction",
  "class": "adversarial",
  "artifact": "adversarial/vectors/v5-late-concurrent-after-compaction.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §4.7 V5",
   "FS §4.6 LWW5",
   "FS §5.1 S3"
  ],
  "notes": "snapshot winner POSITIONS make late-concurrent resolve identically to the uncompacted replica. Re-derived by issue 0011 under ED-0043 and the OUTCOME DOES NOT MOVE: the retained winner references batch 0000000000000000100-01948f5a-ca77-7a09-9e76-5ace11b12fd2, whose frontier {X:99} gives causalPastSize 99, against the late batch's 49, so the late write still loses and the field keeps its snapshot value",
  "sha256": "f2f503e73f9e2501a0ca1b82c2227d8b2c853067e25e89dc1f2292d702112193",
  "assert": {
   "outcome": "success",
   "lateBatchApplied": false,
   "watermarkSkipApplied": true
  }
 },
 {
  "id": "adversarial/vectors/v6-snapshot-replay",
  "class": "adversarial",
  "artifact": "adversarial/vectors/v6-snapshot-replay.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §4.7 V6"
  ],
  "notes": "authoritative projection equals full-history replay; no byte-identity claim (§3.3). Re-derived by issue 0011 under ED-0043 for FRAMING only: the fullHistory batches gained the writing actor's own prior end counter, which §4.4 F4 requires of every conforming writer and their frontiers omitted. The equality is now compared over winner positions, creation positions and the shared batchPositions table as well as entities and tombstones",
  "sha256": "3e0a6fbf1d0a778a977c234f93d49d3bad1170f779348bede18aae081ea4db40",
  "assert": {
   "outcome": "success",
   "authoritativeStateEqualsFullReplay": true
  }
 },
 {
  "id": "adversarial/vectors/v7-create-vs-modify",
  "class": "adversarial",
  "artifact": "adversarial/vectors/v7-create-vs-modify-both-orderings.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §4.7 V7",
   "FS §4.5 I5"
  ],
  "notes": "both canonical orderings recorded; outcome depends on delivered SET only. Re-derived by issue 0011 under ED-0043 and BOTH OUTCOMES DO NOT MOVE: causalPastSize 54 < 109 keeps the modify canonically before the create (discarded stale), and 109 < 119 keeps the create before the following modify (applies). Only the artifact's rationale changed - it named the ascending-K ordering of two concurrent batches, which no longer decides anything",
  "sha256": "a637bd442cae441dc41cfaad9f1a2936e99b9079a4a369d98ddc8961b6d1a294",
  "assert": {
   "outcome": "success",
   "orderingOutcomes": {
    "modify-canonically-precedes": "discard-stale",
    "modify-canonically-follows": "apply"
   }
  }
 }
]
```

## Adversarial — warnings, degraded classes, unknown data

```json
[
 {
  "id": "adversarial/warnings/duplicate-level-elevations",
  "class": "adversarial",
  "artifact": "adversarial/warnings/duplicate-level-elevations.json",
  "outcome": "warning",
  "category": null,
  "clauses": [
   "FS §4.3 warning-class row",
   "BE §2"
  ],
  "notes": "batch COMMITS; warning surfaces alongside effects",
  "sha256": "587d63819e43141ae7076018c573a88254861863d54dabd1e6b396f0f39c9ed4",
  "assert": {
   "outcome": "warning",
   "commits": true,
   "warnings": 1
  }
 },
 {
  "id": "adversarial/warnings/ceiling-below-wall-height",
  "class": "adversarial",
  "artifact": "adversarial/warnings/ceiling-below-wall-height.json",
  "outcome": "warning",
  "category": null,
  "clauses": [
   "FS §4.3 warning-class row",
   "BE §2"
  ],
  "notes": "",
  "sha256": "a97461d92371f276143e30887a80a388d739d958a790da55d6945414ec31cac9",
  "assert": {
   "outcome": "warning",
   "commits": true,
   "warnings": 1
  }
 },
 {
  "id": "adversarial/degraded/thin-missing-optional-asset",
  "class": "adversarial",
  "artifact": "adversarial/degraded/thin-missing-optional-asset.json",
  "outcome": "degraded-read-only-inspection",
  "category": null,
  "clauses": [
   "BE §9.6",
   "AS §10.6"
  ],
  "notes": "missing optional; placeholder from descriptor placeholderDimensions",
  "sha256": "81101b9463f22b7d652797354323fbb1b148b40206eb33f240315641170038c4",
  "assert": {
   "outcome": "degraded-read-only-inspection",
   "placeholderSource": "descriptor-placeholderDimensions"
  }
 },
 {
  "id": "adversarial/degraded/dangling-well-formed-digest",
  "class": "adversarial",
  "artifact": "adversarial/degraded/dangling-well-formed-digest.json",
  "outcome": "degraded-read-only-inspection",
  "category": null,
  "clauses": [
   "AS §10.6",
   "BE §9.6"
  ],
  "notes": "well-formed but unresolved => required-missing treatment; fixed 1000 mm box placeholder; portable packing fails completely",
  "sha256": "bb0e2376a41dbf94a9ec6742c03013e446a142755c30b9b7876a739e77fd0f2e",
  "assert": {
   "outcome": "degraded-read-only-inspection",
   "portablePackingFailsCompletely": true
  }
 },
 {
  "id": "adversarial/degraded/descriptorless-reference",
  "class": "adversarial",
  "artifact": "adversarial/degraded/descriptorless-reference.json",
  "outcome": "degraded-read-only-inspection",
  "category": null,
  "clauses": [
   "AS §10.6",
   "BE §9.6"
  ],
  "notes": "distinct from malformed digest syntax which rejects as hostile input",
  "sha256": "3a49fb190e6f5e67ffd389ca50ad707b2f1a2f0cb3999b3ddbdfa1ebc2062f8f",
  "assert": {
   "outcome": "degraded-read-only-inspection"
  }
 },
 {
  "id": "adversarial/degraded/digest-mismatch-thin",
  "class": "adversarial",
  "artifact": "adversarial/degraded/digest-mismatch-thin-missing-equivalence.json",
  "outcome": "missing-asset-equivalence",
  "category": null,
  "clauses": [
   "FS §9.4 digest mismatch row",
   "BE §9.6"
  ],
  "notes": "digest mismatch treated EXACTLY like a missing blob, never corruption",
  "sha256": "19bfe993d5ee4103401fb77d963f62f0020a001a0a180d75c25bf4060d69d56e",
  "assert": {
   "outcome": "missing-asset-equivalence"
  }
 },
 {
  "id": "adversarial/unknown-data/unknown-optional-preserve",
  "class": "adversarial",
  "artifact": "adversarial/unknown-data/unknown-optional-entity-field-preserve.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §8 U2",
   "FS §9.3 L32"
  ],
  "notes": "deterministic PRESERVE variant (HIGH-4): read succeeds; write-back carries futureNote with identical meaning; silent stripping forbidden. The refusal alternative is separate deterministic record adversarial/unknown-data/write-refusal-unpreservable",
  "sha256": "ff92b0225fff348159483ae7dd2238937747e7e332a4577362c1229dee4b9eaa",
  "assert": {
   "outcome": "success-on-read",
   "readOutcome": "success",
   "writeBackContract": "preserve",
   "preservedFields": [
    "futureNote"
   ],
   "silentStrippingForbidden": true
  },
  "rowRefs": [
   "L32"
  ]
 },
 {
  "id": "adversarial/unknown-data/unknown-required-refuse",
  "class": "adversarial",
  "artifact": "adversarial/unknown-data/unknown-required-entity-field-refuse.json",
  "outcome": "reject",
  "category": "unsupported-version",
  "clauses": [
   "FS §8 U1",
   "FS §8.2 V3"
  ],
  "notes": "unrecognized required construct under a newer label refuses before any effect",
   "sha256": "9a8734a6de445a5246f052c065cc50be0c05347634dbe1ffefb038a8726516ed",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "adversarial/unknown-data/write-refusal-unpreservable",
  "class": "adversarial",
  "artifact": "adversarial/unknown-data/write-refusal-unpreservable.json",
  "outcome": "safe-write-refusal",
  "category": null,
  "clauses": [
   "FS §8 U4",
   "FS §7.4 I2"
  ],
  "notes": "partial-knowledge writer never rewrites selectively",
  "sha256": "3f269674485c7975c589b88a53398682314df4339f5112483e7558e786e92eab",
  "assert": {
   "outcome": "safe-write-refusal",
   "writeBackContract": "safe-write-refusal",
   "partialKnowledgeSelectiveRewriteForbidden": true
  },
  "rowRefs": [
   "L32"
  ]
 }
]
```

## Migration and schema identifier (FS §8)

```json
[
 {
  "id": "migration/baseline-0.1-manifest",
  "class": "migration",
  "artifact": "migration/baseline-0.1-manifest.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §8.1 V1"
  ],
  "notes": "the sole accepted schema label for v0.1 fixtures",
  "sha256": "d0864fec54fd762c7d5efbea97939fecd99fd26b9b09d36a5ae321c47a0326cf",
  "assert": {
   "outcome": "success",
   "acceptedFormatVersion": "0.1"
  }
 },
 {
  "id": "migration/newer-minor-reject",
  "class": "migration",
  "artifact": "migration/newer-minor-reject.json",
  "outcome": "reject",
  "category": "unsupported-version",
  "clauses": [
   "FS §8.2 V3"
  ],
  "notes": "minor above supported; actionable diagnostic names BOTH versions; guessing forbidden",
   "sha256": "c10626da876f2d33b4d63a078d99a4637ff5a2344ffc7be92ec9a5c7fa458873",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "migration/older-minor-unknown-label",
  "class": "migration",
  "artifact": "migration/older-minor-unknown-label.json",
  "outcome": "reject",
  "category": "unsupported-version",
  "clauses": [
   "FS §8.2 V3"
  ],
  "notes": "'0.0' is not among the minors this reader supports; V3 pins support, not release status",
  "sha256": "350e4224dbb80156962c5411f8563480418e84397f8ba714d9b54d4838c3bf66",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "migration/foreign-namespace-entry",
  "class": "migration",
  "artifact": "migration/foreign-namespace-entry.json",
  "outcome": "reject",
  "category": "invalid-entry-name",
  "clauses": [
   "FS §8 U3",
   "FS §7.3"
  ],
  "notes": "new namespaces arrive only via a new schema identifier (V5); never smuggled under an old label",
  "sha256": "24662641040a0589303c8cae9bc68e4afba3a21c900c356f7979a0c86b2ceafe",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "migration/v4-tested-migration-path",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "FS §8.2 V4"
  ],
  "implementationDeferred": true,
  "notes": "V4 requires every RELEASED 0.N schema to have a tested migration path in later releases; only '0.1' exists and no successor schema has specified semantics, so a positive migration golden cannot be authored honestly yet. Recorded per reviewer amendment 5; unblocks when issue 0031 defines its first migrated-from schema."
 }
]
```

## Hostile — physical container defects (.domo)

Every committed hostile container is inert and ≤256 KiB; each defect is a
byte-level delta over a conforming base package built by the Rust corpus generator.

```json
[
 {
  "id": "hostile/container/empty-file",
  "class": "hostile",
  "artifact": "hostile/container/empty-file.domo",
  "outcome": "reject",
  "category": "malformed-container",
  "clauses": [
   "FS §9.4 empty input row"
  ],
  "notes": "zero-byte input",
  "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
 },
 {
  "id": "hostile/container/bad-signature",
  "class": "hostile",
  "artifact": "hostile/container/bad-signature.domo",
  "outcome": "reject",
  "category": "malformed-container",
  "clauses": [
   "FS §9.4 invalid signature row"
  ],
  "sha256": "4832bdf5d66ce7878c6a8f8d9ae74994b71bc1652b22e3c9ee6d49ce2564e88b"
 },
 {
  "id": "hostile/container/truncated-header",
  "class": "hostile",
  "artifact": "hostile/container/truncated-header.domo",
  "outcome": "reject",
  "category": "malformed-container",
  "clauses": [
   "FS §9.4 truncated header/payload/index/footer row"
  ],
  "sha256": "6d921a5cd02ec2b78a8222a0d058ac52f6008ea0ec129e931dabbd1cf6b4143b"
 },
 {
  "id": "hostile/container/truncated-eocd",
  "class": "hostile",
  "artifact": "hostile/container/truncated-eocd.domo",
  "outcome": "reject",
  "category": "malformed-container",
  "clauses": [
   "FS §9.4 truncated index/footer row"
  ],
  "sha256": "9f31bb477ec0dcc98e7a5dd90b12e1e1208ff0211508f1f8a180e6a48968a9f9"
 },
 {
  "id": "hostile/container/trailing-data",
  "class": "hostile",
  "artifact": "hostile/container/trailing-data.domo",
  "outcome": "reject",
  "category": "trailing-data",
  "clauses": [
   "FS §9.4 rule T1"
  ],
  "notes": "bytes appended after the container terminator; nothing ignored silently",
  "sha256": "66cc1691eba88aa9e1a38ffab5fb11cc5d4e2cee8be9911462e041e0fa3d56eb"
 },
 {
  "id": "hostile/container/concatenated-packages",
  "class": "hostile",
  "artifact": "hostile/container/concatenated-packages.domo",
  "outcome": "reject",
  "category": "trailing-data",
  "clauses": [
   "FS §9.4 T1"
  ],
  "notes": "two complete packages concatenated",
  "sha256": "6301962246ee3489e176742322edec0efb3adb1d8dedadbcb5315c99a5eabd6e"
 },
 {
  "id": "hostile/container/overlapping-regions",
  "class": "hostile",
  "artifact": "hostile/container/overlapping-regions.domo",
  "outcome": "reject",
  "category": "overlapping-regions",
  "clauses": [
   "FS §9.4 T1"
  ],
  "notes": "central-directory offset moved into the local payload region",
  "sha256": "a89b533e24df780ec89acf278b690e277f301038d467368745087caf7f4d11ea"
 },
 {
  "id": "hostile/container/malformed-length",
  "class": "hostile",
  "artifact": "hostile/container/malformed-length.domo",
  "outcome": "reject",
  "category": "malformed-length",
  "clauses": [
   "FS §9.4 malformed length row",
   "FS §9.3 C3 declared-length validation"
  ],
  "notes": "declared-vs-observed contradiction; declared sizes never drive allocation",
  "sha256": "b67e01b81c06337c681856b478d0d40177ed889c82329c7de02eb765008c52e1"
 },
 {
  "id": "hostile/container/encrypted-entry",
  "class": "hostile",
  "artifact": "hostile/container/encrypted-entry.domo",
  "outcome": "reject",
  "category": "encrypted-entry",
  "clauses": [
   "FS §7.2",
   "FS §9.4 encrypted-entry row"
  ],
  "notes": "general-purpose bit 0 set; unconditional reject in v0.1",
  "sha256": "19b9212b9d37b934be845b2820be932d46b4f8332b678d0786fb5026c717d28d"
 },
 {
  "id": "hostile/container/unsupported-method-required-scope",
  "class": "hostile",
  "artifact": "hostile/container/unsupported-method.domo",
  "outcome": "reject",
  "category": "unsupported-method",
  "clauses": [
   "FS §7.7 allowlist",
   "FS §9.4 unsupported-method row (required-entry scope)"
  ],
  "notes": "method 12 outside {0 store, 8 deflate} on a structural entry",
  "sha256": "2079288b23926683b6ce89b085d02c9c2bec57e98f9ff9e3d2de0f9f5abd9bc0"
 },
 {
  "id": "hostile/container/checksum-error",
  "class": "hostile",
  "artifact": "hostile/container/checksum-error.domo",
  "outcome": "reject",
  "category": "checksum-error",
  "clauses": [
   "FS §7.2 CRC-32 verified",
   "FS §9.4 malformed checksum row"
  ],
  "notes": "consistent but wrong CRC in both headers; detects corruption, confers no authenticity",
  "sha256": "a4ac8cbde48d21302bc28f19611ae51bc13559e2b1ef61aa49ff80864e898996"
 },
 {
  "id": "hostile/container/utf8-flag-clear",
  "class": "hostile",
  "artifact": "hostile/container/utf8-flag-clear.domo",
  "outcome": "reject",
  "category": "invalid-entry-name",
  "clauses": [
   "FS §7.2 UTF-8 names only"
  ],
  "notes": "language-encoding bit 11 clear",
  "sha256": "aaf1356a59a1fe56bf508c47f8879616f2d835e8c32b9859e442c1ee62535b87"
 },
 {
  "id": "hostile/container/nonzero-comment",
  "class": "hostile",
  "artifact": "hostile/container/nonzero-comment.domo",
  "outcome": "reject",
  "category": "unsupported-extension",
  "clauses": [
   "FS §7.2 no comments"
  ],
  "notes": "ENTRY comment limb of the \u00a77.2 rule; the archive-comment limb is hostile/container/nonzero-archive-comment",
  "sha256": "47c5e4e11d517571983e9268f9242301e3b7359f136a6cb48bab5e59a2aec700"
 },
 {
  "id": "hostile/container/nonzero-archive-comment",
  "class": "hostile",
  "artifact": "hostile/container/nonzero-archive-comment.domo",
  "outcome": "reject",
  "category": "unsupported-extension",
  "clauses": [
   "FS \u00a77.2 no comments"
  ],
  "notes": "ARCHIVE comment limb of the \u00a77.2 rule: the EOCD carries a 36-byte comment and a commentLength that matches it, so the archive is otherwise well formed and the only defect is the comment. Added because the rule reads \"Entry comments AND the archive comment\" and only the entry limb was exercised; a reader enforcing one and ignoring the other passed the whole corpus (issue 0029, corpus-defect).",
  "sha256": "6bd194a617cd0fe00f222d5e6951efb445b1fb6f4988255b24ac62f1f868438d"
 },
 {
  "id": "hostile/container/foreign-extra-field",
  "class": "hostile",
  "artifact": "hostile/container/foreign-extra-field.domo",
  "outcome": "reject",
  "category": "unsupported-extension",
  "clauses": [
   "FS §7.2 extra fields restricted"
  ],
  "notes": "only the Zip64 extra field 0x0001 is permitted",
  "sha256": "3d9d2cce42694c104453c10e63e62d13f2d0a456145a27ba7e1d8675460257d0"
 },
 {
  "id": "hostile/container/version-needed-over",
  "class": "hostile",
  "artifact": "hostile/container/version-needed-over.domo",
  "outcome": "reject",
  "category": "unsupported-version",
  "clauses": [
   "FS §7.2 version bound"
  ],
  "notes": "version-needed-to-extract 46 > 45",
  "sha256": "d43b46a1add0c493df87b8e516a3a0892726073212066ecedf3ed4c94af4f47d"
 },
 {
  "id": "hostile/container/directory-entry-attr",
  "class": "hostile",
  "artifact": "hostile/container/directory-entry-attr.domo",
  "outcome": "reject",
  "category": "invalid-entry-name",
  "clauses": [
   "FS §7.2 regular-file entries only"
  ],
  "notes": "MS-DOS directory attribute bit set",
  "sha256": "f80b38a21268532c7d99aafb1bacca709b0f54ac5ac66ce3818f5e1aeb23ec80"
 },
 {
  "id": "hostile/container/symlink-designation",
  "class": "hostile",
  "artifact": "hostile/container/symlink-designation.domo",
  "outcome": "reject",
  "category": "invalid-entry-name",
  "clauses": [
   "FS §7.2",
   "FS §9.4 special-file designations"
  ],
  "notes": "Unix creator system with symlink mode bits: symlink rejection realized at carrier level",
  "sha256": "56b92ef3598bfea4c94e986c544aad1f460d2ca5a7876741db16cb649505261e"
 },
 {
  "id": "hostile/container/spanned-disk-number",
  "class": "hostile",
  "artifact": "hostile/container/spanned-disk-number.domo",
  "outcome": "reject",
  "category": "malformed-container",
  "clauses": [
   "FS §7.2 single-disk non-spanned archive"
  ],
  "notes": "EOCD thisDiskNumber (record offset 4) is nonzero; the fixture is golden/packages/thin-valid.domo with exactly that one field changed, asserted byte-exactly by TestSpannedDiskFixtureDiffersOnlyInTheDiskField. The superseded fixture patched a cdOffset byte instead and still rejected — as malformed-length — so the category alone does not attribute the reject to FS 7.2",
  "sha256": "3cd5d8470a63c930eb1bddbab8b9d056be6b726a2604817c188cc877e2027eec"
 },
 {
  "id": "hostile/container/zip64-malformed-locator",
  "class": "hostile",
  "artifact": "hostile/container/zip64-malformed-locator.domo",
  "outcome": "reject",
  "category": "malformed-container",
  "clauses": [
   "FS §7.8 item 4 fail-closed T1"
  ],
  "notes": "Zip64 locator with impossible offsets appended after EOCD",
  "sha256": "9b24c8d25f20ac6189b7cbe74442c06dc68167e3b2affdc51dfaa536d6471410"
 },
 {
  "id": "hostile/container/zip64-truncated-eocd64",
  "class": "hostile",
  "artifact": "hostile/container/zip64-truncated-eocd64.domo",
  "outcome": "reject",
  "category": "malformed-container",
  "clauses": [
   "FS §7.8 item 4"
  ],
  "notes": "truncated Zip64 EOCD record",
  "sha256": "46065c5636683105e25b3f5dff63ffa4fc018889e3a7c3f49abd3f89962528f1"
 },
 {
  "id": "hostile/container/missing-manifest",
  "class": "hostile",
  "artifact": "hostile/container/missing-manifest.domo",
  "outcome": "reject",
  "category": "missing-manifest",
  "clauses": [
   "FS §7.4 exactly one manifest",
   "FS §9.4 missing manifest row"
  ],
  "notes": "structurally valid archive whose domowik.json is absent",
  "sha256": "f5567e38497799a9b9ef03633922c33c2bb5ba5a321e769587d70ddae90d0b4d"
 },
 {
  "id": "hostile/container/digest-mismatch-portable",
  "class": "hostile",
  "artifact": "hostile/container/digest-mismatch-portable.domo",
  "outcome": "missing-asset-equivalence",
  "category": null,
  "clauses": [
   "FS §7.4 entries inventory integrity layer",
   "FS §9.4 digest mismatch row",
   "AS §10.4"
  ],
  "notes": "embedded blob altered after manifest construction: portability claim treated as broken, never presented as holding; digest mismatch ≡ missing blob",
  "sha256": "78a37992c86caf90c94593ac0c847c4fbab7f7689795892ff7a3447640ff0ac3"
 },
 {
  "id": "hostile/container/ratio-under-L15-meta",
  "class": "hostile",
  "artifact": "hostile/container/ratio-under-L15-success-twin.meta.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS 9.3 L15"
  ],
  "notes": "machine-readable measured sizes (logicalBytes/storedBytes/ratioPercent); TestRatioTwinMeta recomputes both deterministically on every run and asserts the ratio stays strictly below L15",
  "sha256": "238e3cb64f93386c2a0427b36a33f92d6788b22d9ecf9a1a5b006b31c6329503"
 },
 {
  "id": "gaps/ambiguous-manifest-deterministic-trigger",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "FS 7.4 exactly one manifest",
   "FS 9.4 ambiguous-manifest row",
   "FS 7.3 closed namespaces"
  ],
  "implementationDeferred": true,
  "notes": "UPSTREAM SPEC BLOCKER (final review): within the normative v0.1 profile - closed namespaces (FS 7.3), injective blob mapping, unconditional duplicate-logical-name rejection - no construct can produce multiple manifests independently of naming; every candidate route collides at name normalization first. Taxonomy coverage for ambiguous-manifest is therefore NOT CLAIMED; the category remains in the audit as blocked-pending-owner-decision (FS 7.4/9.4 owners)"
 }
]
```

## Hostile — names and paths (synthetic names only; nothing is extracted)

```json
[
 {
  "id": "hostile/container/traversal-dotdot",
  "class": "hostile",
  "artifact": "hostile/container/traversal-dotdot.domo",
  "outcome": "reject",
  "category": "path-traversal",
  "clauses": [
   "FS §9.4 names and paths",
   "FS §7.3"
  ],
  "notes": "'..' segment inside package metadata only",
  "sha256": "00c9dc2e7f24a775b5cc880095cfa81a2f710bc2ea8be90f71b1baebc594493c"
 },
 {
  "id": "hostile/container/absolute-path",
  "class": "hostile",
  "artifact": "hostile/container/absolute-path.domo",
  "outcome": "reject",
  "category": "path-traversal",
  "clauses": [
   "FS §9.4 absolute paths"
  ],
  "sha256": "78da1c8248c2494a78c1d1e83ea6de051522e8c7aad35cd18c3bcd18ec5459e0"
 },
 {
  "id": "hostile/container/drive-form",
  "class": "hostile",
  "artifact": "hostile/container/drive-form.domo",
  "outcome": "reject",
  "category": "path-traversal",
  "clauses": [
   "FS §9.4 drive forms"
  ],
  "sha256": "50621c9b4892ddd65bb195dcc6797926fb46efeb60ef19b1b3bc1330130ec793"
 },
 {
  "id": "hostile/container/backslash-name",
  "class": "hostile",
  "artifact": "hostile/container/backslash-name.domo",
  "outcome": "reject",
  "category": "path-traversal",
  "clauses": [
   "FS §9.4 mixed separators"
  ],
  "sha256": "a403cff27c3d8046c53f93010d0e995136d6f02986a57d8b8542091a4789e7c5"
 },
 {
  "id": "hostile/container/nul-byte-name",
  "class": "hostile",
  "artifact": "hostile/container/nul-byte-name.domo",
  "outcome": "reject",
  "category": "invalid-entry-name",
  "clauses": [
   "FS §9.4 NUL/control characters"
  ],
  "sha256": "4a40821636303b6f567c76b90dbd8bdb4e4c61db13b4fac3fcfd0d26050d38c3"
 },
 {
  "id": "hostile/container/dot-segment-name",
  "class": "hostile",
  "artifact": "hostile/container/dot-segment-name.domo",
  "outcome": "reject",
  "category": "path-traversal",
  "clauses": [
   "FS §9.4 dot segments"
  ],
  "sha256": "8fecdbd1b2cdd2ba51e695b48e4c2f758ac4744edeaa3812f9b2e5d64a209a3b"
 },
 {
  "id": "hostile/container/overlong-component-L9",
  "class": "hostile",
  "artifact": "hostile/container/overlong-component-L9.domo",
  "outcome": "reject",
  "category": "limit-name-component",
  "clauses": [
   "FS §9.3 L9"
  ],
  "notes": "single component of 300 UTF-8 bytes (>255)",
  "sha256": "a3f8bb87a6fcfe545086bef5b641f1acc2797627ebd558084a50aeefe973ccbe",
  "rowRefs": [
   "L9"
  ]
 },
 {
  "id": "hostile/container/overlong-name-L8",
  "class": "hostile",
  "artifact": "hostile/container/overlong-name-L8.domo",
  "outcome": "reject",
  "category": "limit-name-length",
  "clauses": [
   "FS §9.3 L8"
  ],
  "notes": "total path >1024 bytes via components each within L9",
  "sha256": "c6e703719f219f628d3b7aed1e5bff14907d464f4475af50f716d6e80126f9db",
  "rowRefs": [
   "L8"
  ]
 },
 {
  "id": "hostile/container/duplicate-logical-name",
  "class": "hostile",
  "artifact": "hostile/container/duplicate-logical-name.domo",
  "outcome": "reject",
  "category": "duplicate-logical-name",
  "clauses": [
   "FS §7.3",
   "FS §9.4 duplicate logical names row"
  ],
  "notes": "distinct from tolerated duplicate PHYSICAL occurrences up to L29",
  "sha256": "ce6e0dc506f754bbf5f74b2c718c0f98fcd29e22bb185250352dea5e593304a6"
 },
 {
  "id": "hostile/container/name-casefold-collision",
  "class": "hostile",
  "artifact": "hostile/container/name-casefold-collision.domo",
  "outcome": "reject",
  "category": "duplicate-logical-name",
  "clauses": [
   "FS §7.3 normalization comparison",
   "FS §9.4 colliding after normalization"
  ],
  "notes": "SNAPSHOT/STATE.JSON vs snapshot/state.json",
  "sha256": "1a23c8036615cf2a4f0796929359015d595862c79e89cacabc090c6d3d7012b6"
 },
 {
  "id": "hostile/container/ambiguous-manifest",
  "class": "hostile",
  "artifact": "hostile/container/ambiguous-manifest.domo",
  "outcome": "reject",
  "category": "duplicate-logical-name",
  "clauses": [
   "FS §7.4 exactly one manifest",
   "FS §9.4 multiple manifests row",
   "FS §9.5 precedence"
  ],
  "notes": "domowik.json plus DOMOWIK.JSON: deterministic rejection class is duplicate-logical-name via the FS 7.3 normalization comparison; whether any structure can produce multiple manifests independently of naming inside the closed namespace set (FS 7.4 / FS 9.4 ambiguous-manifest row) is unresolved - recorded as machine-readable gap gaps/ambiguous-manifest-deterministic-trigger",
  "sha256": "e3bd8c19efb256a6bdb7ebcf782dd6d8b58bc01b7147c6ff36450d585a5b11ea"
 }
]
```

## Hostile — bombs and resource limits (INERT; see README safety section)

```json
[
 {
  "id": "hostile/container/ratio-under-L15-success-twin",
  "class": "hostile",
  "artifact": "hostile/container/ratio-under-L15-success-twin.domo",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §9.3 L15 boundary"
  ],
   "notes": "genuine deflate stream whose payload is a CONFORMING FS 5.1 snapshot document carrying the COMPLETE S1-S5 member set in the writer's own shape (entities, frontier, batchPositions, winnerPositions, createdPositions, tombstones, batchMetadata.recentBatchIds), padded to the corpus cap with insignificant inter-token whitespace (FS 3.3), so the SUCCESS oracle holds end to end; RE-DERIVED by issue 0011 under ED-0043 — the generator's constant still emitted the pre-ED-0037 winnerKeys/createdKeys and no batchPositions while claiming to match the writer, which made the payload a LEGACY snapshot rather than the conforming one the oracle names, and the parser now rejects an omitted required member outright; expansion RE-MEASURED at 704:1, still below the ratified 1000:1 rejection ratio; fully inert (262144 B logical, within cap). The manifest is built over the patched entry set: the superseded fixture swapped the payload AFTER manifest construction, leaving a stale inventory digest (checksum-error) around bytes that were not a parsable snapshot document either. The pure L15 boundary is carried by hostile/recipes/limit-expansion-ratio-L15-clean. SUCCESS OUTCOME GATED ON UNRULED QUESTION: validity depends on whether FORMAT-SPEC permits partial-history packages (snapshot.frontier={} with counterBase=42, batch.frontier={A:41}). See gaps/partial-history-package-conformance. This is NOT the zero-valued frontier question (issue 0004 forbids {A:0}; empty {} is absence and remains legal).",
  "sha256": "99b84f2febe8a6f8e20d9733aeb88553194d6369a1d1fef7aeaec523ff19b758",
  "rowRefs": [
   "L15"
  ]
 },
 {
  "id": "hostile/container/limit-expansion-ratio-over-L15",
  "class": "hostile",
  "artifact": "hostile/container/limit-expansion-ratio-over-L15.domo",
  "outcome": "reject",
  "category": "malformed-length",
  "clauses": [
   "FS §9.3 L15"
  ],
  "notes": "declared uncompressed size encodes a 1001:1 ratio against a stored stream at or above the L15 small-input floor: deterministic rejection class is malformed-length (declared-vs-observed contradiction; FS 9.4/FS 9.5); pure L15 boundary carried by hostile/recipes/limit-expansion-ratio-L15-clean",
  "sha256": "d881a5ef5906faa02449e92c93672b6bb15900e8a6df9da4d66ac59bc0fc6e1a"
 },
 {
  "id": "hostile/container/declared-size-lie-limit-L3",
  "class": "hostile",
  "artifact": "hostile/container/declared-size-lie-limit-L3.domo",
  "outcome": "reject",
  "category": "malformed-length",
  "clauses": [
   "FS §9.3 L3",
   "FS §9.4 malformed length row",
   "FS §9.5"
  ],
  "notes": "declared logical size exceeds the ratified L3 quantity by one byte with a tiny stream; deterministic rejection class is malformed-length (FS 9.4 declared-vs-observed row; FS 9.5 physical stage wins); pure L3 boundary carried by clean recipe hostile/recipes/limit-entry-expanded-L3",
  "sha256": "31e1ca0ac8d9ec8600434679e4dbd400f63a6de12e9806c53ddfd54e4be9ee40",
  "rowRefs": [
   "L3"
  ]
 },
 {
  "id": "hostile/recipes/limit-total-expanded-L1",
  "class": "hostile",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L1"
   ],
   "declaredLogicalSize": "cumulativeExpanded == L1 succeeds (== N succeeds); L1+1 rejected before yielding the next chunk",
   "boundaries": [
    {
     "quantity": "cumulativeExpandedBytesPerOpen",
     "row": "L1",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-total-expanded",
  "clauses": [
   "FS §9.3 L1"
  ],
  "implementationDeferred": true,
  "notes": "cumulative counter probe as bounded recipe per reviewer amendment 2"
 },
 {
  "id": "hostile/recipes/limit-total-physical-L2",
  "class": "hostile",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L2"
   ],
   "declaredLogicalSize": "rawInputBytes > L2 rejected before parsing",
   "boundaries": [
    {
     "quantity": "totalPhysicalInputBytes",
     "row": "L2",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-total-physical",
  "clauses": [
   "FS §9.3 L2"
  ],
  "implementationDeferred": true,
  "notes": ""
 },
 {
  "id": "hostile/recipes/limit-entry-physical-L4",
  "class": "hostile",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L4"
   ],
   "declaredLogicalSize": "storedEntryBytes > L4 on the raw entry stream",
   "boundaries": [
    {
     "quantity": "perEntryPhysicalBytes",
     "row": "L4",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-entry-physical",
  "clauses": [
   "FS §9.3 L4"
  ],
  "implementationDeferred": true,
  "notes": ""
 },
 {
  "id": "hostile/recipes/limit-blob-bytes-L5",
  "class": "hostile",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L5"
   ],
   "declaredLogicalSize": "len(B) > L5 during streaming digest verification, before cache admission",
   "boundaries": [
    {
     "quantity": "perBlobLogicalBytes",
     "row": "L5",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-blob-bytes",
  "clauses": [
   "FS §9.3 L5",
   "AS §10.1"
  ],
  "implementationDeferred": true,
  "notes": ""
 },
 {
  "id": "hostile/recipes/limit-manifest-reserve-L7",
  "class": "hostile",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L7"
   ],
   "declaredLogicalSize": "computedReserve(entryCount x minimumRecordOverhead) > L7 rejected BEFORE any proportional allocation; declared lengths never drive reservation",
   "boundaries": [
    {
     "quantity": "manifestAssetRootReserveBytes",
     "row": "L7",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-manifest-reserve",
  "clauses": [
   "FS §9.3 L7",
   "FS §7.4 C4"
  ],
  "implementationDeferred": true,
  "notes": ""
 },
 {
  "id": "hostile/recipes/limit-aggregate-expansion-L16",
  "class": "hostile",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L15",
    "L16"
   ],
   "declaredLogicalSize": "many entries each just UNDER L15 whose aggregate exceeds totalPhysical x 100 AND 64 MiB; both conditions required",
   "boundaries": [
    {
     "quantity": "aggregateExpansionRatioWithFloor",
     "row": "L16",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-aggregate-expansion",
  "clauses": [
   "FS §9.3 L16",
   "FS §9.3 L15 interaction"
  ],
  "implementationDeferred": true,
  "notes": "ratio/nested-limit interaction vector owed by the §9.3 boundary obligation"
 },
 {
  "id": "hostile/recipes/limit-entry-count-L6-one-over",
  "class": "hostile",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L6"
   ],
   "declaredLogicalSize": "entryCount == 65535 succeeds; 65536 rejected at manifest/index enumeration before per-entry processing",
   "boundaries": [
    {
     "quantity": "entryCountPerPackage",
     "row": "L6",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-entry-count",
  "clauses": [
   "FS §9.3 L6"
  ],
  "implementationDeferred": true,
  "notes": "at-limit success twin carried by the same expression; counts beyond the legacy ZIP ceiling also classified hostile structurally (FS §7.8 item 3)"
 },
 {
  "id": "hostile/recipes/limit-array-count-L17-boundary",
  "class": "hostile",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "bounded",
   "boundRows": [
    "L17"
   ],
   "declaredLogicalSize": "arrayLength == 65536 parses; 65537 rejected before element allocation",
   "boundaries": [
    {
     "quantity": "genericArrayElementCount",
     "row": "L17",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-array-count",
  "clauses": [
   "FS §9.3 L17"
  ],
  "implementationDeferred": true,
  "notes": "semantic caps on specific arrays are NOT ratified (FS §2.2 C4); this probes the generic parser ceiling only"
 },
 {
  "id": "hostile/recipes/limit-member-count-L18-boundary",
  "class": "hostile",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "bounded",
   "boundRows": [
    "L18"
   ],
   "declaredLogicalSize": "memberCount == 65536 parses; 65537 rejected by parser",
   "boundaries": [
    {
     "quantity": "objectMemberCountPerObject",
     "row": "L18",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-member-count",
  "clauses": [
   "FS §9.3 L18"
  ],
  "implementationDeferred": true,
  "notes": ""
 },
 {
  "id": "hostile/recipes/limit-entry-expanded-L3",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L3"
   ],
   "declaredLogicalSize": "perEntryExpanded == L3 succeeds; L3+1 rejected against declared-and-observed size before extraction proceeds past the cap",
   "boundaries": [
    {
     "quantity": "perEntryExpandedBytes",
     "row": "L3",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-entry-expanded",
  "clauses": [
   "FS 9.3 L3"
  ],
  "implementationDeferred": true,
  "notes": "clean bounded recipe carrying the pure L3 boundary after HIGH-1 re-categorization of the byte-level declared-size probe"
 },
 {
  "id": "hostile/recipes/limit-expansion-ratio-L15-clean",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L15"
   ],
   "declaredLogicalSize": "ratio expanded/physical > 1000 rejected incrementally with saturating arithmetic; exactly 1000 succeeds; inputs below the L15 small-input floor use the absolute expanded bound instead",
   "boundaries": [
    {
     "quantity": "perEntryExpansionRatio",
     "row": "L15",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-expansion-ratio",
  "clauses": [
   "FS 9.3 L15"
  ],
  "implementationDeferred": true,
  "notes": "clean bounded recipe carrying the pure L15 boundary; the genuine-stream under-limit twin plus its recomputed sidecar cover the success side"
 }
]
```

## Hostile — malformed JSON, numbers, schema-level classes

```json
[
 {
  "id": "hostile/json/malformed-truncated",
  "class": "hostile",
  "artifact": "hostile/json/malformed-truncated.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "FS §9.4 invalid encoding / malformed JSON row"
  ],
  "notes": "truncated document",
  "rawByDesign": true,
  "sha256": "46cbf3989123fdd5d8c780562dca2dbabbd71e45b0e8050567178ca65f9159d7"
 },
 {
  "id": "hostile/json/duplicate-object-keys",
  "class": "hostile",
  "artifact": "hostile/json/duplicate-object-keys.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "FS §9.4 duplicate object keys"
  ],
  "sha256": "c572f6bf9365e406acd54650c937391f9fc73408e7aafb6adde954275dfe2d7f",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "hostile/json/wrong-type-field",
  "class": "hostile",
  "artifact": "hostile/json/wrong-type-field.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "FS §9.4 wrong types"
  ],
  "sha256": "b77f23d017bd90acca9a9af3ed26210938c3fa83be3e039bce1c4fd7f4aed822",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "hostile/json/missing-required-field",
  "class": "hostile",
  "artifact": "hostile/json/missing-required-field.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "FS §9.4 missing required fields"
  ],
  "sha256": "39c510b57f63383c06db8e51affacdc024fb1b066184bf29a7a077bef729cce1",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "hostile/json/invalid-enum-value",
  "class": "hostile",
  "artifact": "hostile/json/invalid-enum-value.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "FS §9.4 invalid enums"
  ],
  "sha256": "9c18a3be5e3f68852b1b475b778e0ff1f637dd0e89be03f3874ddec603fbe274",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "hostile/json/uuid-uppercase-reject",
  "class": "hostile",
  "artifact": "hostile/json/uuid-uppercase-reject.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "FS §2.1 E1",
   "FS §9.3 L19"
  ],
  "notes": "canonical lowercase hyphenated form, exactly 36 characters",
  "sha256": "466ff314f33dbd280e07479e4e20610121ff79978dbe115fffaf8a73846e92f5",
  "assert": {
   "outcome": "reject"
  },
  "rowRefs": [
   "L19"
  ]
 },
 {
  "id": "hostile/json/uuid-short-form-reject",
  "class": "hostile",
  "artifact": "hostile/json/uuid-short-form-reject.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "FS §2.1 E1",
   "FS §9.3 L19"
  ],
  "sha256": "0485a9fd6b189b1223f7dd26497670b48fe86bf2824c5f4e35ed7e2e209aa0c8",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "hostile/json/uuid-braced-form-reject",
  "class": "hostile",
  "artifact": "hostile/json/uuid-braced-form-reject.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "FS §2.1 E1",
   "FS §9.3 L19"
  ],
  "sha256": "74561669104c67489bcf1d5681c56aae94286dd79aa79b6671ac1c0fb7833f39",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "hostile/json/digest-uppercase-hex-reject",
  "class": "hostile",
  "artifact": "hostile/json/digest-uppercase-hex-reject.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "AS §10.1",
   "FS §9.4 malformed digest syntax => hostile rejection"
  ],
  "notes": "hostile rejection, NOT missing-blob equivalence",
  "sha256": "7d2c73aea32c4b779acf93fb88377ca62d1447b9917c152a976cd9a83086922f",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "hostile/json/digest-wrong-length-reject",
  "class": "hostile",
  "artifact": "hostile/json/digest-wrong-length-reject.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "FS §9.3 L19 exact 71 characters",
   "AS §10.1"
  ],
  "sha256": "484a5667ab7527e0df445634731d96152f19bcf6582de43122a2634df3d873a4",
  "assert": {
   "outcome": "reject"
  },
  "rowRefs": [
   "L19"
  ]
 },
 {
  "id": "hostile/json/digest-unprefixed-reject",
  "class": "hostile",
  "artifact": "hostile/json/digest-unprefixed-reject.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "AS §10.1 unprefixed forms invalid"
  ],
  "sha256": "5472ccbf8b1fe457d794eb5822eeb20b2c2166e5bfd3e7dcb7ef48f3a50d708b",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "hostile/json/timestamp-non-rfc3339",
  "class": "hostile",
  "artifact": "hostile/json/timestamp-non-rfc3339-reject.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "FS §9.3 L19 RFC 3339 UTC fixed form"
  ],
  "sha256": "60d7a38688a83f3b41ef7517ca5b16b128ed7dab46f401861f02dc00c854e0ec",
  "assert": {
   "outcome": "reject"
  },
  "rowRefs": [
   "L19"
  ]
 },
 {
  "id": "hostile/json/nonfinite-nan-literal",
  "class": "hostile",
  "artifact": "hostile/json/nonfinite-nan-literal.json",
  "outcome": "reject",
  "category": "nonfinite-value",
  "clauses": [
   "FS §3",
   "FS §9.3 L13",
   "FS §9.4 NaN/infinity row"
  ],
  "sha256": "c30690dff795ed51ecaa0fc286f1f78823a02a80c63c03f7bf07f94511043b61",
  "rawByDesign": true,
  "rowRefs": [
   "L13"
  ]
 },
 {
  "id": "hostile/json/nonfinite-infinity-literal",
  "class": "hostile",
  "artifact": "hostile/json/nonfinite-infinity-literal.json",
  "outcome": "reject",
  "category": "nonfinite-value",
  "clauses": [
   "FS §3",
   "FS §9.3 L13"
  ],
  "sha256": "560edd92aa5520d11cb00564ad374ebd6095e29f49925debc455f82867806cc9",
  "rawByDesign": true,
  "rowRefs": [
   "L13"
  ]
 },
 {
  "id": "hostile/json/number-token-over-L12",
  "class": "hostile",
  "artifact": "hostile/json/number-token-over-L12.json",
  "outcome": "reject",
  "category": "limit-number-token",
  "clauses": [
   "FS §9.3 L12"
  ],
  "notes": "65-character literal (padding/exponent abuse class)",
  "sha256": "38a8260f166c633f894fc4d94e51ee338f9c82c540d4f3890d39d4ea39fed7d2",
  "assert": {
   "outcome": "reject"
  },
  "rowRefs": [
   "L12"
  ]
 },
 {
  "id": "hostile/json/integer-overflow-i64",
  "class": "hostile",
  "artifact": "hostile/json/integer-overflow-i64.json",
  "outcome": "reject",
  "category": "numeric-overflow",
  "clauses": [
   "FS §9.3 L13"
  ],
  "notes": "2^63 exceeds the signed 64-bit ceiling",
  "sha256": "7be687081db09eedf43b99709667fbc4acbe93a8f9296023a132b7b00ee9dc78",
  "assert": {
   "outcome": "reject"
  },
  "rowRefs": [
   "L13"
  ]
 },
 {
  "id": "hostile/json/nesting-depth-over-L11",
  "class": "hostile",
  "artifact": "hostile/json/nesting-depth-over-L11.json",
  "outcome": "reject",
  "category": "limit-nesting-depth",
  "clauses": [
   "FS §9.3 L11"
  ],
  "notes": "depth 129; rejection must occur before deep recursion",
  "rawByDesign": true,
  "sha256": "bf9ccb2c6b046c499dabdebed2f03905969c04d81b93b7f396977cad5d48cae7",
  "rowRefs": [
   "L11"
  ]
 },
 {
  "id": "hostile/json/negative-zero-writer-form",
  "class": "hostile",
  "artifact": "hostile/json/negative-zero-writer-form.json",
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §3.2 negative zero",
   "FS §9.4 negative-zero paragraph"
  ],
  "notes": "parsers ACCEPT -0 where IEEE-meaningful and validation maps -0 to +0; conforming writers emit 0; this fixture pins the reader contract while writer emission is pinned by the FS §3.2 canonicalization rule",
  "sha256": "ae2a600535d284255dccb8c0cfc15cc2f4ee4b6971bc52553ff66f68d0230e06",
  "assert": {
   "outcome": "success",
   "validationMapsNegativeZeroToPositiveZero": true
  }
 }
]
```

## Hostile — operation-batch framing (pre-replay rejects, FS §7.6 / §4.5)

```json
[
 {
  "id": "hostile/framing/name-content-binding-mismatch",
  "class": "hostile",
  "artifact": "hostile/framing/name-content-binding-mismatch.json",
  "outcome": "reject",
  "category": "malformed-framing",
  "clauses": [
   "FS §7.6 name/content binding",
   "FS §4.3 row 4"
  ],
  "notes": "entry name counterBase 43 vs embedded counterBase 44",
  "sha256": "e43e2e95630686e7c015ffee7ee2ae97e578e72ecf0db8ea842ac1de59044c5e",
  "assert": {
   "outcome": "reject",
   "entryNameCounterBase": 43,
   "embeddedCounterBase": 44
  }
 },
 {
  "id": "hostile/framing/duplicate-batch-ids",
  "class": "hostile",
  "recipe": {
   "kind": "batch-pair",
   "materialization": "bounded",
   "inputs": [
    {
     "path": "hostile/framing/duplicate-batch-id-a.json",
     "sha256": "316b2bd1b93d113ba2276729c0b376d434003e962055b34162774b0c7f27f745"
    },
    {
     "path": "hostile/framing/duplicate-batch-id-b.json",
     "sha256": "441f5b54b3d279837c2af5728ab767111f6b8536d6968f97daa8b9d46a454d71"
    }
   ]
  },
  "outcome": "reject",
  "category": "malformed-framing",
  "clauses": [
   "FS 7.6 structural log checks precede replay",
   "FS 4.3 row 4"
  ],
  "notes": "neither document alone is defective; the violation exists only across the pair (identical embedded batchId), verified by the checker parsing both inputs on every run. The two inputs differ in counterBase (50 and 51) so FS 7.6 naming yields two distinct legal entry names and their ranges stay contiguous, leaving the duplicate id as the only defect; the superseded inputs were byte-identical, which collided at duplicate logical names before the duplicate-id check could run and made the record look unmaterializable"
 },
 {
  "id": "hostile/framing/overlapping-reserved-ranges",
  "class": "hostile",
  "artifact": "hostile/framing/overlapping-reserved-ranges.json",
  "outcome": "reject",
  "category": "malformed-framing",
  "clauses": [
   "FS §4.1 B3 disjoint strictly-increasing ranges",
   "FS §7.6"
  ],
  "notes": "sub-batch a reserves [60,61] and b reserves [61,61]: the shared counter 61 is the only defect. The counterBases differ so FS 7.6 naming yields two distinct legal entry names, and both frontiers stop at 59 so neither batch claims observation of the other's interior counters (B5). The superseded fixture gave both sub-batches counterBase 60, so faithful naming produced one entry name and the duplicate-logical-name reject preceded range classification",
  "sha256": "13fe3eafab643c6c4dd6d07f1e3d5236f386125a5c882bcb27e32b6da4459ad9",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "hostile/framing/gapped-reserved-ranges",
  "class": "hostile",
  "artifact": "hostile/framing/gapped-reserved-ranges.json",
  "outcome": "reject",
  "category": "malformed-framing",
  "clauses": [
   "FS §4.1 B3 no gaps between successive batches",
   "FS §7.6"
  ],
  "sha256": "2cc8f42ac5ae08858e1ee6463ac8f05e6ce67386b67f76bba713e90883677ca1",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "hostile/framing/torn-batch-document",
  "class": "hostile",
  "artifact": "hostile/framing/torn-batch-document.json",
  "outcome": "reject",
  "category": "malformed-framing",
  "clauses": [
   "FS §7.6 torn or partial batch documents",
   "FS §4.3 row 4"
  ],
  "rawByDesign": true,
  "sha256": "5f6cd8e84805cb1a8654507e92925ef685cc7de86c9254f01f3fe9da29604d23"
 },
 {
  "id": "hostile/framing/empty-operations-array",
  "class": "hostile",
  "artifact": "hostile/framing/empty-operations-array.json",
  "outcome": "reject",
  "category": "malformed-framing",
  "clauses": [
   "FS §4.1 B3 empty batch is malformed-framing",
   "FS §4.3 row 4"
  ],
  "sha256": "06ae376fe78bc75d4f1e7daf80e162d593eef44ba21b65fd479a1fc710b06fd6",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "hostile/framing/inconsistent-frontier",
  "class": "hostile",
  "artifact": "hostile/framing/inconsistent-frontier.json",
  "outcome": "reject",
  "category": "malformed-framing",
  "clauses": [
   "FS §7.6 inconsistent frontiers",
   "FS §4.1 B5"
  ],
  "notes": "declaring actor frontier advances past its own reserved range end",
  "sha256": "b8e04d61ea296a412ceee7f5a95439a60e3b3a618a2d760bc2d107a4cdba0a0f",
  "assert": {
   "outcome": "reject"
  }
 },
 {
  "id": "hostile/framing/zero-frontier-member",
  "class": "hostile",
  "artifact": "hostile/framing/zero-frontier-member.json",
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "FS §4.4 F1 zero prohibition",
   "FS §9.5 StageSchema precedence"
  ],
  "notes": "Snapshot frontier carries zero-valued member {A:0}. F1 forbids zero; absence (actor omitted) is the sole representation of 'nothing observed'. Discrimination: wire.go:523 checks n<0 but not n==0; zero passes to snapshot.go:106 normalization. Fixture exposes gap between wire parser (accepts zero) and F1 prohibition (forbids zero). Expected diagnostic per issue 0005: invalid-field-syntax at StageSchema.",
  "sha256": "9cd8691c09983866d9f61eb8647263c0d83424575060bbf9523c9f283b7b4d7c",
  "assert": {
   "outcome": "reject",
   "category": "invalid-field-syntax"
  }
 },
 {
  "id": "hostile/recipes/limit-operation-count-L23",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L23"
   ],
   "declaredLogicalSize": "batchesPerPackage > 65535 OR operationsPerBatch > 65535 OR totalRetainedOperations > 1048576 rejected at framing enumeration before replay",
   "boundaries": [
    {
     "quantity": "batchesPerPackage",
     "row": "L23",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    },
    {
     "quantity": "operationsPerBatch",
     "row": "L23",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    },
    {
     "quantity": "totalRetainedOperations",
     "row": "L23",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-operation-count",
  "clauses": [
   "FS §9.3 L23"
  ],
  "implementationDeferred": true,
  "notes": "reader envelope distinct from logical semantic caps, which remain deferred to issue 0004"
 },
 {
  "id": "hostile/recipes/limit-snapshot-metadata-L23",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L23"
   ],
   "declaredLogicalSize": "snapshotMetadataBytes > 16 MiB rejected",
   "boundaries": [
    {
     "quantity": "snapshotMetadataBytes",
     "row": "L23",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-snapshot-metadata",
  "clauses": [
   "FS §9.3 L23",
   "FS §7.5"
  ],
  "implementationDeferred": true,
  "notes": ""
 }
]
```

## Declarative vectors — retrieval policy (FS §9.3 L24–L28; AS §10.8 structure)

No live network I/O exists in this corpus; each vector is an inert
configuration/hint validation expectation, implementation-deferred.

```json
[
 {
  "id": "declarative/retrieval/consent-gate-default-off",
  "class": "declarative",
  "recipe": {
   "kind": "policy-vector",
   "materialization": "none"
  },
  "outcome": "success",
  "category": null,
  "clauses": [
   "AS §10.8 consent gate"
  ],
  "implementationDeferred": true,
  "notes": "retrieval disabled by default; fetching without consent never occurs; offline summary per BE §9.6"
 },
 {
  "id": "declarative/retrieval/userinfo-url-parse-reject",
  "class": "declarative",
  "recipe": {
   "kind": "policy-vector",
   "materialization": "none"
  },
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "AS §10.8 credentials rule",
   "FS §9.4 invalid URI syntax"
  ],
  "implementationDeferred": true,
  "notes": "hint URL carrying userinfo parses as invalid; nothing credential-bearing transmitted"
 },
 {
  "id": "declarative/retrieval/scheme-not-in-allowlist",
  "class": "declarative",
  "recipe": {
   "kind": "policy-vector",
   "materialization": "none"
  },
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "AS §10.8 scheme allowlist + normalization"
  ],
  "implementationDeferred": true,
  "notes": "NFKC -> lowercase -> ASCII exact match; reject before dial"
 },
 {
  "id": "declarative/retrieval/ip-literal-host-reject",
  "class": "declarative",
  "recipe": {
   "kind": "policy-vector",
   "materialization": "none"
  },
  "outcome": "reject",
  "category": "invalid-field-syntax",
  "clauses": [
   "AS §10.8 hint host form"
  ],
  "implementationDeferred": true,
  "notes": "IP-literal hint hosts rejected outright in v0.1"
 },
 {
  "id": "declarative/retrieval/private-destination-reject",
  "class": "declarative",
  "recipe": {
   "kind": "policy-vector",
   "materialization": "none"
  },
  "outcome": "reject",
  "category": "invalid-reference",
  "clauses": [
   "AS §10.8 trusted destinations"
  ],
  "implementationDeferred": true,
  "notes": "loopback/RFC1918/link-local incl. 169.254.169.254 rejected unless user-configured; never document-supplied, never hint-inferred"
 },
 {
  "id": "declarative/retrieval/redirect-hop-bound-L26",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L26"
   ],
   "declaredLogicalSize": "hopCount == 4 succeeds; hop 5 rejected with full per-hop revalidation",
   "boundaries": [
    {
     "quantity": "redirectHops",
     "row": "L26",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-redirect-hops",
  "clauses": [
   "FS §9.3 L26",
   "AS §10.8 redirects rule"
  ],
  "implementationDeferred": true,
  "notes": "maps to missing-blob handling on abort"
 },
 {
  "id": "declarative/retrieval/transfer-byte-cap-L24",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L24",
    "L5"
   ],
   "declaredLogicalSize": "post-transfer-decoding bytes > L24 aborted mid-stream before decode/admission",
   "boundaries": [
    {
     "quantity": "retrievalTransferBytesPostDecoding",
     "row": "L24",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "missing-asset-equivalence",
  "category": "limit-transfer-bytes",
  "clauses": [
   "FS §9.3 L24"
  ],
  "implementationDeferred": true,
  "notes": "abort maps to missing-blob handling; >= L5 so local admission never fails where retrieval would"
 },
 {
  "id": "declarative/retrieval/time-caps-L25",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L25"
   ],
   "declaredLogicalSize": "connect > 10 s OR stalled > 15 s without progress OR total > 120 s aborts",
   "boundaries": [
    {
     "quantity": "retrievalSecondsConnectStalledTotal",
     "row": "L25",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "missing-asset-equivalence",
  "category": "limit-transfer-time",
  "clauses": [
   "FS §9.3 L25"
  ],
  "implementationDeferred": true,
  "notes": ""
 },
 {
  "id": "declarative/retrieval/hints-per-session-L27",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L27"
   ],
   "declaredLogicalSize": "attempt 65 within one document session skipped with summary diagnostic",
   "boundaries": [
    {
     "quantity": "hintsPerSessionAttempts",
     "row": "L27",
     "atLimitOutcome": "success",
     "oneOverOutcome": "warning"
    }
   ]
  },
  "outcome": "warning",
  "category": "limit-hints-session",
  "clauses": [
   "FS §9.3 L27"
  ],
  "implementationDeferred": true,
  "notes": "excess attempts are skipped with summary diagnostic, not errors"
 },
 {
  "id": "declarative/retrieval/concurrent-bound-L28",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L28"
   ],
   "declaredLogicalSize": ">4 concurrent requests queued then capped by L27",
   "boundaries": [
    {
     "quantity": "concurrentRetrievalRequests",
     "row": "L28",
     "atLimitOutcome": "success",
     "oneOverOutcome": "success"
    }
   ]
  },
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §9.3 L28"
  ],
  "implementationDeferred": true,
  "notes": "scheduler queuing behavior; no rejection of well-formed hints"
 },
 {
  "id": "declarative/retrieval/digest-before-decode",
  "class": "declarative",
  "recipe": {
   "kind": "policy-vector",
   "materialization": "none"
  },
  "outcome": "missing-asset-equivalence",
  "category": null,
  "clauses": [
   "AS §10.8 verification rule",
   "AS §10.1 transfer-decoding order"
  ],
  "implementationDeferred": true,
  "notes": "digest over transfer-decoded bytes BEFORE any decode and before cache admission; mismatch => missing handling; non-identity Content-Encoding rejected, not decoded"
 }
]
```

## Declarative vectors — cache GC pins/leases (AS §10.7)

```json
[
 {
  "id": "declarative/gc/lease-expiry-monotonic-same-boot",
  "class": "declarative",
  "recipe": {
   "kind": "pure-function-vector",
   "materialization": "none",
   "inputsDescription": "now_mono vs monoDeadline"
  },
  "outcome": "success",
  "category": null,
  "clauses": [
   "AS §10.7 expiry decision"
  ],
  "implementationDeferred": true,
  "notes": "collectable iff now_mono > monoDeadline (same boot); suspend does not advance CLOCK_MONOTONIC: lease life extends, the safe direction"
 },
 {
  "id": "declarative/gc/lease-expiry-utc-cross-boot",
  "class": "declarative",
  "recipe": {
   "kind": "pure-function-vector",
   "materialization": "none",
   "inputsDescription": "persisted UTC vs expiresAtUtc + M (M = 24 h default named quantity)"
  },
  "outcome": "success",
  "category": null,
  "clauses": [
   "AS §10.7 expiry decision"
  ],
  "implementationDeferred": true,
  "notes": "cross-boot collection requires persisted UTC past expiresAtUtc + M"
 },
 {
  "id": "declarative/gc/fail-closed-metadata-anomaly",
  "class": "declarative",
  "recipe": {
   "kind": "policy-vector",
   "materialization": "none"
  },
  "outcome": "safe-write-refusal",
  "category": null,
  "clauses": [
   "AS §10.7 fail-closed rule",
   "AS §10.7 G4"
  ],
  "implementationDeferred": true,
  "notes": "checksum failure, generation gap/regression, truncation, unparsable record => zero collection until validated rebuild; retained-until-repair for unreadable documents' pinned digests"
 },
 {
  "id": "declarative/gc/pin-never-collected",
  "class": "declarative",
  "recipe": {
   "kind": "policy-vector",
   "materialization": "none"
  },
  "outcome": "success",
  "category": null,
  "clauses": [
   "AS §10.7 G1–G4"
  ],
  "implementationDeferred": true,
  "notes": "collection never removes blobs referenced by retained snapshots/operations; crashes yield over-retention only"
 }
]
```

## Declarative vectors — decoder admission limits (FS §9.3 L20/L21/L22)

```json
[
 {
  "id": "declarative/decoder/image-dimensions-over-L20",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L20"
   ],
   "declaredLogicalSize": "width > 16384 OR height > 16384 OR width x height > 268435456 rejected on declared headers BEFORE decode allocation",
   "boundaries": [
    {
     "quantity": "imagePixelsWidthHeightAndArea",
     "row": "L20",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-image-dimensions",
  "clauses": [
   "FS §9.3 L20"
  ],
  "implementationDeferred": true,
  "notes": "no codec fixtures in this corpus; declared-header shape only"
 },
 {
  "id": "declarative/decoder/model-complexity-over-L21",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L21"
   ],
   "declaredLogicalSize": "vertices > 10000000 OR primitiveInstances > 10000000 OR nodes/meshes > 65536 OR texturesPerModel > 64 OR decodedBytes > L3 rejected at metadata pre-scan",
   "boundaries": [
    {
     "quantity": "modelComplexityVerticesPrimitivesNodesTexturesBytes",
     "row": "L21",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-model-complexity",
  "clauses": [
   "FS §9.3 L21"
  ],
  "implementationDeferred": true,
  "notes": "codec-independent admission caps"
 },
 {
  "id": "declarative/decoder/chunk-count-over-L22",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L22"
   ],
   "declaredLogicalSize": "chunksPerBlob > 65536 rejected during streaming enumeration",
   "boundaries": [
    {
     "quantity": "chunksPerBlob",
     "row": "L22",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-chunk-count",
  "clauses": [
   "FS §9.3 L22"
  ],
  "implementationDeferred": true,
  "notes": "generic guard for future chunked classes; splats excluded from v0.1 entirely"
 },
 {
  "id": "declarative/decoder/nested-archive-over-L10",
  "class": "declarative",
  "recipe": {
   "kind": "limit-probe",
   "materialization": "none",
   "boundRows": [
    "L10"
   ],
   "declaredLogicalSize": "container nesting depth > 1: interpreting an embedded archive-class payload as an archive during the streaming structural scan rejects",
   "boundaries": [
    {
     "quantity": "containerNestingDepth",
     "row": "L10",
     "atLimitOutcome": "success",
     "oneOverOutcome": "reject"
    }
   ]
  },
  "outcome": "reject",
  "category": "limit-archive-depth",
  "clauses": [
   "FS 9.3 L10",
   "FS 7.1 C8 size-class separation"
  ],
  "implementationDeferred": true,
  "notes": "bounded declarative vector: the package IS depth 1; any embedded archive-class payload is an opaque asset and interpreting it as an archive rejects before recursion. No real nested-archive bytes are committed because none are needed to pin the expectation"
 }
]
```

## Declarative vectors — save/recovery observability (FS §9.7; BE §9.5 F-matrix)

Fixture duty for F1–F9 belongs to issue 0028 but requires the issue 0029
persistence harness; until then these are declarative expected-state records.

```json
[
 {
  "id": "declarative/recovery/f1-first-save-failure",
  "class": "declarative",
  "recipe": {
   "kind": "fault-vector",
   "materialization": "none",
   "injectionPoint": "F1"
  },
  "outcome": "degraded-read-only-inspection",
  "category": null,
  "clauses": [
   "FS §9.7 fault matrix F1",
   "BE §9.5"
  ],
  "implementationDeferred": true,
  "notes": "RS3 leftover candidate ignorable/cleanable; target untouched",
  "diagnostic": "state-invalid-candidate"
 },
 {
  "id": "declarative/recovery/f2-candidate-write-interrupted",
  "class": "declarative",
  "recipe": {
   "kind": "fault-vector",
   "materialization": "none",
   "injectionPoint": "F2"
  },
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §9.7 F2",
   "BE §9.5"
  ],
  "implementationDeferred": true,
  "notes": "RS1 + RS3: previous package readable; leftovers cleanable",
  "diagnostic": "state-valid-target"
 },
 {
  "id": "declarative/recovery/f3-candidate-validation-fails",
  "class": "declarative",
  "recipe": {
   "kind": "fault-vector",
   "materialization": "none",
   "injectionPoint": "F3"
  },
  "outcome": "reject",
  "category": null,
  "clauses": [
   "FS §9.7 F3 S2 abort path"
  ],
  "implementationDeferred": true,
  "notes": "save aborts actionable; target untouched"
 },
 {
  "id": "declarative/recovery/f4-content-sync-fails",
  "class": "declarative",
  "recipe": {
   "kind": "fault-vector",
   "materialization": "none",
   "injectionPoint": "F4"
  },
  "outcome": "reject",
  "category": null,
  "clauses": [
   "FS §9.7 F4 S3 abort path"
  ],
  "implementationDeferred": true,
  "notes": ""
 },
 {
  "id": "declarative/recovery/f5-replace-interrupted",
  "class": "declarative",
  "recipe": {
   "kind": "fault-vector",
   "materialization": "none",
   "injectionPoint": "F5"
  },
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §9.7 F5 EITHER guarantee"
  ],
  "implementationDeferred": true,
  "notes": "old-or-new readable under rename atomicity; no power-loss claim off-profile",
  "diagnostic": "state-valid-target"
 },
 {
  "id": "declarative/recovery/f6-directory-sync-failure",
  "class": "declarative",
  "recipe": {
   "kind": "fault-vector",
   "materialization": "none",
   "injectionPoint": "F6"
  },
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §9.7 F6 S5 caveat path"
  ],
  "implementationDeferred": true,
  "notes": "new package readable; durability caveated; no rollback",
  "diagnostic": "state-valid-target"
 },
 {
  "id": "declarative/recovery/f7-artifact-write-failure",
  "class": "declarative",
  "recipe": {
   "kind": "fault-vector",
   "materialization": "none",
   "injectionPoint": "F7"
  },
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §9.7 F7 RS6 exclusion"
  ],
  "implementationDeferred": true,
  "notes": "artifact discarded/marked invalid; target unaffected",
  "diagnostic": "state-invalid-candidate"
 },
 {
  "id": "declarative/recovery/f8-cleanup-interrupted",
  "class": "declarative",
  "recipe": {
   "kind": "fault-vector",
   "materialization": "none",
   "injectionPoint": "F8"
  },
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §9.7 F8"
  ],
  "implementationDeferred": true,
  "notes": "artifacts valid-or-excluded; never partially promoted"
 },
 {
  "id": "declarative/recovery/f9-autosave-failure",
  "class": "declarative",
  "recipe": {
   "kind": "fault-vector",
   "materialization": "none",
   "injectionPoint": "F9"
  },
  "outcome": "warning",
  "category": null,
  "clauses": [
   "FS §9.7 F9 autosave path"
  ],
  "implementationDeferred": true,
  "notes": "non-blocking notification; retry next trigger; target untouched"
 },
 {
  "id": "declarative/recovery/rs1-rs8-state-tokens",
  "class": "declarative",
  "recipe": {
   "kind": "fault-vector",
   "materialization": "none",
   "states": "RS1 state-valid-target; RS2 indistinguishable-from-RS1; RS3 state-invalid-candidate; RS4 state-stale-candidate; RS5 state-corrupt-target; RS6 state-invalid-candidate; RS7 state-no-recovery; RS8 state-ambiguous-recovery"
  },
  "outcome": "success",
  "category": null,
  "clauses": [
   "FS §9.7 recovery-state table",
   "BE §9.3/§9.4 ordering authority"
  ],
  "implementationDeferred": true,
  "notes": "machine-readable categories bound every startup combination; auto-promotion of stale/invalid artifacts prohibited; ambiguity requires explicit user choice"
 }
]
```

## Cross-layer precedence collisions (FS §9.5)

Outermost failing stage wins; diagnostics name that stage.

```json
[
 {
  "id": "declarative/precedence/physical-before-schema",
  "class": "declarative",
  "recipe": {
   "kind": "precedence-vector",
   "materialization": "none",
   "layers": [
    "checksum-error",
    "invalid-field-syntax"
   ]
  },
  "outcome": "reject",
  "category": "checksum-error",
  "clauses": [
   "FS §9.5 malformed physical bytes rejected BEFORE schema parsing"
  ],
  "implementationDeferred": true,
  "notes": "a CRC-corrupted entry wrapping also-invalid JSON must surface checksum-error"
 },
 {
  "id": "declarative/precedence/schema-limits-before-decode",
  "class": "declarative",
  "recipe": {
   "kind": "precedence-vector",
   "materialization": "none",
   "layers": [
    "limit-image-dimensions",
    "undecodable-blob"
   ]
  },
  "outcome": "reject",
  "category": "limit-image-dimensions",
  "clauses": [
   "FS §9.5 schema and limit violations before decoding"
  ],
  "implementationDeferred": true,
  "notes": ""
 },
 {
  "id": "declarative/precedence/digest-before-decode-and-cache",
  "class": "declarative",
  "recipe": {
   "kind": "precedence-vector",
   "materialization": "none",
   "layers": [
    "missing-asset-equivalence",
    "limit-model-complexity"
   ]
  },
  "outcome": "missing-asset-equivalence",
  "category": null,
  "clauses": [
   "FS §9.5 digest verification before decode and cache admission",
   "AS §10.8"
  ],
  "implementationDeferred": true,
  "notes": ""
 },
 {
  "id": "declarative/precedence/trailing-vs-checksum",
  "class": "declarative",
  "recipe": {
   "kind": "precedence-vector",
   "materialization": "none",
   "layers": [
    "trailing-data",
    "checksum-error"
   ]
  },
  "outcome": "reject",
  "category": "trailing-data",
  "clauses": [
   "FS §9.4 T1 fail-closed rule",
   "FS §9.5"
  ],
  "implementationDeferred": true,
  "notes": "T1 ambiguity rejects the entire package regardless of inner integrity"
 }
]
```

## Known coverage gaps (machine-readable)

```json
[
 {
  "id": "gaps/v4-positive-migration-path",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "FS §8.2 V4"
  ],
  "implementationDeferred": true,
  "notes": "duplicate pointer to migration/v4-tested-migration-path: no predecessor schema exists"
 },
 {
  "id": "gaps/provisional-parameter-bound-probes",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "FS §3.1",
   "FS §9.3 L14 deferred to issues 0022/0030"
  ],
  "implementationDeferred": true,
  "notes": "coordinate-out-of-range / dimension-out-of-range probes cannot be authored as bytes or expected bounds while parameter values are unratified (ED-0019); see parameters.json — symbolic references only. Unblocks at issue 0030 ratification."
 },
 {
  "id": "gaps/section-4-7-vector-audit-dimension",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "FS §4.7",
   "FS §9.6"
  ],
  "implementationDeferred": true,
  "notes": "NO AUDIT DIMENSION COUNTS ANY §4.7 VECTOR, so none of the eight has ever been counted. categoryAudit maps §9.4 REJECTION tokens and every vector record is outcome=success with category null; rowAudit maps §9.3 rows, which the vector records do not carry. This is a gate-level blind spot, not a fixture-level one: the vector ASSERTIONS are not blocked and no specification question is open — V1 through V7 execute and assert for real, V2 included now that issue 0011 has removed its marker. What is missing is the dimension that would let the coverage audit STATE that fact. Found by issue 0011 while discharging ED-0036's replacement condition, which asked for a category and coveredBy entry that could not exist; the dimension is routed to issue 0028 by ED-0045 and deliberately NOT built by the pair whose semantics it would certify"
 },
 {
  "id": "gaps/seeded-replay-accepts-what-full-replay-rejects",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "FS §4.7 V6",
   "FS §5.1 S4",
   "FS §4.3 row 1"
  ],
  "implementationDeferred": true,
  "notes": "KNOWN V6 NONCONFORMANCE (ED-0049), not a documented limitation. A late batch LIVE at its write position (I5 branch 2) whose target is already tombstoned in the seeded replay root has that one write superseded by the later delete (LWW3) and materializes nothing, while the batch still applies and its writes to other targets land. FULL replay reaches the same write with the entity PRESENT and runs payload schema validation, so a schema-invalid setFields rejects the whole batch under §4.3 row 1 with zero effects; SEEDED replay has no entity to validate against, because a tombstoned entity is not in S1's live set and S4 retains no entity KIND. THE DIRECTION IS THE SECURITY-RELEVANT PART: seeded is MORE PERMISSIVE than full, so a compacted package accepts a batch an uncompacted replica rejects — compaction weakens validation. It is unreachable by a CONFORMING writer because O4 validates at authoring time, and that is EXPLICITLY REJECTED as a resolution: a hostile or non-conforming author is the threat model this corpus exists for. The remedy ED-0037 comment (q) already named — payload/schema support bounded at worst at entity kind or schema per tombstone — is a §5.1 S4 member addition on ED-0040's standing queue; issue 0011 may not invent a snapshot member and did not. The hostile fixture exhibiting the divergence is routed to issue 0028"
 },
 {
  "id": "gaps/l30-recovery-candidate-file-count",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "FS §9.3 L30 deferred to issue 0029"
  ],
  "implementationDeferred": true,
  "notes": "owner-deferred quantity; artifact naming/mechanism intentionally unspecified"
 },
 {
  "id": "gaps/l31-retained-package-size-writer-thresholds",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "FS §9.3 L31 deferred to issues 0005/0029"
  ],
  "implementationDeferred": true,
  "notes": "writer policy, never a reader-validated constraint"
 },
 {
  "id": "gaps/physical-ordering-row",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "FS §7.2 physical entry ordering writer obligation",
   "FS §9.4 owner-defined row"
  ],
  "implementationDeferred": true,
  "notes": "readers never depend on physical order; no reader-side rejection class arises until issue 0005 makes behavior normative"
 },
 {
  "id": "gaps/preview-render-fixtures",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "AS §10.5 preview records",
   "AS §10.9 reference encoder evidence vector only"
  ],
  "implementationDeferred": true,
  "notes": "rendered thumbnails are measurement-time derived output per AS §10.9; corpus pins shapes, not rendered pixels"
 },
 {
  "id": "gaps/l29-duplicate-physical-occurrence-semantics",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "FS 9.3 L29",
   "FS 7.3 duplicate logical names always reject",
   "AS 10.3 injective mapping"
  ],
  "implementationDeferred": true,
  "notes": "UPSTREAM SPEC BLOCKER: tolerance of repeated physical occurrences of identical blobs appears unreachable because two occurrences of one digest share one derived logical path and duplicate logical names always reject; raised to spec owners rather than claimed as coverage"
 },
 {
  "id": "gaps/l33-reader-arithmetic-obligation",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "FS 9.3 L33"
  ],
  "implementationDeferred": true,
  "notes": "ROUTED TO ISSUE 0011: L33 is DERIVED from L18 and L13 (maximum causalPastSize = 2^79 - 2^16). Reader representation obligation: represent and compare every value in [0, 2^79 - 2^16] exactly. Per FS 9.3:1631-1632, no corpus fixture can violate L33 by construction (L18 or L13 rejects first). Obligation discharged by reader arithmetic test in issue 0011, not corpus fixture."
 },
 {
  "id": "gaps/partial-history-package-conformance",
  "class": "declarative",
  "recipe": {
   "kind": "blocked-obligation",
   "materialization": "none"
  },
  "outcome": "blocked-open-question",
  "category": null,
  "clauses": [
   "FS §7.5 snapshot placement",
   "FS §4.9 batch structure"
  ],
  "implementationDeferred": true,
  "notes": "ROUTED TO ISSUE 0004 or specification owners: Does FORMAT-SPEC permit partial-history packages — packages where snapshot.frontier={} (empty, per FS 7.5 brand-new document) while batch claims prior operations exist (counterBase > 1, batch.frontier non-empty)? Three success-outcome packages exhibit this shape: golden/packages/thin-valid, golden/packages/portable-valid, and hostile/container/ratio-under-L15-success-twin. All are structurally valid and internally consistent, but their success outcomes depend on whether partial-history packages are conforming. This is NOT the zero-valued frontier question issue 0004 ruled on (explicit {A:0} is forbidden); empty {} is absence and remains legal. Partial-history conformance is a separate, unruled question."
 }
]
```

## Taxonomy and limit-row coverage audit

Machine-checked mapping of every ratified FS 9.4 category token and every FS 9.3
row to its triggering records. Non-covered entries carry a gapRecord and count
AGAINST full taxonomy coverage: the corpus does NOT claim complete taxonomy
coverage while any blocked entry exists.

```json
{
 "categoryAudit": [
  {
   "token": "ambiguous-manifest",
   "status": "blocked-upstream-spec",
   "coveredBy": [],
   "gapRecord": "gaps/ambiguous-manifest-deterministic-trigger"
  },
  {
   "token": "checksum-error",
   "status": "covered",
   "coveredBy": [
    "hostile/container/checksum-error",
    "declarative/precedence/physical-before-schema"
   ]
  },
  {
   "token": "duplicate-logical-name",
   "status": "covered",
   "coveredBy": [
    "hostile/container/duplicate-logical-name",
    "hostile/container/name-casefold-collision",
    "hostile/container/ambiguous-manifest"
   ]
  },
  {
   "token": "encrypted-entry",
   "status": "covered",
   "coveredBy": [
    "hostile/container/encrypted-entry"
   ]
  },
  {
   "token": "invalid-entry-name",
   "status": "covered",
   "coveredBy": [
    "migration/foreign-namespace-entry",
    "hostile/container/utf8-flag-clear",
    "hostile/container/directory-entry-attr",
    "hostile/container/symlink-designation",
    "hostile/container/nul-byte-name"
   ]
  },
  {
   "token": "invalid-field-syntax",
   "status": "covered",
   "coveredBy": [
    "hostile/json/malformed-truncated",
    "hostile/json/duplicate-object-keys",
    "hostile/json/wrong-type-field",
    "hostile/json/missing-required-field",
    "hostile/json/invalid-enum-value",
    "hostile/json/uuid-uppercase-reject",
    "hostile/json/uuid-short-form-reject",
    "hostile/json/uuid-braced-form-reject",
    "hostile/json/digest-uppercase-hex-reject",
    "hostile/json/digest-wrong-length-reject",
    "hostile/json/digest-unprefixed-reject",
    "hostile/json/timestamp-non-rfc3339",
    "hostile/framing/zero-frontier-member",
    "declarative/retrieval/userinfo-url-parse-reject",
    "declarative/retrieval/scheme-not-in-allowlist",
    "declarative/retrieval/ip-literal-host-reject"
   ]
  },
  {
   "token": "invalid-reference",
   "status": "covered",
   "coveredBy": [
    "declarative/retrieval/private-destination-reject"
   ]
  },
  {
   "token": "limit-aggregate-expansion",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-aggregate-expansion-L16"
   ]
  },
  {
   "token": "limit-archive-depth",
   "status": "covered",
   "coveredBy": [
    "declarative/decoder/nested-archive-over-L10"
   ]
  },
  {
   "token": "limit-array-count",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-array-count-L17-boundary"
   ]
  },
  {
   "token": "limit-blob-bytes",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-blob-bytes-L5"
   ]
  },
  {
   "token": "limit-chunk-count",
   "status": "covered",
   "coveredBy": [
    "declarative/decoder/chunk-count-over-L22"
   ]
  },
  {
   "token": "limit-entry-count",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-entry-count-L6-one-over"
   ]
  },
  {
   "token": "limit-entry-expanded",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-entry-expanded-L3"
   ]
  },
  {
   "token": "limit-entry-physical",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-entry-physical-L4"
   ]
  },
  {
   "token": "limit-expansion-ratio",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-expansion-ratio-L15-clean"
   ]
  },
  {
   "token": "limit-hints-session",
   "status": "covered",
   "coveredBy": [
    "declarative/retrieval/hints-per-session-L27"
   ]
  },
  {
   "token": "limit-image-dimensions",
   "status": "covered",
   "coveredBy": [
    "declarative/decoder/image-dimensions-over-L20",
    "declarative/precedence/schema-limits-before-decode"
   ]
  },
  {
   "token": "limit-manifest-reserve",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-manifest-reserve-L7"
   ]
  },
  {
   "token": "limit-member-count",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-member-count-L18-boundary"
   ]
  },
  {
   "token": "limit-model-complexity",
   "status": "covered",
   "coveredBy": [
    "declarative/decoder/model-complexity-over-L21"
   ]
  },
  {
   "token": "limit-name-component",
   "status": "covered",
   "coveredBy": [
    "hostile/container/overlong-component-L9"
   ]
  },
  {
   "token": "limit-name-length",
   "status": "covered",
   "coveredBy": [
    "hostile/container/overlong-name-L8"
   ]
  },
  {
   "token": "limit-nesting-depth",
   "status": "covered",
   "coveredBy": [
    "hostile/json/nesting-depth-over-L11"
   ]
  },
  {
   "token": "limit-number-token",
   "status": "covered",
   "coveredBy": [
    "hostile/json/number-token-over-L12"
   ]
  },
  {
   "token": "limit-operation-count",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-operation-count-L23"
   ]
  },
  {
   "token": "limit-redirect-hops",
   "status": "covered",
   "coveredBy": [
    "declarative/retrieval/redirect-hop-bound-L26"
   ]
  },
  {
   "token": "limit-snapshot-metadata",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-snapshot-metadata-L23"
   ]
  },
  {
   "token": "limit-total-expanded",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-total-expanded-L1"
   ]
  },
  {
   "token": "limit-total-physical",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-total-physical-L2"
   ]
  },
  {
   "token": "limit-transfer-bytes",
   "status": "covered",
   "coveredBy": [
    "declarative/retrieval/transfer-byte-cap-L24"
   ]
  },
  {
   "token": "limit-transfer-time",
   "status": "covered",
   "coveredBy": [
    "declarative/retrieval/time-caps-L25"
   ]
  },
  {
   "token": "malformed-container",
   "status": "covered",
   "coveredBy": [
    "hostile/container/empty-file",
    "hostile/container/bad-signature",
    "hostile/container/truncated-header",
    "hostile/container/truncated-eocd",
    "hostile/container/spanned-disk-number",
    "hostile/container/zip64-malformed-locator",
    "hostile/container/zip64-truncated-eocd64"
   ]
  },
  {
   "token": "malformed-framing",
   "status": "covered",
   "coveredBy": [
    "hostile/framing/name-content-binding-mismatch",
    "hostile/framing/duplicate-batch-ids",
    "hostile/framing/overlapping-reserved-ranges",
    "hostile/framing/gapped-reserved-ranges",
    "hostile/framing/torn-batch-document",
    "hostile/framing/empty-operations-array",
    "hostile/framing/inconsistent-frontier"
   ]
  },
  {
   "token": "malformed-length",
   "status": "covered",
   "coveredBy": [
    "hostile/container/malformed-length",
    "hostile/container/limit-expansion-ratio-over-L15",
    "hostile/container/declared-size-lie-limit-L3"
   ]
  },
  {
   "token": "missing-manifest",
   "status": "covered",
   "coveredBy": [
    "hostile/container/missing-manifest"
   ]
  },
  {
   "token": "nonfinite-value",
   "status": "covered",
   "coveredBy": [
    "hostile/json/nonfinite-nan-literal",
    "hostile/json/nonfinite-infinity-literal"
   ]
  },
  {
   "token": "numeric-overflow",
   "status": "covered",
   "coveredBy": [
    "hostile/json/integer-overflow-i64"
   ]
  },
  {
   "token": "overlapping-regions",
   "status": "covered",
   "coveredBy": [
    "hostile/container/overlapping-regions"
   ]
  },
  {
   "token": "path-traversal",
   "status": "covered",
   "coveredBy": [
    "hostile/container/traversal-dotdot",
    "hostile/container/absolute-path",
    "hostile/container/drive-form",
    "hostile/container/backslash-name",
    "hostile/container/dot-segment-name"
   ]
  },
  {
   "token": "trailing-data",
   "status": "covered",
   "coveredBy": [
    "hostile/container/trailing-data",
    "hostile/container/concatenated-packages",
    "declarative/precedence/trailing-vs-checksum"
   ]
  },
  {
   "token": "unsupported-extension",
   "status": "covered",
   "coveredBy": [
    "hostile/container/nonzero-comment",
    "hostile/container/nonzero-archive-comment",
    "hostile/container/foreign-extra-field"
   ]
  },
  {
   "token": "unsupported-method",
   "status": "covered",
   "coveredBy": [
    "hostile/container/unsupported-method-required-scope"
   ]
  },
  {
   "token": "unsupported-version",
   "status": "covered",
   "coveredBy": [
    "adversarial/unknown-data/unknown-required-refuse",
    "migration/newer-minor-reject",
    "migration/older-minor-unknown-label",
    "hostile/container/version-needed-over"
   ]
  },
  {
   "token": "limit-concurrent-retrieval",
   "status": "no-reject-trigger-by-design",
   "coveredBy": [
    "declarative/retrieval/concurrent-bound-L28"
   ],
   "note": "scheduler queuing never rejects well-formed hints"
  }
 ],
 "rowAudit": [
  {
   "row": "L1",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-total-expanded-L1"
   ]
  },
  {
   "row": "L2",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-total-physical-L2"
   ]
  },
  {
   "row": "L3",
   "status": "covered",
   "coveredBy": [
    "hostile/container/declared-size-lie-limit-L3",
    "hostile/recipes/limit-entry-expanded-L3"
   ],
   "note": "at-limit success twins carried inside coveredBy records"
  },
  {
   "row": "L4",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-entry-physical-L4"
   ]
  },
  {
   "row": "L5",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-blob-bytes-L5"
   ]
  },
  {
   "row": "L6",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-entry-count-L6-one-over"
   ]
  },
  {
   "row": "L7",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-manifest-reserve-L7"
   ]
  },
  {
   "row": "L8",
   "status": "covered",
   "coveredBy": [
    "hostile/container/overlong-name-L8"
   ]
  },
  {
   "row": "L9",
   "status": "covered",
   "coveredBy": [
    "hostile/container/overlong-component-L9"
   ]
  },
  {
   "row": "L10",
   "status": "covered",
   "coveredBy": [
    "declarative/decoder/nested-archive-over-L10"
   ]
  },
  {
   "row": "L11",
   "status": "covered",
   "coveredBy": [
    "golden/parser-twins/nesting-at-L11",
    "hostile/json/nesting-depth-over-L11"
   ]
  },
  {
   "row": "L12",
   "status": "covered",
   "coveredBy": [
    "golden/parser-twins/number-token-at-L12",
    "hostile/json/number-token-over-L12"
   ]
  },
  {
   "row": "L13",
   "status": "covered",
   "coveredBy": [
    "golden/parser-twins/integer-max-i64",
    "hostile/json/integer-overflow-i64",
    "hostile/json/nonfinite-nan-literal",
    "hostile/json/nonfinite-infinity-literal"
   ],
   "note": "at-limit success twins carried inside coveredBy records"
  },
  {
   "row": "L14",
   "status": "blocked-owner-deferred",
   "coveredBy": [],
   "gapRecord": "gaps/provisional-parameter-bound-probes"
  },
  {
   "row": "L15",
   "status": "covered",
   "coveredBy": [
    "hostile/container/ratio-under-L15-success-twin",
    "hostile/recipes/limit-expansion-ratio-L15-clean"
   ]
  },
  {
   "row": "L16",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-aggregate-expansion-L16"
   ]
  },
  {
   "row": "L17",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-array-count-L17-boundary"
   ]
  },
  {
   "row": "L18",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-member-count-L18-boundary"
   ]
  },
  {
   "row": "L19",
   "status": "covered",
   "coveredBy": [
    "hostile/json/uuid-uppercase-reject",
    "hostile/json/digest-wrong-length-reject",
    "hostile/json/timestamp-non-rfc3339"
   ]
  },
  {
   "row": "L20",
   "status": "covered",
   "coveredBy": [
    "declarative/decoder/image-dimensions-over-L20"
   ]
  },
  {
   "row": "L21",
   "status": "covered",
   "coveredBy": [
    "declarative/decoder/model-complexity-over-L21"
   ]
  },
  {
   "row": "L22",
   "status": "covered",
   "coveredBy": [
    "declarative/decoder/chunk-count-over-L22"
   ]
  },
  {
   "row": "L23",
   "status": "covered",
   "coveredBy": [
    "hostile/recipes/limit-operation-count-L23",
    "hostile/recipes/limit-snapshot-metadata-L23"
   ],
   "note": "at-limit success twins carried inside coveredBy records"
  },
  {
   "row": "L24",
   "status": "covered",
   "coveredBy": [
    "declarative/retrieval/transfer-byte-cap-L24"
   ]
  },
  {
   "row": "L25",
   "status": "covered",
   "coveredBy": [
    "declarative/retrieval/time-caps-L25"
   ]
  },
  {
   "row": "L26",
   "status": "covered",
   "coveredBy": [
    "declarative/retrieval/redirect-hop-bound-L26"
   ]
  },
  {
   "row": "L27",
   "status": "covered",
   "coveredBy": [
    "declarative/retrieval/hints-per-session-L27"
   ]
  },
  {
   "row": "L28",
   "status": "covered",
   "coveredBy": [
    "declarative/retrieval/concurrent-bound-L28"
   ]
  },
  {
   "row": "L29",
   "status": "blocked-upstream-spec",
   "coveredBy": [],
   "gapRecord": "gaps/l29-duplicate-physical-occurrence-semantics",
   "note": "tolerance rule appears unreachable given injective mapping plus duplicate-logical-name rejection; see gapRecord"
  },
  {
   "row": "L30",
   "status": "blocked-owner-deferred",
   "coveredBy": [],
   "gapRecord": "gaps/l30-recovery-candidate-file-count"
  },
  {
   "row": "L31",
   "status": "blocked-owner-deferred",
   "coveredBy": [],
   "gapRecord": "gaps/l31-retained-package-size-writer-thresholds"
  },
  {
   "row": "L32",
   "status": "covered",
   "coveredBy": [
    "adversarial/unknown-data/unknown-optional-preserve",
    "adversarial/unknown-data/write-refusal-unpreservable"
   ],
   "note": "write-back-path obligation; preservation volume bounded within L1"
  },
  {
   "row": "L33",
   "status": "blocked-upstream-spec",
   "coveredBy": [],
   "gapRecord": "gaps/l33-reader-arithmetic-obligation",
   "note": "DERIVED from L18 and L13; maximum 2^79 - 2^16. Reader representation obligation not corpus-coverable (FS 9.3:1631-1632: L18/L13 reject before L33 bound reached). Routed to issue 0011 for reader arithmetic test."
  }
 ],
 "residualBlockers": [
  {
   "record": "gaps/section-4-7-vector-audit-dimension",
   "state": "blocked-open-question",
   "blockedAssertions": [],
   "routedTo": "issue 0028 (ED-0045)"
  },
  {
   "record": "gaps/seeded-replay-accepts-what-full-replay-rejects",
   "state": "blocked-open-question",
   "blockedAssertions": [],
   "routedTo": "issue 0004 for the FS 5.1 S4 member, issue 0005 for container wording, issue 0028 for the hostile fixture (ED-0049)"
  },
  {
   "record": "gaps/ambiguous-manifest-deterministic-trigger",
   "state": "blocked-open-question",
   "blockedAssertions": [],
   "routedTo": "FS §7.3/§7.4/§9.4 owners"
  },
  {
   "record": "gaps/l29-duplicate-physical-occurrence-semantics",
   "state": "blocked-open-question",
   "blockedAssertions": [],
   "routedTo": "FS §7.x owners"
  },
  {
   "record": "gaps/v4-positive-migration-path",
   "state": "blocked-open-question",
   "blockedAssertions": [],
   "routedTo": "issue 0031"
  },
  {
   "record": "migration/v4-tested-migration-path",
   "state": "blocked-open-question",
   "blockedAssertions": [],
   "routedTo": "issue 0031"
  },
  {
   "record": "gaps/provisional-parameter-bound-probes",
   "state": "blocked-open-question",
   "blockedAssertions": [],
   "routedTo": "issue 0030"
  },
  {
   "record": "gaps/l30-recovery-candidate-file-count",
   "state": "blocked-open-question",
   "blockedAssertions": [],
   "routedTo": "issue 0029 writer policy"
  },
  {
   "record": "gaps/l31-retained-package-size-writer-thresholds",
   "state": "blocked-open-question",
   "blockedAssertions": [],
   "routedTo": "issue 0029 writer policy"
  },
  {
   "record": "gaps/physical-ordering-row",
   "state": "blocked-open-question",
   "blockedAssertions": [],
   "routedTo": "issue 0005"
  },
   {
    "record": "gaps/preview-render-fixtures",
    "state": "blocked-open-question",
    "blockedAssertions": [],
    "routedTo": "issue 0026"
   },
   {
    "record": "gaps/l33-reader-arithmetic-obligation",
    "state": "blocked-open-question",
    "blockedAssertions": [],
    "routedTo": "issue 0011 causalPastSize reader arithmetic"
   },
   {
    "record": "gaps/partial-history-package-conformance",
    "state": "blocked-open-question",
    "blockedAssertions": [],
    "routedTo": "issue 0004 or specification owners"
   }
  ]
 }
 ```
