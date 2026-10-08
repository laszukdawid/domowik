# Executive Decisions

This is the running record of project-level decisions made while orchestrating
Domowik. Add comments directly beneath a decision; future updates must preserve
those comments.

## Operating Contract

- Work follows issue dependencies, then priority, then issue number unless a
  documented risk or critical-path decision justifies reordering.
- At most four issues may be active at once. Each issue has one worker and one
  reviewer, for at most eight delegated agents.
  (Amended by ED-0028: the active cap is two issues / four delegated agents.)
- Every issue uses an isolated Git worktree and issue branch.
- Every worker is paired from the start with a separate adversarial reviewer
  running GPT-5.6 Luna at maximum reasoning effort.
  (Amended by ED-0029: workers run Claude Code on Opus 5; adversarial
  reviewers run OpenCode on `openai/gpt-5.6-sol`.)
- Before editing, the worker and reviewer must agree on scope, acceptance
  criteria, verification, and exclusions.
- Before handoff to the orchestrator, the reviewer must inspect the completed
  diff and verification evidence, raise adversarial findings, and explicitly
  agree with the worker that the issue is complete.
- Reviewer approval is necessary but not sufficient. The orchestrator performs
  the final diff, provenance, dependency, and verification review.
- Completed work is committed locally on its issue branch, integrated locally,
  and tested from the integration branch. Nothing is pushed unless requested.
- The nested `issues/` repository is coordinated separately from the main
  repository. Issue status changes are integrated only with the corresponding
  accepted main-repository work.
- Existing user changes and comments are preserved. Conflicting concurrent
  edits stop integration until they are reconciled deliberately.

## Performance Policy

- Performance is a release criterion, not optional polish.
- Behavioral and format work must define measurable bounds where performance
  is observable, including corpus, hardware or reference environment, dataset
  size, percentile or throughput metric, memory/allocation limits, and failure
  thresholds where applicable.
- Correctness tests and benchmarks are distinct. Benchmarks cannot replace
  deterministic tests, and tests must avoid timing-dependent assertions.
- Expensive operations must be bounded and suitable for incremental or
  visible-region execution where the architecture requires interactive use.
- Verification remains local until a CI scaffold exists; commands and results
  are recorded in issue handoffs so failures are reproducible.

## Decision Log

### ED-0001: Use two worker-reviewer pairs

- **Date:** 2026-08-22
- **Status:** Superseded by ED-0005
- **Decision:** Interpret the four-agent limit as four total delegated agents.
  Run no more than two issues concurrently, each with one worker and one
  adversarial reviewer.
- **Reason:** This satisfies the review requirement without temporarily
  exceeding the concurrency cap and keeps review continuous rather than
  deferring it until implementation is finished.

### ED-0002: Start with issues 0001 and 0020

- **Date:** 2026-08-22
- **Status:** Completed
- **Decision:** The first work wave is issue 0001, walls and joins, and issue
  0020, levels, viewport, and persistence.
- **Reason:** Both are Phase 0, High priority, and dependency-ready. Issue 0001
  unlocks issues 0002, 0003, and 0019; issue 0020 is also a direct prerequisite
  of the normative logical format.

### ED-0003: Create a local baseline before worktrees

- **Date:** 2026-08-22
- **Status:** Completed
- **Decision:** Preserve the current uncommitted design state in local baseline
  commits in both the main repository and nested issue tracker before creating
  issue worktrees. Do not push these commits.
- **Reason:** Linked worktrees start from commits and cannot safely inherit the
  current working-tree state. A local baseline prevents agents from working
  against stale specifications and makes each issue diff reviewable.

### ED-0004: Keep specifications ahead of implementation

- **Date:** 2026-08-22
- **Status:** Active
- **Decision:** Do not begin Phase 1 implementation merely because an issue is
  dependency-ready on paper. The relevant behavior and format sections must be
  normative, testable, provenance-reviewed, and accepted first.
- **Reason:** This preserves the clean-room policy and avoids encoding unresolved
  product semantics in code.

### ED-0005: Run at most four issue pairs

- **Date:** 2026-08-22
- **Status:** Superseded by ED-0028 (status metadata intentionally updated;
  decision and reason text below are preserved verbatim)
- **Decision:** The concurrency cap is four issues, each staffed by one worker
  and one adversarial reviewer, for at most eight delegated agents total.
- **Reason:** This reflects the project owner's clarification of ED-0001 while
  retaining continuous paired review. Fewer than four issues run when the
  dependency graph exposes fewer ready nodes.

### ED-0006: Treat the Domowik name as unresolved

- **Date:** 2026-08-22
- **Status:** Superseded by ED-0008 (status metadata intentionally updated;
  decision and reason text below are preserved verbatim)
- **Decision:** Issue 0009 must not claim that the Domowik name or related
  identifiers are cleared. A public Android home-organizer product currently
  uses the Domowik name, `domowik.dev`, and `pl.domowik.app`. The issue may
  document due diligence and recommend retention or replacement, but final
  ratification requires confirmation of the relationship to that product or an
  approved rename.
- **Reason:** Reusing a live product name without establishing ownership or an
  acceptable coexistence strategy creates avoidable trademark, package-ID,
  discoverability, and user-confusion risk. This is a risk gate, not a legal
  conclusion.

### ED-0007: Assign models by role

- **Date:** 2026-08-22
- **Status:** Superseded by ED-0029 (status metadata intentionally updated;
  decision and reason text below are preserved verbatim)
- **Decision:** Issue workers run on Ox Alpha. Adversarial reviewers run on
  GPT-5.6 Luna at maximum reasoning effort. GPT-5.6 Sol must not author issue
  work.
- **Reason:** This records the project owner's model assignment explicitly. The
  initial Sol-authored worktrees were stopped and removed without integration;
  replacement work starts from the clean baseline.

> **Comment 2026-08-24:** Post-wave audit found issue 0034's reviewer pane ran
> `gpt-5.6-luna` at medium reasoning effort rather than maximum. Recorded as an
> orchestration incident; that wave's outputs are held regardless by ED-0013.
> Current launches pass `model_reasoning_effort=xhigh` (Codex) or
> `--variant max` (OpenCode) explicitly.

### ED-0008: Adopt domowik.dev provisionally; supersedes the naming gate

- **Date:** 2026-08-23
- **Status:** Active
- **Supersedes:** ED-0006 (naming treated as unresolved)
- **Decision:** Record the project owner's executive input verbatim: "For the
  url, domowik.dev sounds good. We'll definitely change this in the future so
  make sure that it's easy to find and modify. This shouldn't be a blocker."
  Adopt `https://domowik.dev` as an explicitly temporary project URL, expected
  to change, derive the provisional Go module path from it, and treat naming
  as non-blocking for issue 0009. Every identity value is provisional or
  rule-deferred and is tracked centrally in [docs/NAMING.md](docs/NAMING.md)
  with a defined migration-alias location so a future change is mechanical.
  This supersedes the Blocked state recorded for issue 0009 on 2026-08-23.
  It establishes no legal clearance, availability, ownership, affiliation,
  non-affiliation, or non-infringement for any identifier or any party.
- **Reason:** The owner accepts future change as expected and prioritizes
  replaceability over finality. The canonical registry makes that
  replaceability mechanically testable while the due-diligence dossier's
  observations, URLs, timestamps, limitations, and original matrix data are
  preserved as risk history, and ADR 0006's release gate retains the
  pre-publication checks ED-0006 was protecting.

### ED-0009: Launch the first recovered wave on issues 0006, 0007, 0034, and 0036

- **Date:** 2026-08-23
- **Status:** Active
- **Affected issues:** 0006, 0007, 0022, 0034, 0036
- **Decision:** After recovering orchestration, launch issues 0006 and 0007
  against issue 0022's explicitly reviewed Candidate handoff, plus
  dependency-ready issues 0034 and 0036. Issue 0022 remains In Progress until
  the spike evidence is consumed by issue 0030; its open status does not block
  the two downstream evidence-producing spikes because the handoff text
  expressly assigns those remaining gates to them. Issue 0034 may perform and
  document read-only due diligence and make reversible repository-local
  identity decisions, but it must not claim legal clearance or reserve an
  external namespace/account without separately established authority. Issue
  0036 must treat present release artifacts as a reproducible baseline whose
  component-set checks continue to fail closed as future distributed content
  is added.
- **Reason:** These are the four highest-priority unfinished issues whose
  declared dependencies have either reached Done or provide an explicit
  reviewed downstream handoff. Running the two geometry spikes together
  shortens the Phase 0 critical path to numerical ratification. Starting the
  two release-risk issues early exposes identity and supply-chain gaps before
  they become release blockers, while the stated limits avoid unauthorized
  external actions and premature completeness claims.

### ED-0010: Replenish reviewed work continuously

- **Date:** 2026-08-24
- **Status:** Active
- **Affected issues:** all orchestrated work
- **Decision:** While unfinished dependency-ready work exists, a completed
  issue pair must trigger prompt orchestrator review, integration or a
  documented rejection/continuation decision, tracker synchronization,
  finished-agent purge, and replacement dispatch. Agent lifecycle alone is
  insufficient: the orchestrator must inspect pair communication, the final
  diff, verification evidence, and branch state. Capacity may remain unused
  only when no issue can make meaningful progress without unresolved
  dependencies, external authority, or a recorded validation gate.
- **Reason:** The recovered first wave completed successfully but remained
  unattended after both agents reached Done, leaving reviewed work
  unintegrated and all useful capacity idle. Continuous replenishment is part
  of the orchestrator role, not an optional follow-up.

### ED-0011: Reject flat segment-soup face extraction as conformance evidence

- **Date:** 2026-08-24
- **Status:** Active
- **Affected issues:** 0007, 0022, 0030, 0013
- **Decision:** The existing internal spike API
  `CandFaces([][2][2]float64)` remains a characterization artifact only. Its
  face, hole, exterior, determinism, and performance results must not be used
  as conforming planar-face evidence or strategy-selection evidence. Issue
  0007 must stop redesigning that flat API, preserve the independently
  reproduced ambiguity and permutation defect as evidence, and complete only
  its separable boolean/oracle work. It may hand off as `In Progress (partial
  scope)` with a deferred-closure gate on issue 0030. Issue 0030 may activate
  from that explicit partial handoff despite its ordinary dependency on 0007;
  before judging or authorizing any replacement implementation, it must choose
  and specify a provenance-preserving structured face-extraction input that
  retains ring/operand roles and multiplicity, or explicitly reject face
  extraction as a selected candidate. This decision changes no public format
  or product API and authorizes no implementation by itself.
- **Reason:** Erasing ring direction and operand/role provenance is
  information-losing. A filled outer plus filled inner ring and an outer plus
  a hole can reduce to the same eight undirected segments while requiring
  different canonical topology; likewise, undirected deduplication cannot
  distinguish an internal shared seam from a genuine retraced boundary.
  Directed half-edge traversal can enumerate the resulting planar graph but
  cannot reconstruct semantics discarded before traversal. The issue pair
  independently reproduced the practical defect: a non-uniform permutation
  plus endpoint reversal changed the pre-chain state from 12 fragments / 12
  directed pieces / 2 cycles to 16 / 10 / 1. Treating an arbitrary canonical
  pairing as recovered semantics would be an unreviewed API decision. The
  partial-handoff gate lets numerical ratification make that decision with the
  ambiguity visible instead of forcing a circular dependency or accepting
  false conformance.
- **Validation/reversal gate:** Supersede this decision only with a normative,
  testable input contract and independent proof that it preserves enough
  provenance to distinguish holes, filled nested components, duplicates,
  seams, contacts, and invalid rings under input permutation. Any resulting
  implementation requires its own reviewed tests, provenance, metrics, and
  0030 selection judgment.

### ED-0012: Reject the flat candidate face input; continue the numerical model unratified

- **Date:** 2026-08-24
- **Status:** Active
- **Affected issues:** 0030, 0007, 0013
- **Decision:** Exercising the rejection branch ED-0011 delegates to issue
  0030: the flat provenance-erasing face input
  (`CandFaces([][2][2]float64)`) is REJECTED as a selected approach; no
  structured replacement is ratified. The numerical geometry model remains
  Candidate (not ratified) and ADR 0002 remains Proposed; issue 0030
  records/leaves its outcome as PARTIAL/NOT COMPLETE, with registered
  performance reruns (see the
  pre-registered criterion), B-1q's certify-or-escalate gate, and the
  remaining H-register/API/format gates required before any promotion.
  Full record:
  [docs/evidence/0030-numerical-model/RATIFICATION.md](docs/evidence/0030-numerical-model/RATIFICATION.md).
- **Reason:** The flat input provably discards ring direction,
  multiplicity, and operand/ring provenance (ambiguities P1/P2 plus the
  measured permutation defect), so no conforming face classifier can exist
  over it; ratifying anything less would encode false conformance.
  Reversible: any future face capability restarts through ED-0011's
  structured-contract validation gate with its own tests, provenance, and
  selection judgment.

### ED-0013: Hold identity finalization at the evidence-authority gate

- **Date:** 2026-08-24
- **Status:** Active
- **Affected issues:** 0034, 0036, 0018
- **Decision:** Issue 0034 remains `In Progress (partial scope)` with no further
  repository edits, identifier selection, namespace reservation, or tracker
  closure authorized from the current automated evidence. Existing external
  evidence remains retained as timestamped, access-limited observations. The
  outstanding interactive trademark/package checks require fresh human-
  interactive evidence or an explicitly authorized re-scope before another
  evidence pass can begin.
- **Reason:** The current evidence does not establish the remaining acceptance
  criteria, and clean-room exclusions prohibit inferring legal clearance,
  availability, ownership, affiliation, or non-infringement. Repeating
  restricted automated queries would not close the gate and risks presenting
  absence as evidence.
- **Validation/reversal gate:** Reopen only after the owner supplies missing
  interactive-evidence authority or explicitly re-scopes 0034. Any continuation
  must preserve UTC timestamps, exact URLs/queries and outputs, access
  restrictions, and the issue's non-legal-claims language.

### ED-0014: Activate issue 0004 against labeled provisional handoffs

- **Date:** 2026-08-24
- **Status:** Active
- **Affected issues:** 0004, 0021, 0030, 0005
- **Decision:** Issue 0004 begins specification work now. It consumes: (a) the
  Done behavior specifications 0001/0002/0003/0019/0020 as normative input;
  (b) only material labeled "Provisional handoff — 0021 partial scope"
  (ASSET-STRATEGY §10, FORMAT-SPEC §6) for asset-related fields; and (c)
  NUMERICAL-MODEL coordinate/bound values strictly as Candidate inputs,
  encoded as named single-definition parameters marked provisional, never as
  ratified limits. Numeric-bound fixation that RATIFICATION §6 lists as NOT
  APPROVED — including the FORMAT-SPEC L14 delegation and BEHAVIOR-SPEC R3.0.8
  precision deferrals — remains explicitly deferred with pointers; schemas may
  reference parameters without fixing values. Issue 0004 may not reach Done
  while any 0030-dependent parameter fixation or deferred marker remains,
  except by explicit re-scope.
- **Reason:** Every ordinary dependency is Done or publishes an explicitly
  labeled provisional handoff designed for dependent consumption (tracker
  partial-scope semantics; ED-0009 precedent). Issue 0004 heads the Phase 1
  critical path: all core/CLI issues chain through 0004 → 0010 → 0011, so
  waiting for full 0030 ratification idles every slot despite ready
  specification work. Parameterization keeps the format honest about
  unresolved numeric limits and makes later fixation mechanical. This decision
  selects no technology and changes no product direction.
- **Validation/reversal gate:** If 0030's trigger-3 revision or any future
  structured-face outcome changes units, bounds structure, or operation-
  metadata assumptions, the affected 0004 sections must be revised before its
  closure. Superseded immediately by any contrary owner direction.

### ED-0015: Reviewer surface fallback while Codex quota is exhausted

- **Date:** 2026-08-24
- **Status:** Active
- **Affected issues:** all dispatched pairs until quota reset
- **Decision:** The Codex CLI surface is account-wide usage-limited until
  2026-08-30 17:07; both launch-day reviewers failed on their first model
  call. Adversarial reviewers for active pairs run GPT-5.6 Luna via OpenCode
  (`--model openai/gpt-5.6-luna --variant max`), preserving the model and
  maximum-effort requirements of ED-0007 and the orchestration contract; only
  the hosting CLI differs. New dispatches revert to Codex once its quota
  resets or the owner directs otherwise. Replacement reviewers receive the
  full original contracts verbatim.
- **Reason:** Continuous paired adversarial review is an operating-contract
  requirement; idling two dependency-ready issues for six days over a hosting
  surface would sacrifice the substance of review (model identity, effort,
  independence from the executor) for its transport.
- **Validation/reversal gate:** Revert to Codex when the usage limit resets or
  on any observed degradation in review rigor; superseded immediately by
  contrary owner direction.

> **Comment 2026-08-24:** The installed OpenCode CLI exposes no per-launch
> reasoning-effort flag (`--variant` no longer exists); replacement reviewers
> run `opencode -m openai/gpt-5.6-luna` with provider-default effort. Codex
> reversion at quota reset restores guaranteed xhigh.

> **Comment 2026-08-24 (2):** The `openai/gpt-5.6-luna` OpenCode path shares
> the exhausted ChatGPT quota. Reviewers were relaunched on
> `opencode-go/gpt-5.6-luna` — same GPT-5.6 Luna model via a separately
> metered provider — which passed connectivity probes. Contracts re-sent in
> full; no review work occurred before this swap.

### ED-0016: Activate issue 0010 against the accepted 0004 partial handoff

- **Date:** 2026-08-24
- **Status:** Active
- **Affected issues:** 0010, 0004
- **Decision:** Issue 0010 begins implementation now. It consumes FORMAT-SPEC
  §2–§5 as integrated on main (`6041867`, refreshed `04a8304`) — normative,
  testable (deterministic vectors plus the jsoncheck step), provenance-
  reviewed and accepted through paired review and orchestrator integration —
  together with the Done behavior specifications. Numeric values are consumed
  ONLY through the named provisional parameters; no code or test may fix a
  parameter to an independent literal, and RATIFICATION §6 deferrals stay
  untouched. Issue 0010 may not reach Done while 0004 remains unclosed under
  ED-0014 gates, except by explicit re-scope.
- **Reason:** ED-0004's precondition keys on specification maturity, not
  upstream Done badges: the relevant format sections are now normative,
  testable, provenance-reviewed, and accepted, which is exactly the state
  ED-0004 demands before Phase 1 implementation. The tracker's partial-scope
  semantics govern consuming delivered material (ED-0009/ED-0014 precedent),
  and 0010 heads the remaining critical path (0011 feeds nearly every core/
  CLI issue).
- **Validation/reversal gate:** If any 0030 outcome changes a named
  parameter's structure or units, the affected 0010 code and tests must be
  revised before its closure. Superseded immediately by contrary owner
  direction.

### ED-0017: Activate issue 0005 against labeled provisional handoffs

- **Date:** 2026-08-24
- **Status:** Active
- **Affected issues:** 0005, 0004, 0021
- **Decision:** Issue 0005 begins specification work now. It consumes the Done
  0009/0023 outputs; only material labeled "Provisional handoff — 0021 partial
  scope" (ASSET-STRATEGY §10, FORMAT-SPEC §6) for asset/package fields; and
  FORMAT-SPEC §2–§5 as integrated on main (`6041867`), referencing named
  provisional parameters without fixing numeric values. Operation-batch
  naming/order/framing and compression-setting fixation transfer to 0005 as
  its owning deliverables, honoring 0021's contingent-clause structure.
  Issue 0005 may not reach Done while any consumed-provisional or RATIFICATION
  §6 gate touching its criteria remains open, except by explicit re-scope.
- **Reason:** Every ordinary dependency is Done or publishes an explicitly
  labeled provisional handoff designed for dependent consumption (ED-0009/
  ED-0014 precedent). The container decision is numerically independent of the
  unratified model, and 0005 gates the persistence/security chain (0028, 0029,
  0031) plus one of 0021's own closure gates, so waiting idles capacity without
  risk-reduction benefit.
- **Validation/reversal gate:** If 0004 parameter structure, 0030 outcomes, or
  0021 gate resolutions change framing or compression assumptions, the
  affected 0005 sections must be revised before closure. Superseded
  immediately by contrary owner direction.

### ED-0018: Activate issue 0011 against accepted partial handoffs

- **Date:** 2026-08-24
- **Status:** Active
- **Affected issues:** 0011, 0004, 0010
- **Decision:** Issue 0011 begins implementation now. It consumes FORMAT-SPEC
  §2–§5 (normative; batch/frontier/idempotency/tombstone/LWW semantics and the
  V1–V7 conformance vectors), the Done undo behavior specification (0003), and
  issue 0010's delivered `internal/document` material as a labeled provisional
  handoff, including ownership of the command-layer invariants 0010 explicitly
  deferred (at-least-one-level). Numeric values flow only through named
  provisional parameters; RATIFICATION §6 deferrals stay untouched. Issue 0011
  may not reach Done while 0004 or 0010 remain unclosed under their recorded
  gates, except by explicit re-scope.
- **Reason:** ED-0004's maturity precondition is satisfied by accepted
  normative sections plus reviewed deterministic vectors; both dependency
  edges resolve to Done or accepted partial handoffs under the tracker's
  partial-scope semantics (ED-0009/ED-0014/ED-0016 precedent). Issue 0011 is
  the chokepoint of the Phase 1 graph: 0012–0017 and 0024–0029 chain through
  it.
- **Validation/reversal gate:** If any 0030 outcome changes a named parameter,
  or 0010's deferred gates alter the apply-seam contract, affected 0011 code
  and tests must be revised before closure. Superseded immediately by contrary
  owner direction.

### ED-0019: Activate issue 0028 against accepted partial handoffs

- **Date:** 2026-08-24
- **Status:** Active
- **Affected issues:** 0028, 0004, 0005, 0021
- **Decision:** Issue 0028 begins corpus authoring now. It consumes FORMAT-SPEC
  §2–§9.6 as integrated on main (normative logical model, container profile,
  versioning rules, security limits/rejection taxonomy, and the §9.6
  conformance-fixture obligations addressed to this issue), plus only labeled
  "Provisional handoff — 0021 partial scope" material for asset fields.
  Fixtures are authored from these specs before any persistence implementation,
  exactly as the issue intends. Numeric values appear only as references to
  named provisional parameters or §9.3 quantities; no independent literals.
  Issue 0028 may not reach Done while its upstream issues remain unclosed
  under their recorded gates, except by explicit re-scope.
- **Reason:** All four dependency edges resolve to Done or accepted partial
  handoffs under the tracker's partial-scope semantics (ED-0009/ED-0014/
  ED-0016/ED-0017 precedent). The spec surface the corpus tests is now
  normative and stable except for isolated parameter fixation, and authoring
  fixtures early exposes specification ambiguities while they are cheap to
  fix.
- **Validation/reversal gate:** If any upstream gate resolution changes schema,
  framing, container, or limit structure, affected fixtures and their expected
  categories must be revised before closure. Superseded immediately by
  contrary owner direction.

### ED-0020: Activate issue 0024 against the accepted 0011 partial handoff

- **Date:** 2026-08-25
- **Status:** Active
- **Affected issues:** 0024, 0011
- **Decision:** Issue 0024 begins its bridge feasibility spike now. It consumes
  the Done scaffold (0008) and issue 0011's delivered `internal/document`
  material as a labeled provisional handoff as the stateful core being bridged.
  Environment audit comes first: available toolchains are recorded exactly;
  platforms whose toolchain is absent are assessed-and-deferred with explicit
  gap documentation, never simulated. All Dart/Flutter/native tooling gets
  ledger-before-reliance third-party records; the main Go module stays
  pure-Go single-module (c-shared artifacts live in spike harnesses only).
  Issue 0024 may not reach Done while 0011 remains unclosed under ED-0018's
  gates, except by explicit re-scope.
- **Reason:** Both dependency edges resolve to Done or an accepted partial
  handoff (ED-0009-line precedent). ROADMAP sequences Phase 2 to begin with
  this exact validation before any GUI work, and its outcome directly informs
  the owner's pending post-trigger-3 stack decision — starting it now shortens
  the architecture risk path instead of idling the slot while 0028 finishes.
- **Validation/reversal gate:** If 0011's deferred gates alter the apply-seam
  contract, affected bridge code/tests must be revised before closure.
  Superseded immediately by contrary owner direction.

### ED-0021: Activate issue 0029 against accepted partial handoffs

- **Date:** 2026-08-25
- **Status:** Active
- **Affected issues:** 0029, 0005, 0011, 0004, 0028
- **Decision:** Issue 0029 begins persistence implementation now. It consumes
  the normative FORMAT-SPEC §7/§8 container and versioning specification plus
  §9 security limits (0005/0023), issue 0011's delivered replay/session
  material as a labeled provisional handoff, and the issue 0028 fixture corpus
  with its machine-readable expectations. Numeric values flow only through
  named provisional parameters or §9.3 quantities. Issue 0029 may not reach
  Done while 0005, 0011, or 0004 remain unclosed under their recorded gates,
  except by explicit re-scope.
- **Reason:** Every dependency edge resolves to Done or an accepted partial
  handoff under the tracker's partial-scope semantics (ED-0009-line
  precedent). The spec surface this issue implements is normative and stable
  outside isolated parameter fixation, and persistence is the last Phase 1
  blocker for the CLI chain (0014 onward).
- **Validation/reversal gate:** If any upstream gate resolution changes
  framing, container rules, limits structure, or the apply-seam contract,
  affected code/tests must be revised before closure. Superseded immediately
  by contrary owner direction.

### ED-0022: Activate issues 0015, 0026, 0027, and 0031 against accepted partial handoffs

- **Date:** 2026-08-25
- **Status:** Active
- **Affected issues:** 0015, 0026, 0027, 0031, 0004, 0011, 0021, 0028, 0029
- **Decision:** All four issues begin implementation now, consuming the
  normative FORMAT-SPEC sections, the delivered `internal/document` /
  `internal/persist` material as labeled provisional handoffs, and the 0028
  fixture corpus with machine-readable expectations (known-defective records
  stay in their closed fail-safe skip sets). Numeric values flow only through
  named parameters or §9.3 quantities. CLI wiring across 0015/0027 must stay
  isolated per-command to minimize integration conflicts. None may reach Done
  while its upstream issues remain unclosed under their recorded gates,
  except by explicit re-scope.
- **Reason:** Every dependency edge of all four issues now resolves to Done or
  an accepted partial handoff under the tracker's partial-scope semantics
  (ED-0009-line precedent). The four are mutually independent — three distinct
  CLI surfaces over shared seams, one migration layer over container+corpus —
  so they fill exactly the four concurrency slots without blocking each other,
  completing the Phase 1 command surface.
- **Validation/reversal gate:** Upstream gate resolutions that alter seams,
  fixtures, or limits require revision of affected code/tests before closure.
  Superseded immediately by contrary owner direction.

> **Comment 2026-08-25:** After several executor OpenCode process crashes,
> some replacement sessions came up on the `opencode zen` client/provider
> path rather than `opencode-go`. Session-reported model identity remains
> ox-alpha (`opencode/x-preview-f-free`) at max effort, so ED-0007's model
> assignment holds in substance; only the transport differs, accepted
> provisionally under ED-0015's reasoning pending owner ratification.

### ED-0023: Authorize a parse-only persistence inspection seam for issue 0027

- **Date:** 2026-08-25
- **Status:** Active
- **Affected issues:** 0027, 0029
- **Decision:** Issue 0027 may add a minimal read-only `internal/persist`
  inspection seam (for example `inspect.go`) because `persist.LoadFile` replays
  batches and cannot satisfy `domowik ops`' mandatory no-execution contract.
  The seam stops after container, manifest, limit, digest, namespace, snapshot-
  metadata, and structural batch-framing checks. It must never construct
  document operations, replay a snapshot, call Deliver, or mutate state;
  existing Load behavior must remain unchanged.
- **Reason:** Reusing the replaying load path would violate issue 0027 AC1;
  reimplementing the parser in CLI code would fork security-critical logic.
  A narrow internal seam preserves one parser and is the smallest reversible
  solution.
- **Validation/reversal gate:** Before editing, the pair must agree on
  differential non-execution tests (structurally valid/application-invalid
  packages inspect successfully while Load rejects), corpus classification
  parity through framing, inherited limit enforcement, and a mutation probe
  that fails if replay/application enters the seam. Any broader persistence
  API change requires a new decision.

### ED-0024: Activate issue 0035 Part A against accepted partial handoffs

- **Date:** 2026-08-25
- **Status:** Active
- **Affected issues:** 0035, 0004, 0005, 0021, 0026, 0018
- **Decision:** Issue 0035 begins the `empty-v1` and
  `unfurnished-1000-v1` generator, self-check, measured thin-package records,
  preview inventories, and encoding-candidate benchmark records now. It
  consumes FORMAT-SPEC §2–§8 and the issue 0026 writer/tooling as labeled
  provisional handoffs, plus ASSET-STRATEGY §10.9 Part A. The
  `furnished-cached-v1` scenario remains dormant because BEHAVIOR-SPEC §10 is
  still undefined; workers must not invent furniture placement or instance
  semantics. External benchmark tools require ledger-before-reliance review
  under THIRD-PARTY-POLICY, and no candidate becomes a format requirement.
  Measurements name the actual local environment and retain raw outputs.
- **Reason:** All formal dependencies resolve to accepted partial handoffs and
  Part A is normative and implementable. Running the generator against the
  delivered writer now closes empirical risk without crossing the explicit
  furnished-scenario gate.
- **Validation/reversal gate:** Issue 0035 remains In Progress while 0004,
  0005, or 0026 remains unclosed and while the furnished scenario is dormant.
  Any measurement at or above budget records a budget revision rather than a
  silent pass. Issue 0018 must add the documented 0035 publication dependency
  when its scope is next activated. Superseded by contrary owner direction.

### ED-0025: Clarify issue 0035 Part A corpus ambiguities

- **Date:** 2026-08-25
- **Status:** Active pending issue 0021 amendment
- **Affected issues:** 0021, 0035
- **Decision:** For `unfurnished-1000-v1`, number ring instances globally in
  level-major/ring-major order as `g` in 0 through 39. Each instance `g` in 0
  through 38 receives four doors: one on the middle sub-wall of each edge in
  bottom, right, top, left order. This yields the normative total of 156
  non-overlapping openings (`39 * 4`) while preserving all 480 wall and 320
  join formulas. Opening identity/order is level, ring, then edge order.
  Separately, the stated one-entry style registry is a generator-local corpus
  vocabulary containing only `S0`, not a wire object. Walls and openings
  serialize `styleRefs:["S0"]`; room optional style references stay absent.
  The independent self-check resolves every serialized style token against
  that local vocabulary. No manifest/entity/cache field is added.
- **Reason:** ASSET-STRATEGY §10.9's phrase “one door on rings 0 through 38's
  bottom-middle sub-wall” describes only 39 openings, contradicting its
  normative total of 156, and its local `r` range is only 0 through 9. Four
  distinct edge-middle hosts on each of 39 flattened instances is the smallest
  deterministic interpretation that satisfies the total without overlapping
  openings. FORMAT-SPEC §2.3 defines style references as opaque tokens and
  §2.4 permits no authoritative document-level registry, so serializing a new
  registry would invent incompatible wire semantics.
- **Validation/reversal gate:** Issue 0021 must amend §10.9 to state or replace
  these rules before either issue closes. If that owner selects another
  construction or later defines a registry wire representation, issue 0035
  regenerates its corpus and measurements; no compatibility claim attaches to
  this provisional corpus. Superseded immediately by contrary owner direction.

### ED-0026: Activate issue 0018 as a gated v0.1 release-candidate audit

- **Date:** 2026-08-25
- **Status:** Active
- **Affected issues:** 0018, 0005, 0011, 0026, 0027, 0028, 0029, 0031,
  0035, 0036
- **Decision:** Issue 0018 begins against the accepted provisional handoffs
  from every formal dependency plus issue 0035's measured-budget publication
  handoff, which becomes an explicit dependency. Work may audit and close
  golden-corpus coverage, state the exact experimental v0.1 compatibility and
  migration promise in its owning ADR, assemble reproducible release-candidate
  evidence, and identify upstream blockers. It must not create a tag, publish
  artifacts, call v0.1 released, mark the compatibility ADR Accepted, or claim
  Done while any dependency or 0035 remains unclosed. No earlier public schema
  or positive migration may be fabricated; an empty predecessor set must be
  stated honestly.
- **Reason:** All original dependency edges now resolve to Done or reviewed
  partial handoffs, and 0035 delivered the measured size records explicitly
  handed to 0018. Performing the cross-cutting audit now exposes remaining
  release gaps without converting provisional implementation into a public
  compatibility commitment.
- **Validation/reversal gate:** Closure requires every dependency including
  0035 to close, all claimed corpus categories to pass, the exact compatibility
  promise to receive its required ADR approval, and a clean full release
  verification. Upstream defects are routed back to their owners rather than
  silently repaired by weakening tests. Superseded by contrary owner direction.

### ED-0027: Activate release-audit remediation for issues 0026, 0028, and 0029

- **Date:** 2026-08-25
- **Status:** Active
- **Affected issues:** 0018, 0026, 0028, 0029, 0031
- **Decision:** Three continuation pairs may address the actionable blockers
  routed by issue 0018. Issue 0028 owns skipped/partial semantic vectors,
  hostile-container fixtures, duplicate-key bytes, and framing coverage; it
  must not weaken expected categories or convert an impossible vector to a
  pass. Issue 0026 owns a complete duplicate-blob and detectable-orphan/crash
  matrix within its cache/generation model. Issue 0029 owns missing save,
  recovery, opaque-data round-trip, and fault-phase proofs within documented
  platform capabilities. Cross-platform or power-loss outcomes may be
  assessed-and-deferred when unavailable, never simulated. Normative ambiguity
  is routed to its spec owner instead of decided by a test workaround.
- **Reason:** These gaps fit existing acceptance criteria and can be tested
  independently on current main. Resolving them now reduces the concrete
  release audit deficit while preserving the fail-closed corpus and platform
  evidence contracts.
- **Validation/reversal gate:** Each pair must reproduce its audit finding,
  agree on a pre-edit matrix, add adversarial regressions, and pass full
  verification. Issues remain In Progress under existing upstream gates.
  Issue 0031's absent independently evidenced predecessor release is not part
  of this wave and must not be fabricated. Superseded by contrary owner
  direction.

### ED-0028: Reduce the active cap to two issue pairs

- **Date:** 2026-08-26
- **Status:** Active
- **Supersedes:** ED-0005 (four concurrent issue pairs)
- **Affected issues:** all scheduling
- **Decision:** Record the project owner's executive input verbatim: "Work at
  two issues max at a time." At most two issues are active concurrently, each
  staffed by one worker and one adversarial reviewer, for at most four
  delegated agents total. Fewer pairs run whenever the dependency graph,
  normative-specification gates, or the verification baseline expose fewer
  ready issues.
- **Reason:** This is the owner's direct instruction and replaces the cap
  recorded in ED-0005. It does not change any issue's scope, dependency
  semantics, or acceptance criteria.
- **Validation/reversal gate:** Superseded by contrary owner direction.

### ED-0029: Reassign role models to Claude Code and OpenCode Sol

- **Date:** 2026-08-26
- **Status:** Active
- **Supersedes:** ED-0007 (Ox Alpha workers, GPT-5.6 Luna reviewers)
- **Affected issues:** all delegated work
- **Decision:** Record the project owner's executive input verbatim: "For the
  worker use Claude Code with Opus 5 and for reviewer use OpenCode with
  openai/gpt-5.6-sol." Issue workers run Claude Code on `claude-opus-5`.
  Adversarial reviewers run OpenCode on `openai/gpt-5.6-sol`, launched with
  the maximum reasoning variant the surface accepts, per the ED-0007 comment's
  standing effort requirement. ED-0007's prohibition on Sol authoring issue
  work is retained in its new form: Sol reviews and must not author issue
  work; authorship belongs to the Claude Code worker.
- **Reason:** This is the owner's direct instruction and replaces the model
  assignment recorded in ED-0007. The adversarial-review contract, the
  pre-edit scope agreement, and the orchestrator's independent final review
  are unchanged; only the model behind each role changes.
- **Validation/reversal gate:** If a launched reviewer pane cannot be placed
  at maximum reasoning effort, record the achieved effort in the issue handoff
  as an orchestration incident rather than silently accepting a lower tier.
  Superseded by contrary owner direction.

### ED-0030: Restore the verification baseline after the macOS host migration

- **Date:** 2026-08-26
- **Status:** Active
- **Affected issues:** 0035, 0018, 0021, and every issue whose acceptance
  depends on `./scripts/verify.sh`
- **Decision:** The repository now runs on the owner's macOS/arm64 host, while
  all committed issue-0035 Part A evidence was captured on a Linux/amd64
  reference host (`raw/environment.txt`: arch linux, Ryzen 9 7950X, btrfs,
  `go1.27.0 linux/amd64`) with Python 3.14.7 and zlib 1.3.2. Three
  consequences are recorded, with the first two resolved here:
  1. **Go toolchain.** `scripts/verify.sh` pins `GOTOOLCHAIN=local` and
     `go.mod` requires `go >= 1.27`, but the host's `go` on `PATH` is
     Homebrew 1.26.5 and no `go@1.27` formula exists. Until the host installs
     Go 1.27 as its default, every Domowik verification run prepends the
     already-downloaded module-cache toolchain
     `~/go/pkg/mod/golang.org/toolchain@v0.0.1-go1.27.0.darwin-arm64/bin` to
     `PATH`. This changes no repository content and no pinned policy.
  2. **Clean-checkout gate.** The untracked `CLAUDE.md -> AGENTS.md` symlink
     the owner's Claude Code setup requires made `domowik-sbom verify` and the
     P13 clean-tree assertion fail closed. It is excluded locally through
     `.git/info/exclude`, which leaves the tracked tree and `.gitignore`
     untouched and applies to linked worktrees through the shared common dir.
     Additionally, commits `a74f6ce` and `00893a5` brought `issues/*.md` into
     the tracked source set without refreshing the release inventory; the
     stale SPDX source document is regenerated and committed as `5440373`.
  3. **Host-bound evidence gate (open).** `internal/sizecorpus`
     `TestCommittedArtifactsConsistent` compares committed evidence records
     against live regeneration under a guard that tests only
     `GOTOOLCHAIN=local`, not the reference environment the records themselves
     name. On this host the packages and their digests still regenerate
     byte-identically — the measured budgets are intact and host-independent —
     but `raw/environment.txt`, `raw/go-version.txt`, `raw/python3-zlib.txt`,
     the preview inventories, and the env-bearing budget records cannot match,
     so the gate false-fails. This is the single accepted baseline exception
     until ED-0031 closes it. No committed 0035 evidence may be regenerated,
     re-measured, or overwritten to make the gate pass.
- **Reason:** INSTRUCTIONS.md requires `./scripts/verify.sh` to exit 0 before
  work is accepted. Items 1 and 2 are host and housekeeping defects with no
  bearing on product semantics. Item 3 is a portability defect in a
  fail-closed evidence gate, not evidence of a regression: treating it as a
  licence to rewrite Linux-captured provenance would destroy the reference
  record that ADR/ASSET-STRATEGY §10.10 and issue 0018 row S depend on.
- **Validation/reversal gate:** Until ED-0031 lands, an issue pair's
  verification evidence is accepted only if `./scripts/verify.sh` reproduces
  exactly this baseline — every step passing except
  `TestCommittedArtifactsConsistent` — with no new failure and a clean tree.
  Any pair that observes a different failure stops and reports it. Escalated
  to the owner separately: whether the Linux reference host remains the
  authority for 0035 measurement, and whether Python 3.14.7 / zlib 1.3.2
  should be restored on this host.

> **Comment 2026-08-27 (owner-directed toolchain fix; items 1 and 2 resolved,
> the escalated Python question ANSWERED).**
>
> **Item 1 is retired.** On owner direction, Go 1.27 is installed globally
> through mise (`~/.config/mise/config.toml`, `go = "1.27"`). `GOTOOLCHAIN=local`
> now resolves `go1.27.0` and **`./scripts/verify.sh` exits 0 with no PATH
> prefix of any kind**. The module-cache workaround
> (`~/go/pkg/mod/golang.org/toolchain@v0.0.1-go1.27.0.darwin-arm64/bin`) is no
> longer required and should not be reintroduced. mise is activated from
> `~/.bashrc`, so interactive shells — including agent panes — resolve it
> automatically; non-interactive contexts can use `~/.local/share/mise/shims`.
>
> **The escalated question about restoring Python 3.14.7 / zlib 1.3.2 resolves
> as NO — it cannot be done on this host.** Measured across every available
> source: uv's `cpython-3.14.6`, uv's `cpython-3.14.2`, uv's 3.13 and 3.12
> builds, Homebrew's `python@3.14`, and Apple's `/usr/bin/python3` **all report
> `zlib 1.2.12`**. macOS ships 1.2.12 and python-build-standalone links it, so
> no Python version manager changes the answer. Python 3.14.7 itself is also
> unavailable — uv's newest 3.14 is 3.14.6. **The issue-0035 reference
> environment exists only on the Linux host and cannot be reproduced here.**
>
> That is harmless rather than blocking, for two reasons already on the
> record: ED-0031 scoped the evidence gate to the environment its records name,
> so a non-conforming host degrades loudly instead of false-failing; and the
> pinned ASSET-STRATEGY §10.9 thumbnail vector still reproduces byte-exactly
> under zlib 1.2.12, so the measured budgets are unaffected. What remains open
> is only the separate question of whether the Linux host stays issue 0035's
> measurement authority.
>
> **Python is pinned LOCALLY, not in the repository.** A `mise.toml` pinning
> `python@3.14.6` sits in the working tree and is excluded through
> `.git/info/exclude`, exactly as item 2 handles `CLAUDE.md`. It is deliberately
> not committed: doing so would make mise a requirement for reproducing the
> project's Python, which is a THIRD-PARTY-POLICY tooling decision belonging to
> the owner rather than an orchestration call. The value it delivers is
> defence against the drift that caused this whole item — Homebrew silently
> moving 3.14.7 to 3.14.6 — not a fix, since ED-0031 already makes that drift
> harmless.
>
> **A `.python-version` file was tried and REJECTED as inert.** uv honours it,
> but mise does not drive `PATH` from it, and
> `internal/sizecorpus/preview.go` shells out to bare `python3` — so the file
> would have looked like a pin while governing nothing that the build actually
> consults. That is ED-0041's shape applied to configuration: an artifact that
> resembles the thing that does the work without doing it.

> **Comment 2026-08-26 (issue 0028 pair):** ED-0030's validation gate said an
> accepted run shows "every step passing except
> `TestCommittedArtifactsConsistent`". That is imprecise and `w0028` was right
> to challenge it: `verify.sh` runs under `set -e`, so the script aborts at
> `go test ./...` and **no later step executes at all** on this host — the
> release-inventory, probe, cross-build, and clean-tree steps included. Until
> ED-0031 lands, the evidence contract is therefore: (a) the `verify.sh` run
> verbatim, exiting 1 with only the sizecorpus record-regeneration failure;
> (b) every step after `go test` executed manually and reported individually;
> (c) `git status --porcelain` clean. Branches still refresh the release
> inventory as their final commit so the combined tree verifies once ED-0031
> lands. The original decision text above is preserved unchanged.

### ED-0031: Activate an issue 0035 continuation for the host-portable evidence gate

- **Date:** 2026-08-26
- **Status:** Active
- **Affected issues:** 0035, 0021, 0018
- **Decision:** A continuation pair may make issue 0035's committed-artifact
  gate correctly scoped to the reference environment its own records name, so
  the gate passes on a conforming host and degrades loudly — with an explicit,
  named reason — on a non-conforming one. The unconditional checks stay
  unconditional: committed package digest and byte count, package
  byte-reproducibility, seed literals, the pinned §10.9 thumbnail vector, and
  the round-trip test. Explicitly prohibited: regenerating, re-measuring, or
  overwriting any committed 0035 evidence record; changing any published
  budget, digest, or pinned literal; deleting or `t.Skip`-ing a check that
  currently passes on this host; relaxing the `GOTOOLCHAIN=local` requirement;
  and claiming issue 0035 Done, whose upstream gates remain open.
- **Reason:** The gate's stated intent is already "degrade loudly rather than
  false-fail on invocation differences"; it simply guards on the wrong
  predicate. Restoring it is the critical path, because no other pair's work
  can satisfy the acceptance contract while main cannot verify clean.
- **Validation/reversal gate:** Closure of this activation requires
  `./scripts/verify.sh` to exit 0 on this host from the integrated tree, the
  committed evidence bytes to be unchanged in the diff, and a demonstration
  that the gate still fails when a genuine artifact mismatch is introduced.
  Issue 0035 remains `In Progress`. Superseded by contrary owner direction.

> **Comment 2026-08-26 (closure):** ED-0030's item 3 and ED-0031's validation
> gate are both DISCHARGED. Issue 0035's continuation merged as `cdf887a` with
> SBOM refresh `4f29102`, and full main `./scripts/verify.sh` now **exits 0**
> on this macOS host with a clean tree — the recorded baseline exception no
> longer exists and pairs verify against a green main from here. Committed
> evidence bytes are unchanged in the diff, and the genuine-defect
> demonstration ED-0031 required was produced and independently reproduced by
> the reviewer. The worker escalated a residual risk that a forgery edited
> consistently across all seven environment-bearing records would pass; that
> is mitigated in the delivered design, because `recordedHostIdentity()` is a
> reviewed constant restating the Linux fixture and
> `TestRecordedIdentityParsesAndConforms` asserts the records agree with it.
> Evidence-only rewriting therefore fails a test. A coordinated evidence+test
> rewrite remains an ordinary review threat, not a gap in this branch.
> ED-0030's items 1 and 2 (the Go 1.27 `PATH` prefix and the `CLAUDE.md`
> local exclude) remain in force, as does its escalated owner question about
> whether the Linux host stays the measurement authority.

### ED-0032: Route two issue-0028 discoveries to their owning issues

- **Date:** 2026-08-26
- **Status:** Active
- **Affected issues:** 0028, 0011, 0004, 0029, 0018
- **Decision:** Issue 0028's reproduction pass surfaced two defects outside its
  scope. Neither is repaired by the 0028 pair; both are routed with their
  evidence preserved.

  1. **`v2-reordered-delivery` is an issue-0011 conformance defect, not a
     corpus defect and not an unresolved ambiguity.** `w0028` correctly
     refused to decide it, but reported it as a normative ambiguity whose
     value the specification does not settle. On orchestrator review that
     premise does not hold. FORMAT-SPEC §4.7 V2's required projection is
     "final projection equals in-order delivery exactly", not order
     independence, and in-order delivery of a causally-later `setFields`
     yields 150; §4.4 F3 makes canonical order a linear extension of
     causality with ties broken by K; and LWW1 scopes K to "the winning
     write among concurrent candidates", which a causally-later write is
     not. `internal/document/projection_test.go:245`
     (`TestLWWWinnerKeyKeepsGreatestKAcrossCausalWrites`) cites vector V3,
     whose precondition is "neither observes the other", while its own
     `smallWrite` carries frontier `{main:1, y:1}` — X observed Y. The
     delivered implementation at `internal/document/projection.go:721`
     (`winnerSet.dominates`) applies the K comparison unconditionally, so a
     causally-later write with a smaller K loses. Issue 0011 owns the fix and
     owns retiring that test; issue 0004 is asked to tighten LWW1's fourth
     sentence, which reads loosely in isolation but does not override F3 or
     the vector preconditions. Issue 0028 keeps its oracle at 150 and records
     the vector as BLOCKED against this named defect — never as a skip and
     never as coverage.
  2. **`internal/persist/reader.go:468` discards a structural check.** The
     `Load` path calls `checkReservedRanges(rangesByActor)` and throws away
     the returned `*Error`; `internal/persist/inspect.go:206` handles it
     correctly. FORMAT-SPEC §4/§7.6 require structural log checks to precede
     replay. Overlapping or gapped actor ranges still reject, but only
     incidentally, downstream in `document.ReplayFromSnapshot`, with the
     wrong diagnostic. `go vet` cannot see a discarded non-error-typed
     return. Routed to issue 0029, which owns `internal/persist` and the
     fault-injection evidence this needs; the 0028 pair's reproduction is
     attached to that issue.
- **Reason:** Both defects sit in delivered, reviewer-accepted handoffs, and
  both were found by a pair with no mandate to repair them. Routing them
  preserves the ownership and adversarial-review contract instead of letting
  an unrelated pair rewrite conflict resolution or persistence. Recording the
  first as a conformance defect rather than a spec ambiguity matters: an
  ambiguity would licence either value, while §4 is normative for v0.1 and
  fixes this one.
- **Validation/reversal gate:** Issue 0011's continuation must fix the
  projection defect, retire or rewrite the mislabeled test, and prove V2, V3,
  and V7 against the normative vectors before issue 0028 can close
  `v2-reordered-delivery` or issue 0018 can clear release-audit row O. Issue
  0029's continuation must restore the pre-replay structural check with its
  correct diagnostic and a regression. If issue 0004 instead ratifies the
  loose reading of LWW1 sentence 4, this decision is superseded and both the
  corpus oracle and V2 require re-derivation. Superseded by contrary owner
  direction.

### ED-0033: Settle the V2 recording form and route three further findings

- **Date:** 2026-08-26
- **Status:** Active
- **Affected issues:** 0028, 0029, 0005, 0004, 0011, 0018, 0023
- **Decision:**
  1. **`v2-reordered-delivery` recording form.** The worker and reviewer
     reached compatible but differently-shaped requirements; this settles it.
     The vector is recorded as an EXECUTED subtest whose name carries
     `Nonconformant`: it replays for real, observes the value, asserts that
     the divergence from the 150 oracle still exists, and fails loudly
     ("conformance defect resolved; remove the marker consciously") if issue
     0011's fix lands. It logs ED-0032 and both code citations. **No
     `t.Skip`** — the issue 0018 audit's residual-blocker list is grep-able
     and a fresh skip re-opens the argument that audit settled, and a skipped
     body does not run, which defeats the point of executing it. **Binding
     extra condition:** the machine-readable manifest and `EXPECTATIONS.md`
     must carry a status token that is neither `pass` nor `skip`, and the
     coverage-set equality gate must NOT count the vector as satisfied
     coverage. The reviewer verifies this by reading the gate, not the test.
     Absent that condition the change converts a blocker into a green check,
     which is exactly what the audit condemned.
  2. **Scope arbitration on `reader.go:468`.** The issue 0028 reviewer
     accepted the one-line fix as in scope; the orchestrator overrules it and
     the code stays untouched. The reviewer's technical reading is sound, but
     `internal/persist` belongs to issue 0029, the 0028 pair is not scoped to
     produce the fault-injection evidence the fix requires, and finding 3
     below shows the area is not a one-line repair. Patching only the
     discarded error would obscure a defect whose shape is not yet known.
  3. **`frameAndOrder` filters before it checks (new, UNEXECUTED).**
     `internal/document/projection.go` classifies batches against the
     per-actor watermark and builds its `nonSkipped` set BEFORE per-actor
     reserved-range chaining, so a B3 range violation lying entirely below
     the snapshot watermark has no structural check left on the `Load` path,
     while `Inspect` — parse-only, with no watermark logic — still rejects
     it. Ranges straddling the watermark are still caught as torn batches.
     Compounding ED-0032 finding 2, `internal/persist/inspect.go:206`'s own
     comment claims those ranges "fail here with the same malformed-framing
     token the replaying path produces"; that claim is now false twice over,
     so **ED-0023's parity premise for the parse-only inspection seam is
     broken**. The issue 0028 pair correctly left this labelled and
     unexecuted rather than writing the `internal/persist` test that would
     prove it. Routed to issue 0029 with the pair's written witness.
  4. **FORMAT-SPEC §7.6 name/content binding is unsatisfiable as written.**
     Line 950 requires a batch whose embedded `batchId` contradicts its entry
     name to reject as `malformed-framing`, but the §7.6 entry name is
     `ops/<counterBase>-<actorId>.json` and contains no `batchId`, so
     `internal/persist/reader_helpers.go:47` structurally cannot perform that
     comparison. Wording defect routed to issue 0005, which owns §7. No check
     was invented and no code was touched; it does not affect
     duplicate-batch-id materializability.
- **Open normative question routed to issues 0005 and 0004:** is §7.6's
  structural B3 check unconditional over every retained entry, or scoped to
  batches surviving §4.5 I2 watermark classification? `Load` and `Inspect`
  currently answer differently, and finding 3 cannot be resolved either way
  until an owner rules.
- **Reason:** Findings 1 and 2 were live pair disagreements that would
  otherwise have consumed review cycles or been settled by whoever pressed
  harder. Findings 3 and 4 were produced by a pair with no mandate to repair
  them and are preserved with their evidence rather than executed, guessed,
  or dropped.
- **Validation/reversal gate:** Issue 0028's reviewer must confirm in its
  verdict that the coverage gate does not count `v2-reordered-delivery`.
  Issue 0029's continuation owns findings 2 and 3 and must not close them by
  restoring the discarded error alone. If an owner rules that §7.6's check is
  watermark-scoped, finding 3 becomes an `Inspect` over-rejection instead and
  ED-0023's parity language needs amending rather than the `Load` path.
  Superseded by contrary owner direction.

### ED-0034: Route the reader's null tolerance for a required assetRoot array

- **Date:** 2026-08-26
- **Status:** Active
- **Affected issues:** 0028, 0029, 0005, 0018
- **Decision:** Reviewer byte inspection during issue 0028's continuation found
  that every thin package's `domowik.json` carried `"assetRoot":null`,
  including the `golden/packages/thin-valid.domo` SUCCESS fixture and all 33
  hostile bases derived from it. FORMAT-SPEC §7.4 line 852 lists `assetRoot`
  as required with type "Ascending-by-digest array enumerating U(P)", and
  `null` is not an array, so a SUCCESS oracle rested on non-conforming bytes
  and every hostile base carried a second latent defect beside the one it
  meant to isolate. The dev-only corpus generator is fixed to emit `[]`;
  **product code is deliberately untouched.**
  `internal/persist/wire.go:85-88` explicitly exempts `null` from the
  type-check with the comment "generators may serialize an absent table as
  null". That reasoning is circular — parser laxity cannot make a fixture
  conforming — and the issue 0028 pair correctly routed it rather than
  changing the reader. Two routings follow:
  1. **Implementation, to issue 0029:** decide and implement whether
     `wire.go` should reject `null` for the required `assetRoot`, with a
     regression fixture. It must not be closed by loosening the corpus back.
  2. **Category, to issue 0005:** §7.4 is unambiguous that the field is a
     required array, so no wording change is implied, but the acceptance
     CATEGORY for a `null` value is an owner call — `reject` under §9.6, or
     `warning` with an absent-table reading. Issue 0029's fix direction
     depends on that ruling.
- **Reason:** This is the fourth defect this pair has found in delivered code
  and the second where a fixture's expected outcome rested on bytes that do
  not conform. Fixing the generator strengthens the corpus and changes no
  expected category or outcome — verified across the branch. Changing the
  reader in the same breath would have been an unreviewed product decision
  made by the pair least equipped to evidence it.
- **Note on method:** the gate added to pin this shape,
  `TestGeneratedManifestsAreConforming`, was VACUOUS in its first version — it
  asserted only that `assetRoot` unmarshalled into a slice, and
  `encoding/json` unmarshals a literal `null` into a slice without error, so
  it passed under the exact mutation it existed to catch. Its own mutation
  probe caught that. This vindicates the ED-0033 ruling that the probe record
  belongs durably: a gate that "looks correct, would review correct, and
  asserts nothing" is invisible to code review and visible to mutation.
- **Validation/reversal gate:** Issue 0018 may not clear release-audit row A
  or row H while `null` remains acceptable for a required array. If issue
  0005 rules `null` acceptable as an absent-table reading, the corpus keeps
  `[]` regardless and the wire comment must be rewritten to cite that ruling
  rather than generator behavior. Superseded by contrary owner direction.

### ED-0035: Activate issues 0011 and 0004 to close the ED-0032 conformance defect

- **Date:** 2026-08-26
- **Status:** Active
- **Affected issues:** 0011, 0004, 0028, 0018, 0029, 0005
- **Decision:** With issues 0035 and 0028 integrated and main verifying clean,
  both slots under ED-0028's two-pair cap are filled with the pair of issues
  that jointly close ED-0032, the most severe open finding: a delivered,
  reviewer-accepted Phase-1 implementation contradicting a normative
  specification section.
  1. **Issue 0011 — implementation.** Fix
     `internal/document/projection.go` so the LWW1 winner-key comparison
     governs only concurrent candidates, leaving causally ordered writes to
     canonical order per §4.4 F3. Retire or rewrite
     `TestLWWWinnerKeyKeepsGreatestKAcrossCausalWrites`, which pins the defect
     and cites vector V3 while its own `smallWrite` frontier `{main:1, y:1}`
     violates V3's "neither observes the other" precondition. Prove V2, V3,
     and V7 against the normative vectors. Issue 0028's V2 witness is
     **expected to fail** once this lands: it is written to fail loudly with
     "conformance defect resolved; remove the marker consciously". Removing
     that marker and the `executionBlocked` entry in `EXPECTATIONS.md` is part
     of this issue's work, and the coverage audit must then count V2 as real
     coverage.
  2. **Issue 0004 — specification.** Tighten FORMAT-SPEC §4.6 LWW1's fourth
     sentence, which reads loosely in isolation, so it cannot be read as
     overriding F3 or the vector preconditions. Answer the §4.5 I2 half of
     ED-0033's open normative question: is §7.6's structural B3 check
     unconditional over every retained entry, or scoped to batches surviving
     watermark classification? Issue 0005 owns the §7.6 half and issue 0029's
     fix direction depends on both.
- **The normative outcome is already settled and is not reopened.** ED-0032
  ruled that §4.4 F3, LWW1's "concurrent candidates" scoping, and §4.7 V2's
  "final projection equals in-order delivery exactly" jointly determine 150.
  Issue 0004 writes that reading into the text; issue 0011 implements to it.
  Neither pair may relitigate the value. A pair that believes the ruling is
  wrong escalates to the orchestrator rather than encoding a different answer.
- **Reason:** These two are the only issues whose outputs close ED-0032, they
  are complementary rather than overlapping — `docs/FORMAT-SPEC.md` versus
  `internal/document` — and both are gated by a ruling already made, so
  concurrent execution carries little conflict risk. ED-0032 blocks issue
  0028's `v2-reordered-delivery` and issue 0018's release-audit row O, and
  every further release-audit remediation inherits the ambiguity until the
  text is tightened.
- **Deferred, with reasons:** issue 0005 (§7.6 wording defect, the `assetRoot`
  acceptance category, and the §7.6 half of the watermark question) and issue
  0029 (three routed defects) are next in the queue. Issue 0029 is deliberately
  behind both, because ED-0033's fix direction for its `frameAndOrder` finding
  cannot be chosen until 0004 and 0005 have each ruled on their half.
- **Validation/reversal gate:** Issue 0011 may not close while its own
  upstream gates under ED-0018 remain open; its continuation closes only when
  V2/V3/V7 pass against the normative vectors, the mislabeled test is gone,
  issue 0028's V2 marker is consciously removed, and full
  `./scripts/verify.sh` exits 0. Issue 0004 may not mark §4 sections normative
  beyond their current scope. Superseded by contrary owner direction.

> **Comment 2026-08-26 (two orchestrator corrections, raised by `r0011`):**
>
> 1. **The decision-input artifact gets a tracked home.** The issue 0011 brief
>    said "no `docs/evidence/` byte changes", and the reviewer correctly
>    observed that adding a file is still a byte change. That prohibition was
>    written for issue 0035, where the risk was *rewriting* committed
>    measurement provenance; carrying it verbatim into issue 0011 made it
>    forbid something it was never aimed at. It is void as applied to new
>    files. The candidate analysis is authorised at
>    `docs/evidence/0011-conflict-order-candidates.md`, on the same reasoning
>    as ED-0033: the scratchpad is session-scoped, and this artifact is the
>    decision input for a normative change that issues 0004, 0029, 0031, and
>    0018 will each need to cite. Existing committed evidence stays
>    byte-unchanged — that half of the prohibition stands. Precedent for
>    recording an unsettled outcome under `docs/evidence/` is
>    `0030-numerical-model/RATIFICATION.md`.
>
> 2. **My characterization-test instruction is withdrawn.** I told the worker
>    it "may write characterization tests that assert current behavior". The
>    reviewer refused them, on the grounds that pinning today's non-conforming
>    rule in a test is exactly the failure that produced this continuation —
>    `TestLWWWinnerKeyKeepsGreatestKAcrossCausalWrites` is a characterization
>    test that outlived the reasoning behind it and then supplied false
>    authority to the next reader. That is correct and the instruction is
>    void. Same-actor, same-batch, and precondition cases belong in the
>    candidate model and in a future acceptance checklist until the semantics
>    are settled; no new test may encode current conflict-resolution behavior.
>
> Also endorsed from the same review: the max-K causal-barrier candidate must
> be enumerated alongside the others; old-package facts must be exact values
> and keys rather than a summary; ordinary apply write-back dropping covered
> retained payloads, and L1xL2 composition with atomic multi-op batches, are
> in scope for the analysis. The observation that **frontier closure is not
> reader-provable after compaction without retained or certified ancestry** is
> flagged as potentially a second elimination argument: if causal relationships
> cannot be established by a reader of a compacted package, a causality-based
> resolution rule may be unimplementable for exactly the packages that
> compaction produces. If that holds, it bears directly on the owner
> escalation ED-0036 anticipates.

### ED-0036: Halt the issue 0011 fix; §4.4 F3's canonical order is underdetermined

- **Date:** 2026-08-26
- **Status:** Active
- **Affected issues:** 0011, 0004, 0028, 0029, 0018
- **Decision:** Issue 0011's adversarial reviewer blocked the proposed fix
  **before any edit**, with a counterexample showing that a causal fold
  retaining the value-writer's key trades the V2 repair for a V5/V6 divergence.
  Orchestrator review confirms the counterexample and finds a deeper defect
  that ED-0032 did not reach: **FORMAT-SPEC §4.4 F3 does not define a
  canonical order at all for this shape.**

  F3 says batches "apply in a CANONICAL ORDER, any linear extension of the
  causality relation with ties broken by ascending key K". Take the reviewer's
  shape: writes R, P, D to one field with K(P) < K(D) < K(R), where P is
  causally after R and D is concurrent with both. The tie-break rule orders
  the incomparable pairs D before R and P before D; causality orders R before
  P. Those three precedences form the cycle R → P → D → R, so **no ordering
  satisfies the rule.** Mechanically verified. Distinct valid linear
  extensions also yield distinct projections here — `D,R,P` gives P while
  `R,P,D` gives D — which contradicts F3's own claim that the canonical order
  is a function of the delivered SET, and contradicts V6's requirement that a
  snapshot projection equal full-history replay.

  This is the root cause of ED-0032. The delivered implementation's
  unconditional greatest-K rule is one way to make the model well-defined — a
  total order by K — which is self-consistent but violates V2. There is no
  implementation that satisfies §4 as currently written.

  Consequently:
  1. **Issue 0011's semantic change is HALTED** under ED-0004: no
     implementation against an unsettled specification. It does not resume
     until issue 0004 rules. Its pair stays live and is repurposed to produce
     the decision input described below.
  2. **Issue 0004's scope is expanded and becomes the critical path.** It must
     define the canonical-order construction precisely rather than by the
     current unsatisfiable phrasing; reconcile LWW1's "concurrent candidates"
     scoping with that construction; and state what snapshots must retain
     under S3/LWW5 so that V5 and V6 hold, since the reviewer's counterexample
     shows a single retained winner key per field is insufficient. Every §4.7
     vector's required outcome stays fixed; if the construction cannot satisfy
     one, that is an escalation to the owner, not an edit.
  3. **ED-0032's ruling stands and is narrowed in force.** Causality governs
     and K breaks ties among concurrent writes only; V2's outcome remains 150.
     What ED-0032 did not settle — and could not, because it is a defect in
     F3 rather than LWW1 — is how concurrency is resolved against a compacted
     snapshot. Nothing about the V2 value is reopened.
- **Reason:** The review contract worked exactly as intended: an adversarial
  reviewer stopped a plausible fix before a line was written, on evidence, and
  the escalation exposed a normative defect that three prior waves and two
  accepted handoffs had not. Implementing any of the candidate rules now would
  encode an unreviewed conflict-resolution semantics into a released format.
- **Note on a bad brief condition (orchestrator error):** ED-0035 required the
  issue 0028 coverage audit to "count V2 as real coverage" once the fix lands.
  The reviewer showed that is not achievable as stated — the record carries a
  null category and no `coveredBy`, so deleting its `blockedIDs` and
  `residualBlockers` entries would make it vanish rather than become counted,
  which the brief separately forbids. That condition is void. Its replacement:
  V2 must end up with a real category and `coveredBy` entry and pass the audit
  as ordinary executed coverage, and whoever lands the fix must show the audit
  positively counting it.
- **Validation/reversal gate:** Issue 0011 resumes only when issue 0004's
  construction is accepted and its snapshot-retention requirement is stated.
  Any candidate construction must be checked against the reviewer's shape and
  against old packages written by the current code, whose retained winner keys
  may not carry what the new rule needs. If no construction preserves every
  §4.7 vector, escalate to the owner rather than weakening a vector.

### ED-0037: Owner ruling — adopt causal-position ordering; LWW1's tie-break narrows

- **Date:** 2026-08-26
- **Status:** Active
- **Decided by:** project owner, on an escalation from the orchestrator
- **Affected issues:** 0004, 0011, 0028, 0029, 0005, 0018, 0031

#### READ THIS FIRST — current effective position

This decision carries seventeen dated comments, several of which withdraw
earlier ones. The originals are preserved verbatim per the amendment rule, so
a reader working chronologically will pass through claims that were later
retracted. **Cite this summary, not the superseded text.**

In force as of the last amendment:

| Question | Current answer | Where |
|---|---|---|
| Canonical order | ascending write position **`(causalPastSize, counterBase, actorId, operationIndex)`** | body + (b) |
| Winner for a field | greatest write position among writes to it | body |
| K's role | separates only equal-past-size writers | body |
| Frontier closure | **normative authoring requirement**, incl. the own-actor entry; unprovable after compaction and must say so | (e), soundness-premise comment |
| Snapshot must retain | winner positions, **creation positions**, **delete positions**, and per-entity live intervals | (f), (l) |
| Late-batch placement | **three branches** — before create / within live interval / after delete | (l) |
| `causalPastSize` arithmetic | exact wire arithmetic and bounds still owed by issues 0004 and 0005 | (g) |
| Retention size | the **number** of positions is bounded by state (O(entities+fields)) so the boundedness trigger does **not** fire; the **bytes** are **not** bounded by state — the `batchPositions` table is O(positions x actors) — L23 bounds the total, and a document whose metadata will not fit L23 cannot compact, observably under S6 | (m) as corrected by (s) and ED-0039 |
| Residual I5 class | not fixable by retention; resolved by **ED-0039** tightening S6's drop condition | (o), (q), ED-0039 |
| Legacy single-actor packages | **not lossless**; readable only under **ED-0038**'s convention, with V5 exactness disclaimed | (k), ED-0038 |
| Legacy multi-actor packages | fail-closed unless the full relevant write history for a field is retained | (d) |
| Aggregate verification counts | soundness evidence for the adopted rule only, never proof an alternative fails | (n) |

**This summary went stale once, on 2026-08-26, and the correction is instructive.**
Its retention row read "O(entities + fields); the boundedness trigger does not
fire" — comment (m)'s wording — after comment (s) and ED-0039 had already
narrowed it to *number bounded, bytes not*. It was caught by issue 0004's
reviewer during a citation spot-check, not by its author. The block written to
stop readers citing superseded claims had itself become one, by the same
mechanism it exists to catch: an amendment added to the body while its
restatement above was left standing. **Anyone amending this decision must
update this block in the same commit.**

Superseded — do not cite:

- item 4's `causalPastSize = operationCounter - 1` derivation → corrected by **(a)**; it is now
  ED-0038's adopted *convention*, not a derivable fact.
- the three-component `P` in the decision body → refined by **(b)**.
- (d)'s "single-actor legacy packages are unaffected" → withdrawn by **(k)**.
- (e)'s predecessor, "omitted own-actor frontier entry is benign" → wrong across
  actors; see **(e)** itself.
- (o)'s authorisation of the S6 extension as an orchestrator scope decision →
  withdrawn by **(q)**; S6 is now owner ruling **ED-0039**.

- **The impossibility that forced the choice.** Issue 0004's pair proved, and
  the orchestrator verified, that no construction can hold all three of:
  (a) LWW1's greatest-K among causal maxima; (b) LWW2's discarding of losing
  values, equivalently bounded per-field snapshot metadata; (c) V5 and V6's
  snapshot/full-replay equivalence. The witness is issue 0004's reviewer's own
  attack: `Q(K3)` concurrent with `D(K2)` so Q wins; a later `W(K1)` observing
  only Q supersedes Q, at which point the correct winner becomes `D` — a value
  the replica discarded when Q beat it. 768 of 5256 keyed 4-write shapes
  exhibit that dethroning. ED-0036's cycle is likewise generic, not one shape:
  6 of 114 keyed 3-write and 888 of 5256 keyed 4-write shapes cycle under the
  literal pairwise reading.
- **Decision:** Adopt **ordering position P = (causalPastSize, counterBase,
  actorId)**, where `causalPastSize` is the number of operations in the batch's
  causal past — the sum of its frontier counters, which B3's contiguous ranges
  make exact. Canonical order is ascending P; a field's winner is the greatest
  P among writes to it. The owner accepted the consequence that **K no longer
  decides between two concurrent writes whose causal-past sizes differ**: past
  size decides first, and K breaks ties only among equal-past-size writes,
  which are necessarily concurrent — so ED-0032's "K breaks ties among
  concurrent writes only" is satisfied exactly. Measured deviation from LWW1
  as written: 15.8% of keyed 3-write and 24.7% of keyed 4-write shapes. In
  user terms: when two people edit the same field without seeing each other,
  the edit from whoever had seen more of the document's history wins, and the
  counter only separates writers who had seen equally much.
- **Why this construction and not another.** P is a **sort key, not a set of
  pairwise constraints**, so ED-0036's cycles are structurally impossible
  rather than merely avoided. P is strictly monotone along causal edges, so
  max-P respects causality and V2 holds. Because the winner is `max` over a
  total order and max is a monotone fold, a snapshot needs exactly **one
  position per field** and late arrivals fold in identically to full replay —
  which is what buys V5 and V6 without retaining losing values. The
  orchestrator independently confirmed monotonicity, the fold property over
  200,000 random coverage splits with zero divergence, and the outcomes for
  V2, V3, the Q/D/W dethroning shape, and ED-0036's cycle shape. Issue 0004's
  pair verified 0 linear-extension violations and 0 snapshot-versus-full
  divergences across all 114 keyed 3-write and 5256 keyed 4-write shapes over
  every causally closed coverage subset, plus 6000 random multi-actor
  executions and roughly 36,000 random coverage splits. **P adds no batch
  field** — it is derived from the already-required frontier — so old batches
  stay byte-readable and no §8.2 bump attaches on batch account. Causal height
  and Lamport stamps were rejected for requiring a new field.
- **What does not move.** Every §4.7 vector outcome stays exactly as written,
  including V2 at 150. This is not a vector edit. LWW2's discarding of losing
  values is preserved, as is §4's not-a-CRDT, lossy positioning.
- **Consequences, owned elsewhere:**
  1. **Issue 0011** implements P, not a causal fold; its snapshot state needs
     one widened position per field rather than a key plus loser values. Its
     ED-0036 halt is lifted once issue 0004's text is accepted.
  2. **Issue 0028** has corpus work: `golden/snapshots/snapshot-full-s1-s5.json`,
     `adversarial/vectors/v5-late-concurrent-after-compaction.json`, and
     `EXPECTATIONS.md:448` carry the old key shape and cannot derive the
     leading component. Its V3 fixture must either use equal-past-size writes
     or have its expected value re-derived, since V3's stated setup says only
     "neither observes the other" and is silent on past size.
  3. **§8.2 migration duty does not attach.** Issue 0018's release audit
     (`ccfe953`) established that no independently evidenced predecessor
     release exists and the predecessor set is empty, and ED-0026 forbids
     claiming v0.1 released. The S3 shape change therefore lands before any
     public release. Issue 0005 still records that finding in §8 rather than
     leaving the question open.
  4. **ED-0031 collision cleared.** Issue 0004 opened both protected evidence
     packages: `empty-v1.domo` and `unfurnished-1000-v1.domo` are single-actor
     with zero ops entries, and single-actor snapshots derive
     `causalPastSize = operationCounter - 1` exactly, so their bytes stay
     conforming and no 0035 evidence needs regenerating.
> **Comment 2026-08-26 (soundness premise, from issue 0011's frontier
> analysis):** P has **no derivability gap**. Every §7.6 batch entry carries
> its own frontier object and `internal/persist/reader.go:459` already
> reconstructs it, so `causalPastSize` is one pass over that map, computable
> from a single entry with no ancestry and no certification — in compacted
> packages as much as uncompacted ones. Verified. The ruling does not return
> to the owner on that account.
>
> But P is strictly monotone along causal edges **only if frontiers are
> transitively closed**, and nothing requires or checks that:
> `internal/document/op.go` `validateFraming` rejects a zero actorId and a
> negative counter and stops. So `b1 = actor A, base 1, frontier {B:5}`
> (P = (5,1,A)) and `b2 = actor A, base 2, frontier {}` (P = (0,2,A)) are both
> accepted, and b2 sorts before its own predecessor. An omitted **own-actor**
> entry is benign, because `counterBase` is P's second component so an actor's
> own stream still sorts in counter order — a real robustness property of P
> over a Lamport-style stamp. An omitted **other-actor** entry under-reports
> past size and inverts a genuine causal pair, giving a deterministic but
> wrong order. §4.3 line 416 and §9.4 already require an inconsistent frontier
> to reject as `malformed-framing`; what is missing is that nothing implements
> it and, after compaction, no reader can verify it, since closure is
> checkable only when the observed ancestors are also retained.
>
> Issue 0004's text must therefore state frontier transitive closure as a
> **normative authoring requirement** in §4.4, require validation at authoring
> time and full-log validation wherever ancestry is retained, and record
> explicitly that snapshot-only validation cannot prove it. Without that
> clause the adopted construction rests on an unstated premise and is unsound.
>
> **Legacy-package disposition (accepted):** single-actor legacy packages read
> as-is losslessly, since a single-actor snapshot frontier gives
> `causalPastSize = operationCounter - 1` exactly. Multi-actor legacy packages
> whose winner keys are not position-derivable are **rejected fail-closed**,
> migrating only fields whose writing batch is still retained; reading them
> as-is would silently preserve the exact nonconformance ED-0032 identified.
> Detection needs no version marker: a multi-actor snapshot frontier plus a
> winner key not covered by a retained batch is a byte-level fail-closed rule,
> and the widened `winnerKeys` field is itself the marker separating new
> packages from same-labelled 0.1 development packages.
>
> **Measured fact that makes this cheap:** one ordinary save-after-load cycle
> discards the entire operation history. `Save` writes a State whose frontier
> covers every retained batch, so on reload `ReplayFromSnapshot` skips them all
> under I2 and none enters `Projection.log`; the apply write-back then writes
> `RetainedBatches: proj.Log()` (`internal/cli/apply.go:415`), so the next
> package carries zero retained batches. Per-field migration from the retained
> log is therefore possible only for a package unsaved since its ops were
> written, making the non-derivable case the common one — and refusing to
> compact unresolved multi-actor history would protect nothing.
>
> **Routed, not fixed:** `internal/cli/apply.go:415` rebuilds from
> `proj.Log()` while `internal/persist/smoke_test.go:62` rebuilds from
> `view.Retained`. The two write paths disagree. Recorded against issues 0027,
> 0029, and 0031.

> **Comment 2026-08-26 (two corrections to this decision as recorded):**
>
> **(a) Item 4's derivation claim was over-strong, and it was mine to record.**
> I wrote that "single-actor snapshots derive `causalPastSize =
> operationCounter - 1` exactly". That holds only for SINGLE-OPERATION
> batches. A multi-operation batch has past size `counterBase - 1`, and the
> old S3 key carries neither `counterBase` nor the operation index, so the
> leading component is not recoverable; `unfurnished-1000-v1.domo` retains
> zero ops entries, so how its 1000 operations were batched is not knowable
> from the snapshot at all. What actually survives is weaker and sufficient:
> for a single-actor snapshot the winner IDENTITY is still correct under P,
> because within one actor a greater operation counter always means a greater
> position — a later batch has strictly greater past size, and a later index
> in one batch has a greater position — and that holds whether or not the
> own-actor frontier entry is present. **ED-0031's conclusion stands** (no
> issue 0035 evidence needs regenerating) but for that weaker reason: those
> packages name the right winners and cannot place them against a future
> concurrent writer from another actor.
>
> **Disposition of the two ED-0031-protected packages (orchestrator):** they
> are left as **pre-ED-0037 development artifacts**, explicitly carrying no
> conformance claim, and are NOT re-labelled or regenerated. They exist to
> measure size, not to assert conformance; regenerating them would invalidate
> issue 0035's measured budgets and would require the Linux reference host,
> which ED-0030's open owner question already covers. Under issue 0004's new
> S3 text a snapshot omitting the leading component is non-conforming, readers
> MUST NOT infer it, and V5's guarantee is reported unavailable for the
> affected fields rather than guessed — so nothing silently treats these two
> as conforming.
>
> **Consequence routed to issues 0035 and 0018:** widening S3 winner keys adds
> bytes to every snapshot, so issue 0035's Part A measurements
> (`empty-v1` 3,597 B against 16 KiB; `unfurnished-1000-v1` 119,394 B against
> 256 KiB) are measurements of the PRE-ED-0037 format. They must be re-measured
> once the widening lands, and issue 0018 may not publish them as v0.1 budgets
> without that re-measurement. The headroom is large, so this is a
> record-keeping obligation rather than an expected budget failure.
>
> **(b) P needs one more component to be implementable.** Issue 0004's
> reviewer proved this decision's P is incomplete for multi-operation batches.
> With equal past size, batch A (`base=1, len=3`) writing the field at index 2
> and concurrent batch B (`base=2, len=1`) writing it: batch positions
> `(s,1,A) < (s,2,B)` order A before B, so B's write is last in canonical
> order — but reading "greatest P among writes" with the operation COUNTER as
> discriminator gives `(s,3,A) > (s,2,B)` and picks A's write, contradicting
> the canonical order, and lets operation positions interleave batch ranges
> where B1 forbids it. Verified. The write position is therefore this
> decision's P refined by the operation's INDEX, not its counter:
> **`(causalPastSize, counterBase, actorId, operationIndex)`**. The leading
> three components are this decision's P verbatim, and because they are
> constant within a batch, all of a batch's writes sort contiguously and
> interleaving is structurally impossible. This is the ruling read literally
> plus the discriminator it needs when two writes share a batch; it changes no
> cross-actor semantics the owner ruled on, and same-batch ordering by index
> is what LWW1 already requires of one actor's ordered writes. Verified by
> issue 0004's pair over 7,676 random multi-actor executions with 1–3
> operation batches, with zero failures on batch-order-is-a-linear-extension,
> no-interleaving, write-order-agrees-with-batch-order,
> winner-equals-last-write-in-canonical-order, and
> snapshot-fold-equals-full-replay.

> **Comment 2026-08-26 (issue 0011's reviewer, two further tightenings):**
>
> **(c) The per-write-past completion is REJECTED.** Faced with the same
> batch-versus-write gap, a third completion is available: make the leading
> component itself per-write, `past = sum(frontier) + operationIndex`. Its
> appeal is that it restores `causalPastSize = operationCounter - 1` for the
> single-actor case. It is rejected because it breaks B1. Verified: batch A
> with frontier sum 0 and three operations yields per-write pasts 0,1,2, while
> a concurrent batch B with frontier sum 1 yields past 1, so B's write sorts
> *between* A's — `(0,1,A,0), (1,1,A,1), (1,2,B,0), (2,1,A,2)` — interleaving
> batch ranges where B1 forbids it. The adopted refinement in (b) keeps the
> leading three components constant within a batch, so a batch's writes sort
> contiguously and interleaving is structurally impossible. Correction (a)'s
> weaker survival property — winner identity holds within one actor, the
> leading component does not survive — stands, and is preferred over
> recovering a derivation at the cost of atomicity.
>
> Issue 0011's reviewer independently confirmed (a) from the generator source:
> `internal/sizecorpus/scenario.go:325-345` builds `unfurnished-1000-v1` as
> ONE batch at `counterBase` 1 with a nil frontier carrying all object create
> operations, so every write has `causalPastSize` 0 while the snapshot keys
> carry operation counters 1..N. `empty-v1` is a single operation and is
> unaffected. With zero retained ops the package bytes neither preserve nor
> prove that batch boundary.
>
> **(d) Legacy migration needs the full relevant write history, not the old
> winner's batch.** The disposition recorded above said fields whose writing
> batch is still retained may be migrated. That is insufficient, and the
> reason is this decision's own dethroning argument applied to migration:
> retaining the OLD max-K winner's batch derives only that candidate's new
> position. It cannot exclude a different write — lower K, greater P — that
> lost under the old rule, was therefore never retained as a winner, and whose
> value is gone. Under P that write may be the new winner. A field is
> therefore exactly migratable only when the **full relevant write history for
> that field is retained**, or a certificate attests it; otherwise the
> migration is fail-closed. Combined with the measured fact that one ordinary
> save discards the entire operation history, this means multi-actor legacy
> packages are in practice non-migratable rather than partially migratable.
> Single-actor legacy packages are unaffected: with no concurrency there are no
> candidates to re-rank, so the old winner remains the winner.

> **Comment 2026-08-26 (both reviewers, five further corrections — three of
> them to things this record previously asserted):**
>
> **Corroboration first.** Issue 0004's reviewer independently verified the
> scalar P on all naturally labelled causal posets through n=5 — 42, 960, and
> 42,840 keyed shapes — with zero linear-extension failures and zero max-fold
> snapshot failures **under closed frontiers**. That is independent of issue
> 0004's own verification and of the orchestrator's.
>
> **(e) "Omitted own-actor entry is benign" is WRONG, and the orchestrator
> propagated it.** It is benign only for an actor's own stream order, where
> `counterBase` preserves the edge. It is NOT benign across actors: dropping
> the own-actor entry under-reports past size by that actor's whole prior
> stream, which can invert a cross-actor comparison. Verified — a batch whose
> closed past is 45 (own 40 plus observed 5) reports 5 when the own entry is
> omitted, and loses to a concurrent batch at 30 that it should beat. §4.4's
> F4 must therefore require the own-actor entry like any other and reject its
> omission as a closure violation; the only accurate residual claim is that
> own-stream ORDER survives it, not that the omission is harmless.
>
> **(f) max-P per field does not prove V6 on its own.** The per-field max fold
> is correct for field-level LWW but does not capture V7/I5 **whole-batch**
> staleness. Counterexample: a snapshot holds entity E (created by batch C)
> and field F; a late multi-operation batch M with lower P canonically
> precedes C and both sets E and sets F. Full replay discards M entirely as
> stale, because E does not exist when M applies, so F is unchanged. A
> seeded replay sees E present, suppresses M's write to E by winner position,
> but may still apply M's write to F — diverging. The snapshot must therefore
> retain the entity **creation position** and use it for whole-batch stale
> placement. `createdKeys` is already retained (`internal/persist/wire.go:210`,
> `:271`) as `{operationCounter, actorId}`, so it needs the same widening to a
> position that `winnerKeys` does. This intersects issue 0004's existing
> CreatedKeys omission.
>
> **(g) `causalPastSize` has no safe representation in the current types.** A
> frontier may carry L18 = 65,536 members (§9.3 row L18), each an int64
> counter, so the exact sum approaches 2^79 while the parser and key types are
> int64. Verified. Wrapping or saturating would change ordering, which is the
> one thing P may not do. Issues 0004 and 0005 must define exact wire
> arithmetic and bounds for the leading component rather than leaving it to
> the implementation.
>
> **(h) The "one ordinary save discards the entire history" headline was too
> broad.** `internal/cli/apply.go:396-400` performs no write at all when there
> are no fresh batches. A real apply starts from a reloaded projection whose
> `Log()` is already 0, adds fresh batches, and saves those; it drops all
> PRIOR covered retained entries, not the operation history it just created.
> The compatibility loss is still severe and the migration conclusion is
> unchanged, but the claim as recorded overstated it and any probe must model
> one fresh batch and track old batch ids explicitly.
>
> **(i) Issue 0004's text must follow this record, not soften it.** Its draft
> reported V5 merely "unavailable" for a legacy omission where comment (d)
> above requires the migrate-only-with-full-relevant-history / fail-closed
> rule. The stricter rule governs. Separately, S3's optional shared frontier
> currently has no exact interoperable shape and needs one.
>
> **(j) Authority for the four-component write position is already recorded**
> in comment (b) above (commit `fc00f21`), which issue 0004's reviewer asked
> for. It refines this decision's P rather than altering the semantics the
> owner ruled on.

> **Comment 2026-08-26 (k) — "single-actor legacy is lossless" is WRONG, and
> it was the orchestrator's acceptance.** Comment (d) said single-actor legacy
> packages are unaffected because there is no concurrency to re-rank. That
> preserves the CURRENT VALUE but not late-concurrent semantics, and V5 is
> about exactly the latter. Issue 0011's reviewer showed the ambiguity is
> information-theoretic: two different histories serialize to **identical
> bytes** once logs are dropped. H1 has actor A create N entities in one
> `base=1` multi-operation batch — which is what the `unfurnished` generator
> actually does. H2 has the same creates as N one-operation batches at
> counters 1..N. Both yield the same entities, the same frontier `{A:N}`, the
> same winner and created keys `(c,A)`, and an empty retained log. Under the
> four-component write position, operation `c` is at `(0,1,A,c-1)` in H1 but
> `(c-1,c,A,0)` in H2. Verified: for N=5 and the winner at counter 4, a later
> concurrent writer whose own past size is 0, 1, or 2 **beats H1 and loses to
> H2** — opposite outcomes from bytes no reader can tell apart.
>
> So a single-actor legacy snapshot preserves the current value and the winner
> identity, but cannot reproduce V5's late-concurrent resolution, and
> read-as-is may not be described as lossless. Being single-actor is not
> itself proof of conformance, which also means the two ED-0031-protected
> evidence packages cannot be shown conforming merely from having one actor —
> consistent with their recorded disposition as pre-ED-0037 development
> artifacts carrying no conformance claim, but for a stronger reason than
> recorded there.
>
> **Escalated to the owner** rather than decided here, because the options
> have real and opposed costs: rejecting legacy packages outright would make
> the two committed issue-0035 evidence packages unreadable and break
> `TestCommittedPackagesRoundTrip`, whose repair needs the Linux reference
> host that ED-0030's open question still covers; while adopting a reading
> convention bakes an arbitrary-but-deterministic choice into the format.

> **Comment 2026-08-26 (l) — the tombstone side of (f), and the boundedness
> test that decides whether it reaches the owner.** Widening creation
> positions alone does not preserve V6/I5. Issue 0004's reviewer's symmetric
> case: entities E and F live; `M = [set E, set F]`; `D = delete(E)`; with
> `create(E) < P(M) < P(D)`. Full replay applies M — E is live at M's position
> — so F changes, and D then tombstones E. A snapshot covering D holds E
> tombstoned, so a late M is discarded whole as stale and F never changes.
> Divergence. Verified structurally: the S4 tombstone wire record carries only
> `deletingKey` (`internal/persist/wire.go:304-306`) in the narrow
> `{operationCounter, actorId}` shape, so neither the delete position nor the
> live interval is reconstructable.
>
> **This is a completeness requirement, not a semantics change, and V6 does
> not move.** §4's S4 delete keys widen to positions like `winnerKeys` and
> `createdKeys`, and a tombstone must additionally carry the creation position
> so the live interval `[createPosition, deletePosition)` survives deletion —
> the implementation currently treats `createdKeys` as live-entity-only. Late
> batch placement then classifies three ways rather than two: **before
> create** → stale, discard whole; **within the live interval** → applies,
> with writes to the deleted entity's own fields subsequently superseded per
> LWW3; **after delete** → stale per I3, no resurrection.
>
> **When this becomes an owner question, and only then.** The retention added
> so far is bounded — two positions per entity and one per field, the same
> order as the state itself — so it is a constant-factor snapshot increase,
> not an impossibility, and issue 0004 states it without escalating. The open
> item is the reviewer's note that payload or schema validation of a
> canonically pre-delete write may need further retained state. **If the pair
> establishes that the retention required to satisfy V6 under late concurrency
> is not O(entities + fields), that is the trigger to escalate**, because at
> that point preserving V6 as written would cost unbounded snapshot growth and
> the owner would have to choose between V6's scope and compaction's purpose.
> Until then, no vector moves and no owner ruling is sought.
>
> The snapshot-size consequence compounds the one already routed to issues
> 0035 and 0018: three widened position maps plus per-entity lifetime metadata
> make the pre-ED-0037 Part A budgets further out of date, without changing
> the conclusion that they must be re-measured rather than re-derived.

> **Comment 2026-08-26 (m) — the boundedness test did NOT fire; that
> escalation path is closed.** Issue 0004 measured the retention required for
> V6 under late concurrency at `2 x entities + fields + batchTable`, with the
> batch table bounded by distinct referenced batches and so by the total
> position count — a constant factor on the state, never a function of history
> length. Comment (l)'s trigger therefore does not apply and no owner ruling is
> sought on retention size. The reviewer's payload/schema note also resolves
> without extra retention: a pre-delete write whose VALUE references the
> later-deleted entity needs nothing more, because both paths reach the same
> state — full replay writes `F.ref=E` while E is live and then tombstones E,
> and a seeded replay applies M inside the live interval against an
> already-tombstoned E and stores the same `F.ref=E`. Replay-time placement is
> presence-only under I5, O4 validation is authoring-time, and the
> dangling-reference disposition remains O4/BEHAVIOR-SPEC's, unchanged by
> compaction.
>
> **(n) A methodological finding worth more than any single defect here.**
> Issue 0004 disclosed, unprompted, that the reviewer's tombstone case did not
> merely extend comment (f) — it FALSIFIED a fix issue 0004 had already
> verified. With creation and deleting positions retained but a TWO-branch
> rule, a seeded replay produced `{E.f1: M#0, F.f1: M#1}` where full replay
> gives `{F.f1: M#1}`: it placed the batch correctly and then materialised M's
> write to E, resurrecting a field on a tombstoned entity. Its 40,342-split
> aggregate did not catch it, because its generator never produces that shape
> in the non-exposed class — so the aggregate shows the FINAL variant sound at
> zero divergences while failing to discriminate it from the broken one.
>
> The rule this establishes, and it applies to every verification in this
> decision including the orchestrator's own 200,000 random coverage splits:
> **large random aggregates are soundness evidence for the rule actually
> adopted, never coverage proof that alternatives fail.** A hand-built
> adversarial witness is the discriminating instrument. Where this record
> cites aggregate counts, they carry that weaker meaning. This is exactly what
> the adversarial pairing exists to produce, and it was surfaced by the author
> against its own evidence rather than by the reviewer.
>
> **(o) Residual I5 class, and an authorised scope extension.** One class
> remains that no bounded metadata fixes: a late batch positioned before a
> COMPACTED batch can flip that batch's I5 outcome, and re-deciding it needs
> that batch's operations. 1,401 of 1,401 divergences; minimal witness
> A0/B2/M3. The remedy issue 0004 proposes, verified at 0 of 20,388, is a
> constraint on compaction rather than on retention: **a batch may be dropped
> only when no undelivered batch can still be positioned before it.** §5.2 S6
> already gates synced-document compaction on EITHER recorded peer
> acknowledgement covering the deleted range OR transfer of the successor
> snapshot to peers; the acknowledgement branch establishes the property and
> the snapshot-transfer branch does not.
>
> **Authorised:** issue 0004 extends into §5.2 S6 to state the drop condition.
> §5 is already its own section under the document's status line, the fix is
> required for V6 to hold rather than optional, and preserving V6 is this
> decision's standing rule. No vector moves, so this is not an owner ruling —
> but the owner should know the consequence: compaction of a synced document
> can no longer rest on snapshot transfer alone, so a multi-device user's log
> is retained longer. A local-only document is unaffected via S7.
>
> **(p) Recorded, not substituted:** issue 0004 has no argument for the
> one-multi-operation-batch reading of ED-0038's convention. Its sharper point
> stands on the record — a convention must be byte-derivable, and neither
> reading is; "it is really one batch" is knowledge from the generator source,
> not from the package. Both readings preserve the winners' relative order
> among themselves, so projection and internal ranking are identical, and they
> differ only against a foreign concurrent write, where ED-0038 chose the
> conservative direction deliberately. The `batchPositions` table shape was
> chosen over proposal 4's inline form because comment (g)'s L18 x L13 bound
> makes inline per-field frontiers both unrepresentable as a scalar and
> multiplicative in size.

> **Comment 2026-08-26 (q) — comment (o)'s authorisation is WITHDRAWN; the
> orchestrator ruled past its own criterion.** Issue 0004's reviewer refused
> proposal 7 and read comment (l)'s trigger back correctly. Comment (l) set
> the threshold as: preserving V6 costing unbounded growth, "and the owner
> would have to choose between V6's scope and compaction's purpose".
> Comment (o) then dismissed the escalation on the ground that no vector
> moves. That was the wrong test — it was not the test comment (l) stated.
>
> The reviewer's reading: the new lifetime maps are indeed O(entities+fields),
> and payload/schema support can stay bounded at worst at entity kind or
> schema per tombstone. But the residual I5 class is separate and is
> history-length. To satisfy V6 **as written** for that class, a replica needs
> the COMPACTED batch's operations — minimal witness A0/B2/M3: compacted B2
> previously changed F; a late earlier M3 deletes E, so full replay discards
> B2 whole, and seeded state cannot undo F without B2. No bounded metadata
> substitutes for those operations. The proposed S6 gate does not satisfy V6
> by retention; it **avoids the class by restricting when history may be
> dropped**, which is precisely the trade between V6's scope and compaction's
> purpose that comment (l) reserved to the owner. It also constrains the very
> late-arrival scenario V5 exists to cover and changes S6's snapshot-transfer
> branch.
>
> The reviewer is correct on both the substance and on what this record
> committed to. Comment (o)'s scope authorisation is withdrawn, issue 0004 may
> not claim V6 completeness on the strength of the S6 gate, and the question
> is escalated to the owner. Recorded as an orchestrator error: the criterion
> was well drawn and then not applied to the first case that met it, and the
> reviewer refusing an authorisation the orchestrator had already granted is
> the mechanism that caught it.

> **Comment 2026-08-26 (r) — a tautological conformance check survived the
> issue 0028 remediation AND the orchestrator's acceptance.** The §4.7
> evidence clause was checked, as the orchestrator asked, against the issue
> 0028 corpus integrated earlier today. It does not condemn property tests and
> does not retroactively invalidate that partial handoff. But the check
> surfaced this: `internal/persist/corpus_semantic_test.go:467` `checkV5` takes
> `proj *document.Projection` and **never reads it**. It asserts only that the
> manifest's own `lateBatchApplied` literal is false — the manifest saying what
> the manifest says. V5 and seeded-versus-uncompacted equivalence are not
> exercised at all. `checkV3` likewise ignores `actorId` and
> `loserValueDiscarded`; `checkV4` ignores `concurrentWriteApplied`. Verified.
>
> This is the exact class issue 0028's ED-0027 continuation was chartered to
> eliminate — V6's oracle asserting only non-emptiness, V7's `checkV7` dead
> code — and a third instance survived it, survived its adversarial reviewer's
> fourteen findings, and survived the orchestrator's own acceptance review.
>
> **Why it was missed, recorded so the gap is fixed rather than the instance.**
> Both the reviewer and the orchestrator verified the CHANGED vectors and the
> coverage gate's refusal of blocked-only evidence. Neither re-read the
> unchanged vector checks. And the coverage audit cannot catch this by
> construction: it verifies that a record exists, is categorised, and is not
> blocked — never that the assertion function consumes the projection. **A
> coverage gate that counts records cannot detect a tautological check.**
>
> Routed to issue 0028 with a requirement beyond repairing the three
> functions: every vector check must be **mutation-proven** — executed against
> a deliberately wrong projection and required to fail — which is ED-0033's
> probe discipline applied to the vector harness rather than to the corpus
> gates. Per the reviewer, the §4.7 clause stays and this cost is stated
> rather than described as free.
>
> **(s) Four corrections this record must absorb, three of them to its own
> text.**
> - **"Equal-past-size writes are necessarily concurrent" is FALSE.** Two
>   writes in the SAME batch share a past size and are not concurrent — they
>   are one actor's ordered writes, separated by `operationIndex` under
>   comment (b). The decision body's claim is wrong as written; the
>   four-component position handles the ordering correctly, but the
>   justification does not.
> - **Comment (m)'s "constant factor on the state, never a function of history
>   length" OVERCLAIMS.** The `batchPositions` table is O(positions x actors),
>   not a constant factor on state. The escalation trigger in comment (l) was
>   specifically history-length dependence, and O((entities+fields) x actors)
>   is not that — so the trigger still does not fire and no owner ruling is
>   sought — but the wording must be corrected to what was measured.
> - **ED-0039's acknowledgement branch is not self-sufficient.** Generic peer
>   receipt or acknowledgement is not proof: all capable peers must attest that
>   no pre-acknowledgement unsent lower-P batches exist, and future or newly
>   joining actors must observe the range. Issue 0004 states that or the drop
>   condition does not hold.
> - **The orchestrator's blocked-compaction observable is not testable as
>   required.** "A document MUST surface it" names no operation, trigger, or
>   channel. Replaced: a writer or compaction diagnostic MUST be emitted when a
>   requested or policy-driven drop is blocked, naming the reason and the
>   retained-log consequence.
>
> **(t) Still open, routed into proposal 10:** ED-0038's convention derives
> neither tombstone live intervals nor the zero-actor case; and the three I5
> branches need per-target and mixed-batch semantics, since one batch may
> target several entities in different lifecycle states.

- **Validation/reversal gate:** Issue 0004's text must state the construction
  precisely enough to implement, and its reviewer must confirm totality and
  determinism for every shape rather than for the counterexamples alone. If
  implementation reveals a shape where P diverges from a §4.7 vector, that
  returns to the owner rather than being absorbed. Superseded by contrary
  owner direction.

### ED-0038: Owner ruling — legacy packages read under a documented convention

- **Date:** 2026-08-26
- **Status:** Active
- **Decided by:** project owner, on an escalation from the orchestrator
- **Affected issues:** 0004, 0005, 0011, 0031, 0035, 0018
- **The ambiguity that forced the choice.** ED-0037 comment (k): two different
  histories serialize to identical bytes once retained logs are dropped, so a
  pre-ED-0037 single-actor package preserves the current value and winner
  identity but **cannot reproduce V5's late-concurrent resolution**. Verified.
  Being single-actor is not itself proof of conformance.
- **Decision:** A legacy package is read under **one documented reading
  convention**; **V5 exactness is explicitly not claimed for legacy packages**;
  and the first save **rewrites exact positions**, so the ambiguity cannot
  propagate past the first write. The owner accepted that this bakes an
  arbitrary-but-deterministic choice into the reader, in exchange for keeping
  issue 0035's committed evidence readable without depending on the Linux
  reference host that ED-0030's open question still covers.
- **The convention, subject to issue 0004 justifying otherwise:** a legacy
  single-actor snapshot is read **as though each retained winner were its own
  one-operation batch**, giving `causalPastSize = operationCounter - 1`. This
  adopts by convention exactly the identity ED-0037 comment (a) showed cannot
  be derived as fact, which keeps the reading honest about its own status. It
  is also the conservative direction: it maximises a legacy winner's position,
  so an existing value is harder for a late concurrent write to displace,
  which is the safer default for a local-first application opening its own
  older file.
- **Scope limit — the convention does NOT extend to multi-actor legacy
  packages.** For a single actor the only unknown is how operations were
  batched. For several actors the winner keys cannot be placed at all, and any
  convention would be inventing cross-actor causal relationships that never
  existed. ED-0037 comment (d) therefore stands unchanged for those: exactly
  migratable only where the full relevant write history for a field is
  retained or a certificate attests it, and otherwise fail-closed.
- **Reason:** v0.1 is unreleased and the predecessor set is empty, so the
  convention's arbitrariness never reaches a user, and rewrite-on-save bounds
  its lifetime to a single read. The alternative, rejecting legacy outright,
  had cleaner semantics but broke `TestCommittedPackagesRoundTrip` and made
  its repair depend on a machine this project no longer has.
- **Validation/reversal gate:** Issue 0004 states the convention normatively,
  states plainly that V5 exactness is not claimed under it, and states the
  rewrite-on-save obligation; issue 0005 places whatever §7/§8 wording that
  requires. The two ED-0031-protected evidence packages remain pre-ED-0037
  development artifacts carrying no conformance claim — the convention makes
  them readable, not conforming. If issue 0004 finds a reason to prefer the
  one-multi-operation-batch reading instead, it returns to the orchestrator
  with the argument rather than substituting it silently. Superseded by
  contrary owner direction.

### ED-0039: Owner ruling — tighten S6's drop condition rather than narrow V6

- **Date:** 2026-08-26
- **Status:** Active
- **Decided by:** project owner, on an escalation from the orchestrator
- **Affected issues:** 0004, 0005, 0011, 0027, 0029, 0031, 0018
- **The class that forced the choice.** One I5 class is not fixable by bounded
  metadata: a late lower-P batch can flip the I5 verdict of an ALREADY
  COMPACTED batch, and re-deciding it needs that batch's operations. Minimal
  witness A0/B2/M3 — compacted B2 previously changed F; a late earlier M3
  deletes E, so full replay discards B2 whole, and seeded state cannot undo F
  without B2. 1,401 of 1,401 of that class diverge. No retained key, position,
  or lifetime map substitutes for the operations themselves.
- **Decision:** **§5.2 S6's drop condition is tightened: a batch may be
  dropped only when no undelivered batch can still be positioned before it.**
  S6 today permits dropping on EITHER recorded peer acknowledgement covering
  the range OR transfer of the successor snapshot to peers; the acknowledgement
  branch establishes the property and the snapshot-transfer branch does not, so
  the latter ceases to be sufficient on its own. V6 holds exactly as written,
  no vector weakens, and the remedy is verified at 0 of 20,388.
- **The cost the owner accepted:** a synced document retains its operation log
  until the drop precondition is established, so a peer that never returns
  means the log does not shrink. *(Corrected 2026-08-26 after the
  acknowledgement narrowing below: this line originally read "until peers
  acknowledge", which contradicts the narrowing that proved acknowledgement
  alone insufficient. Issue 0004's reviewer caught the identical contradiction
  in issue 0004's own S6 cost summary; the same error was in this record. The
  cost is bounded by the full attestation, not by receipt.)* Multi-user merge is already out of v0.1 scope
  (`docs/FORMAT-SPEC.md:339`), and local-only documents compact unaffected via
  S7, so in v0.1 this costs close to nothing and the cost lands whenever sync
  actually ships. **Issue 0004 must nonetheless state the observable behaviour
  when compaction is blocked** — a document that cannot compact must surface
  that, not grow silently.
- **Reason:** The alternative was the first normative vector to weaken, which
  would let a compacted file and an uncompacted one disagree — precisely the
  property V6 exists to rule out — and would require re-deriving V6's
  conformance fixtures. Retaining the operations instead was rejected as
  making retention a function of history length, defeating what compaction is
  for.
- **Process note (orchestrator error, and the mechanism that caught it).**
  ED-0037 comment (l) set the escalation trigger as preserving V6 costing
  unbounded growth, "and the owner would have to choose between V6's scope and
  compaction's purpose". Comment (o) then authorised the S6 extension on the
  different ground that no vector moves — not the test comment (l) stated.
  Issue 0004's reviewer refused proposal 7, read the criterion back, and
  declined to let issue 0004 claim V6 completeness on the strength of a gate
  that avoids the class rather than satisfying it. That refusal of an
  authorisation the orchestrator had already granted is what produced this
  ruling. Comment (q) records the withdrawal.
> **Comment 2026-08-26 — this decision's SUPPORTING INFERENCE is narrowed;
> the decision itself is untouched.** ED-0039 states that S6's
> acknowledgement branch "establishes the property". Issue 0004's reviewer
> showed that does not follow from receipt alone, and issue 0004 raised it to
> the orchestrator rather than letting its text carry a narrower claim than
> this record — the same course it took on the F4 benign correction.
>
> An acknowledging peer may still hold a batch it CREATED BEFORE
> acknowledging and has not delivered; that batch has a smaller causal past
> than the acknowledged range and therefore sorts before it. So an
> acknowledgement establishes the drop condition only where the sync protocol
> attests three things: **every capable peer has acknowledged**; **each
> acknowledging peer has delivered everything it created before
> acknowledging**; and **every later-created actor observes the range before
> producing batches**.
>
> **v0.1 defines no sync protocol, so v0.1 supplies no such attestation: a
> v0.1 writer cannot drop from a synced document on acknowledgement alone.**
> Local-only documents compact unaffected under S7.
>
> The ruled precondition — a batch may be dropped only when no undelivered
> batch can still be positioned before it — is unchanged and remains
> normative. What changes is the inference from acknowledgement to that
> precondition. **The v0.1 consequence the owner was shown is unchanged**
> (multi-user merge is out of v0.1 scope, local-only compaction is
> unaffected, so the v0.1 cost remains approximately nothing); the future
> consequence when sync ships is LARGER than this decision's flat sentence
> implied, and the owner is told so rather than it being absorbed silently.
> Re-escalation was not sought because the alternative the owner weighed —
> narrowing V6 — is unchanged, and the v0.1 impact that grounded the choice is
> unchanged.
>
> **Boundedness, resolved properly.** The reviewer also killed issue 0004's
> boundedness paragraph outright: with a `batchPositions` table each
> referenced batch stores a frontier of up to L18 members and the actor count
> grows with history, so "a constant factor on the state, never a function of
> history length" was false — and ED-0037 comment (s)'s O(positions x actors)
> phrasing was itself only half the correction. The accurate statement, now in
> issue 0004's text: **the NUMBER of positions is bounded by state, the BYTES
> are not, L23 bounds the total, and a document whose metadata will not fit
> L23 cannot compact — with that refusal observable under S6.**

- **Validation/reversal gate:** Issue 0004 states the tightened drop condition
  in §5.2 and the blocked-compaction observable; its reviewer confirms the
  gate is stated as a precondition on dropping rather than as advice, and that
  no §4.7 vector text moved. Issue 0027, 0029, and 0031 inherit the condition
  wherever they implement compaction. If sync design later shows the
  acknowledgement branch unworkable in practice, that returns to the owner
  rather than being relaxed in implementation. Superseded by contrary owner
  direction.

### ED-0040: Owner direction — no new issue dispatch after issues 0004 and 0011

- **Date:** 2026-08-26
- **Status:** Active
- **Decided by:** project owner
- **Affected issues:** all scheduling; 0004, 0011, 0005, 0028, 0029, 0031
- **Decision:** Record the project owner's direction verbatim: "after these
  issues stop with new issues". The two active pairs — issue 0004's
  specification work and issue 0011's compatibility analysis — run to
  completion under their existing contracts. **No further issue is dispatched
  afterwards**, and no replacement pair is created. This supersedes the
  orchestrator's recorded intent to purge the issue 0011 pair and dispatch a
  fresh one for the P implementation once issue 0004's text was accepted; that
  implementation stays queued and unstarted.
- **Convergence, not a lowered bar.** The two live pairs finish what is
  already in their scope and **route** everything else to its owning issue
  rather than solving it. No pair opens a new line of inquiry. Every existing
  review gate stands unchanged: the adversarial reviewer's approval is still
  required, `./scripts/verify.sh` must still exit 0, and the orchestrator's
  independent review is unchanged. A finding discovered late is recorded and
  routed, never absorbed to avoid a handoff and never dropped to reach one.
- **Reason:** This is the owner's direct instruction on scheduling. It changes
  no issue's scope, dependency semantics, or acceptance criteria, and no
  accepted decision.
- **Queue left standing, in dependency order, for whenever work resumes.**
  Issue 0004's worker supplied a fuller routing list than the orchestrator's
  first draft; that over-complete version is adopted here deliberately, on the
  principle that the next person has no context beyond this record.
  1. **Issue 0005** — the §7.6 half of the watermark question, which issue
     0004's §4.5 I2 half is written to compose with; ED-0033's §7.6
     name/content-binding defect; placement of a §9.3 row for the
     `causalPastSize` bound, since issue 0004 fixes the arithmetic but not the
     limit table; the `assetRoot` null acceptance category (ED-0034); and
     recording in §8 that v0.1 is unreleased (ED-0037).
  2. **Issue 0011 implementation** — implement P with the widened position
     maps, the three-branch lifecycle rule, and per-target mixed-batch
     semantics; retire `TestLWWWinnerKeyKeepsGreatestKAcrossCausalWrites`; and
     remove issue 0028's V2 marker. Unblocked only once issue 0004's text is
     accepted.
  3. **Issue 0028** — `checkV5` is vacuous at
     `internal/persist/corpus_semantic_test.go:467`, never reading `proj` and
     passing on the fixture's own manifest literal; it must assert replayed
     state, meaning the retained winner and seeded-equals-uncompacted.
     `checkWinner` at `:414` ignores the `actorId` and `loserValueDiscarded`
     it is handed; V4 ignores `concurrentWriteApplied`. Every vector check
     must additionally be **mutation-proven** (ED-0037 comment (r)). The
     position widening requires re-deriving `EXPECTATIONS.md` records at lines
     **448, 452, 685, 708 and 1510** plus the two multi-actor snapshot
     fixtures, and the V3 fixture must use equal-past-size writes or have its
     expected value re-derived.
  4. **Issue 0029** — the `Load` path building its non-skipped set before
     per-actor range chaining, which issue 0004's I2 half rules against;
     `reader.go:468`'s discarded `checkReservedRanges` error (ED-0032); and
     `wire.go`'s null `assetRoot` tolerance (ED-0034).
  5. **Issue 0027/0029/0031** — the `apply.go` versus `smoke_test.go` retained
     -log disagreement and the save-drops-prior-history question.
  6. **Issues 0035 and 0018** — re-measure the Part A budgets after the
     widening lands; never re-derive them, and do not publish the pre-ED-0037
     measurements as v0.1 budgets.
  7. **Issue 0004's own closure gates** — `lwwHorizon` fixation and the
     remaining ED-0014 numeric-bound gates.
  8. **RESOLVED 2026-08-27 (`3cca9ee`) — Issue 0004 — §4.6 LWW4 scoping mismatch, found after acceptance.**
     Raised by issue 0011's worker while reconciling against the newly merged
     text, and verified by the orchestrator against main. §4.5 (line 539)
     sends **any** same-id create collision to LWW4 — "a collision with
     another create on the same id resolves under LWW4" — while LWW4 itself
     (line 602) is scoped to "two **concurrent** `createEntity` operations for
     the SAME id from different actors (each locally valid under O4)". A
     *causally ordered* same-id collision is therefore routed to a rule whose
     text does not cover it. It may well be unreachable — the later actor
     observed the create, so O4 local validation should have rejected it, and
     LWW4 already states the within-one-actor case is impossible — but
     "probably unreachable" is the reasoning that produced this session's
     other defects, and two accepted sentences disagreeing is a wording
     question for §4.6's owner rather than an implementation licence. **This
     is a defect in text accepted and integrated the same day**, caught by the
     other pair rather than by its own review or by the orchestrator.
  9. **RESOLVED 2026-08-27 (`3cca9ee`) — Issue 0004 — legacy-mode selection versus the actor universe.** Also
     found post-acceptance, by issue 0011's reviewer, and verified against
     main. §5.1 (lines 739–747) selects legacy reading mode **solely by the
     frontier's actor count**: one actor gets the documented convention, two
     or more is fail-closed. But `FromSnapshot` accepts winner and creation
     key actors that do NOT appear in the frontier, so a snapshot can carry a
     single-actor frontier while its keys span two or more actors — which the
     spec would route to the single-actor convention while the keys are in
     fact unplaceable. The selection predicate needs to be the actor UNION
     over frontier and keys, or the spec must state why the frontier alone is
     sufficient. Until an owner rules, issue 0011's evidence document states
     the conflict explicitly rather than assuming either rule.
 10. **RESOLVED 2026-08-27 (`3cca9ee`) — Issue 0004 — §5.1 says "instead" but never rules on MIXED members.**
     Third post-acceptance gap, raised by issue 0011's worker and verified
     against main. §5.1 line 738 defines a legacy snapshot as one carrying the
     pre-ED-0037 `winnerKeys`, `createdKeys`, or `deletingKey` members
     "instead". That wording implies the two encodings are mutually
     exclusive but states no rule for a snapshot carrying BOTH — new
     `winnerPositions` alongside an old `createdKeys`, say. A reader
     implementing §5.1 has nothing to apply. The natural candidate is the
     fail-closed posture issue 0011's own document already argued for a mixed
     shape — neither new-shape nor eligible for the convention, therefore
     rejected rather than partly converted — but that is **this project's
     reasoning, not a ruled rule**, and issue 0011 correctly labelled it as
     its document's posture rather than asserting it. §5.1's owner rules.
- **Queue items 8, 9 and 10 are CLOSED**, fixed directly on owner direction in
  `3cca9ee` after an adversarial review of the design settled that each was a
  wording defect rather than a missing mechanism. Item 8 narrows §4.5's
  create-collision referral to concurrent collisions, since LWW4's scope is
  exactly that case. Item 9 selects the legacy reading mode on the actor union
  over the frontier and the retained keys — `internal/cli/ops.go:679-697`
  already computed that union, so the specification was behind the shipped
  code. Item 10 states that the two snapshot encodings are exclusive and that a
  mixed snapshot rejects as ambiguous required data. No §4.7 vector outcome
  changed; the rows are byte-identical. Full `./scripts/verify.sh` exits 0.
- **Not to be built:** ED-0039's S6 peer-attestation apparatus. The
  specification itself records that v0.1 supplies no such attestation, and
  `internal/cli/ops.go:515-527` already refuses multi-actor compaction
  outright. Leaving the rule written and unimplemented costs nothing;
  implementing it now would add machinery for a case v0.1 cannot produce.
- **Open owner questions carried forward:** ED-0030's — whether the Linux host
  remains the measurement authority for issue 0035, and whether Python 3.14.7
  with zlib 1.3.2 should be restored on this host; and whether Go 1.27 should
  be installed as the host default to retire the `PATH` workaround.
- **Validation/reversal gate:** Superseded by contrary owner direction at any
  time.

### ED-0041: The vacuous-check pattern — three instances in one session

- **Date:** 2026-08-26
- **Status:** Active
- **Affected issues:** 0028, 0029, 0011, 0035, and any future verification work
- **Decision:** Record as a standing lesson, findable outside the decision it
  arose in, that this project has now produced **three distinct instances in a
  single session of a check that looks correct, reviews correct, and exercises
  nothing**:
  1. **`checkV5`** (`internal/persist/corpus_semantic_test.go:467`) takes
     `proj *document.Projection` and never reads it, asserting only that the
     fixture's own manifest literal says what it says. V5 was never exercised.
     It survived issue 0028's remediation — chartered precisely to kill this
     class after V6's non-emptiness oracle and V7's dead `checkV7` — survived
     its reviewer's fourteen findings, and survived orchestrator acceptance.
  2. **Issue 0004's §4.7 clause check.** Asked whether a new normative clause
     would invalidate the integrated issue 0028 corpus, its worker inspected
     the expectation RECORDS rather than the CHECKERS that consume them, and
     reported "costs the corpus nothing". It retracted this unprompted, with
     the better diagnosis: *it inspected the artifact that looks like evidence
     instead of the thing that does the work.*
  3. **Issue 0011's §4.2 detection rule.** Defined the legacy/new marker
     through a `winnerKeys` entry alone, so a package with **no** `winnerKeys`
     and old `createdKeys` is missed entirely — **an empty map satisfies a
     per-entry test vacuously.**
  4. **Issue 0004's own I5 prose.** Its reviewer's endpoint blocker was not a
     design error at all: the verified model already had create-establishes
     and delete-closes — in it the create sets the entity live before the
     presence test and is never itself presence-tested, and the delete is
     presence-tested first and then closes the interval. The **prose written
     around that model invented a universal classification the model never
     applied**, one which would have discarded every create. The 0-divergence
     results were real; the sentences describing them were not.
- **Instance 4 is the most dangerous shape and deserves naming separately.**
  The first three are checks that fail to test. This one is a *specification
  text diverging from the model that was verified* — and the specification is
  what ships while the model is only what was run. A spec that does not match
  its verified model is worse than an unverified spec, because it carries the
  assurance of evidence that was gathered about something else. It is also the
  hardest to catch: the evidence is genuine, the numbers are real, and only a
  reader who compares the prose against the executable model can see the gap.
  Issue 0004 found it against its own text after its reviewer pushed, and
  described the progression exactly: first the aggregate that could not
  discriminate, then the expectation records inspected instead of the
  checkers, then the prose written instead of the model that ran.
- **The shape they share.** Each passes not because the property holds but
  because the assertion never reaches the property: a parameter unread, an
  artifact inspected in place of its consumer, an empty collection iterated
  zero times. None is detectable by reading the test name, the record count,
  or the coverage audit — the 0028 gate verifies that a record exists, is
  categorised, and is not blocked, never that the assertion consumes what it
  was given.
- **The remedy already required, generalised:** ED-0037 comment (r) requires
  every issue 0028 vector check to be **mutation-proven** — run against a
  deliberately wrong input and required to fail. That requirement should be
  read as applying to any check this project relies on for conformance
  evidence, not only to the vector harness. A check that has never been
  observed to fail has not been shown to test anything. The empty-collection
  case needs its own probe: a per-entry test must be exercised against an
  empty collection and required to fail or to be explicitly out of scope.
- **Reason:** Three independent instances in one day, two of them surviving
  adversarial review, is not coincidence — it is a blind spot in how this
  project verifies. Recording it once, where it can be found without reading
  ED-0037's twenty comments, is worth more than three separate routings.
  5. **A verification CLAIM broader than the verification performed.** Issue
     0004 stated it had confirmed twelve citations line-by-line against
     ED-0037's in-force table. Its reviewer showed it could not have: at the
     time of the check that table's retention row asserted
     `O(entities + fields)` unqualified, contradicting the worker's own
     number-bounded/bytes-unbounded text, and the row was never reconciled.
     The text was right because it followed comment (s) and ED-0039 — not
     because the summary confirmed it. This is the family applied one level
     up: not a check that fails to test, but a *description of a check* that
     claims more than the check did. The worker recorded the correction in the
     issue file rather than in chat, on the ground that the issue record is
     what survives.
- **A refinement: a lesson can transfer within a domain and fail to cross
  one.** Issue 0011's worker was told, early on, to reconcile its document
  against the decision record ONCE rather than per finding, and it did — then
  spent three further review rounds fixing the specification's encoding model
  one line at a time, because it never asked what else rested on the same
  assumption. Its own account: it applied the discipline to the decision
  record and failed to generalise it to the specification, which is the same
  error twice. When finally instructed to sweep for the ASSUMPTION rather than
  the reported line, it found the fourth instance the orchestrator had
  predicted. So understanding a discipline in one domain is not evidence it
  will be applied in the next one — which is the same conclusion as the
  mechanisation argument, reached from the opposite direction: not "the lesson
  did not stick" but "the lesson stuck and stayed local".
- **The mechanised remedy is validated twice, independently.** ED-0037's
  summary block now carries a standing requirement that any amendment must
  update it in the same commit. That requirement would have caught BOTH
  failures it arose from: the orchestrator's, by forcing the row to be
  rewritten alongside the amendment; and issue 0004's, because a row that
  cannot be stale is a row whose confirmation means something. Two independent
  cases, neither of which understanding prevented.
- **A lesson is the WRONG FORM for this, and there is direct evidence.**
  Issue 0004 diagnosed instance 4 against its own text — prose diverging from
  the model it pointed at — and then, **within the hour and inside the same
  document**, wrote an absolute I5 sentence citing rule E6 as its authority
  for a rule E6 explicitly carves out. The citation was doing the opposite of
  its job: it made the claim look checked while guaranteeing that anyone who
  checked it would find the contradiction. Its own conclusion, and the right
  one: *recognising the shape did not stop it reproducing the shape*, so **the
  lesson does not transfer by being understood — it transfers only by being
  mechanised.**
  That is the argument for putting the check-the-checker requirement in
  NORMATIVE TEXT rather than leaving it in a decision record: a normative
  clause makes a reader check the checker, whereas a lesson only makes an
  author intend to. Issue 0004's §4.7 clause — a fixture has not exercised its
  vector when its checker does not read the projection it replays, however
  complete its recorded expectation looks — is therefore load-bearing, not
  commentary, and it names `checkV5`'s shape without singling out the
  instance. The same reasoning is why ED-0037 comment (r)'s mutation-proof
  requirement is a requirement.
- **Validation/reversal gate:** None; this is a lesson, and the point above is
  that a lesson is insufficient. It is discharged only when the mutation-proof
  requirement and the §4.7 clause are implemented broadly enough that the
  class cannot recur silently — not when it is understood.

### ED-0042: Owner direction — resume dispatch at two pairs, queue items 1 and 2

- **Date:** 2026-08-29
- **Status:** Active
- **Decided by:** project owner
- **Supersedes:** ED-0040's dispatch halt only. ED-0040's standing queue, its
  convergence rules, its "Not to be built" note, and its open owner questions
  all remain in force and unamended.
- **Affected issues:** 0005, 0011; queue items 3–7 (0028, 0029, 0027/0031,
  0035/0018, 0004 closure) stay queued.
- **Decision:** The project owner directed a resumption — "Continue with
  INSTRUCTIONS.md", with the accompanying instruction to start "another round
  of agent per task and two task at a time work". Dispatch resumes at the
  ED-0028 cap of two concurrent issues, one executor–reviewer pair each, with
  the ED-0029 model assignment (worker: Claude Code, Opus 5; reviewer:
  OpenCode, `openai/gpt-5.6-sol` at maximum effort). Both slots are filled
  from ED-0040's standing queue **in its recorded dependency order**, not by a
  fresh selection:
  1. **Issue 0005** — queue item 1, unchanged in content.
  2. **Issue 0011 implementation** — queue item 2, unchanged in content.
- **Both ED-0004 gates on queue item 2 are discharged, and this records why.**
  ED-0036 halted issue 0011's semantic change under ED-0004 with an explicit
  resumption condition: "It does not resume until issue 0004 rules." Issue
  0004 has ruled. The canonical-order construction is defined by ED-0037's
  causal position P and the §4 text merged by `6041867`; the three defects
  found in that text after acceptance are closed in `3cca9ee`; and no §4.7
  vector outcome moved when they were closed. ED-0040's own precondition for
  item 2 — "unblocked only once issue 0004's text is accepted" — is therefore
  satisfied. This is a gate check, not a re-ruling: nothing about the V2
  outcome, the causal-governs ruling of ED-0032, or ED-0037's position design
  is reopened by this decision.
- **Reason:** This is the owner's direct instruction on scheduling. As with
  ED-0040 it changes no issue's scope, dependency semantics, or acceptance
  criteria, and no accepted decision. The queue order is preserved precisely
  because ED-0040 recorded it for a resumption whose orchestrator might have
  no context, and this is that resumption.
- **Validation/reversal gate:** Both pairs run under the unchanged review
  contract — adversarial reviewer approval, `./scripts/verify.sh` exiting 0,
  and independent orchestrator review of dependency, scope, diff, provenance,
  licensing, and verification. If either pair's work would require an
  owner-level product decision, it is escalated rather than absorbed, exactly
  as ED-0040's convergence clause required of the previous round.

### ED-0043: Split ED-0040 queue item 3 so the position widening cannot land broken

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** 0011, 0028
- **Decision:** ED-0040 assigns the whole of queue item 3 to issue 0028,
  including "the position widening requires re-deriving `EXPECTATIONS.md`
  records at lines 448, 452, 685, 708 and 1510 plus the two multi-actor
  snapshot fixtures, and the V3 fixture must use equal-past-size writes or
  have its expected value re-derived". Read literally that would require issue
  0011 to merge an implementation that leaves `./scripts/verify.sh` failing
  until a later issue repairs the corpus, which no branch may do. The item is
  therefore split along a line of authorship, not of convenience:
  1. **Issue 0011 owns every record, fixture, and expectation its own change
     invalidates**, and its branch must exit 0 with a clean tree exactly like
     any other. That includes the five `EXPECTATIONS.md` records, the two
     multi-actor snapshot fixtures, the V3 equal-past-size question, retiring
     `TestLWWWinnerKeyKeepsGreatestKAcrossCausalWrites`, and giving V2 a real
     category and `coveredBy` entry that passes the audit as ordinary executed
     coverage (ED-0036's replacement condition, which is where that condition
     was always meant to be discharged).
  2. **Issue 0028 keeps the vacuous-checker work**, which is independent of
     the widening: `checkV5` never reading `proj`
     (`internal/persist/corpus_semantic_test.go:467`), `checkWinner` ignoring
     `actorId` and `loserValueDiscarded` at `:414`, V4 ignoring
     `concurrentWriteApplied`, and the ED-0037 comment (r) requirement that
     every vector check be mutation-proven. A re-derived expectation is not a
     mutation-proof, and issue 0011 delivering the first does not discharge
     the second.
- **A re-derivation is not a licence to move an outcome.** Issue 0011 may
  re-derive an expectation only where its own change provably alters what the
  normative text requires, and must show the derivation. No §4.7 vector's
  required outcome moves; if the implementation cannot satisfy one, that is an
  escalation to the owner under ED-0036, not an edited expectation.
- **Reason:** Bounded and reversible orchestration decision. It moves no
  semantics and changes no acceptance criterion — it only records which of two
  issues authors a file that both were told to care about, and it prevents the
  one outcome neither issue's contract permits: a green merge that leaves main
  red.
- **Validation/reversal gate:** Discharged when issue 0011's branch verifies
  clean with V2 counted as executed coverage, and issue 0028's later pass
  finds the re-derived records mutation-proof. If issue 0028 finds any record
  issue 0011 re-derived to be wrong, that is a finding against issue 0011's
  review, and is recorded as one rather than quietly corrected.

### ED-0044: Owner direction — raise the cap to four pairs; only two slots are fillable

- **Date:** 2026-08-29
- **Status:** Active
- **Decided by:** project owner (cap); orchestrator (readiness audit)
- **Supersedes:** ED-0028's two-pair cap. ED-0029's model assignment, ED-0042's
  queue-order rule, and ED-0043's split all stand unchanged.
- **Affected issues:** all scheduling; audit below names each open issue.
- **Decision:** The active cap returns to **four** concurrent issues, one
  executor–reviewer pair each, as ED-0005 originally set and ED-0028 reduced.
  The owner's instruction was to raise the cap and to start ready work up to
  it. **The cap is raised; the third and fourth slots are left deliberately
  empty**, because a dependency audit of all 27 open issues finds no further
  work that is both dependency-ready and free of a live pair's files. Per
  INSTRUCTIONS.md an empty slot requires a documented dependency, authority,
  or validation reason, so each is recorded here.
- **Readiness audit, 2026-08-29.** Filled: **0005** and **0011** (ED-0042).
  Every other open issue is blocked, and by what:
  - **0028** — its remaining scope under ED-0043 is the vacuous-checker work in
    `internal/persist/corpus_semantic_test.go`, the same file and corpus issue
    0011 is editing, against semantics 0011 is changing. Mutation-proving a
    check against an implementation that is about to move is worthless.
    Sequential after 0011 by construction, not by preference.
  - **0029** — all three of its routed defects have fix directions explicitly
    gated on issue 0005's rulings, which are in flight in the other slot.
  - **0027, 0031** — the retained-log write path and snapshot member encoding
    are what issue 0011 is reimplementing. 0031 additionally cannot proceed on
    its own terms: no successor schema exists, and no positive migration may
    be fabricated.
  - **0035, 0018** — ED-0040 item 6 conditions both on the widening having
    landed. It has not.
  - **0021, 0026, 0034** — 0021 is gated on 0004/0005/0028/0035. 0026 has no
    open work: its ED-0027 remediation landed on 2026-08-25 and the tracker
    simply never recorded it (see below). 0034's remainder needs interactive
    registry checks and an owner authorization to reserve namespaces.
  - **0006, 0007, 0013, 0022, 0030, and everything downstream of 0013 —
    0012, 0014, 0016, 0017, 0025, 0032, 0033** — blocked behind the two owner
    decisions below. This is eleven issues, including every untouched one.
- **Two owner decisions gate the entire geometry half of the project.**
  Neither is the orchestrator's to make, and guessing either would spend pairs
  on work the other answer discards.
  1. **The ADR-0002 stack decision.** Issue 0030's registered Phase 0
     performance criterion FAILED on A3 robust medians
     (`BenchmarkIntersectSegmentsRobust` 12779.5 ns/op,
     `BenchmarkOffsetCertificateAndVertex` 20949.0 ns/op against a 10000 ns/op
     bound), and the trigger-3 disposition found it NOT PLAUSIBLY CLOSABLE by
     caching or locality. Revision trigger 3 has fired. The recorded ADR-0002
     reassessment is explicitly RECOMMENDATION ONLY and says any stack
     decision belongs to the owner. Until that is answered, 0030 cannot
     ratify, 0006 and 0007 cannot close their deferred gates, and 0013 —
     which five untouched issues depend on — cannot start.
  2. **Which host is the measurement authority** (ED-0030's carried-forward
     question). Issue 0007's one remaining dispatchable criterion is the
     wall-clock and allocation measurement of the boolean and planar-face
     pipeline — precisely the H-6 domain issue 0030's judgment records as
     MISSING. It must be recorded "on the same named corpus and reference
     environment as the correctness runs". The repository has moved to macOS;
     the prior measurements are the Linux host's. Measuring on a different
     host produces numbers that cannot be compared to the ones already judged.
- **A note on the honest answer.** The two live pairs are not a throttle that
  a higher cap relieves; they are the front of a genuinely serial dependency
  chain, and the rest of the project is behind two decisions rather than
  behind capacity. Raising the cap is still correct — it costs nothing and the
  slots fill the moment either pair lands or either decision is made — but
  filling the slots by dispatching blocked or colliding work would manufacture
  activity, not progress.
- **Validation/reversal gate:** Re-run this audit when either live pair lands
  or either owner decision is answered, and fill the freed slots immediately
  per ED-0042's rule that the queue order in ED-0040 is preserved.

### ED-0045: V2's discharge is stated as an observable, not a mechanism; the audit dimension goes to issue 0028

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** 0011, 0028
- **Amends:** ED-0036's replacement condition, restated by ED-0043 item 1.
- **The finding, verified independently.** Issue 0011's worker reported before
  editing anything that ED-0036's replacement condition cannot be satisfied as
  written. Orchestrator verification against main confirms every load-bearing
  claim:
  - `internal/corpusgen/corpus_test.go:460` rejects a `covered` categoryAudit
    entry whose named record's category differs from the token
    (`catOfRec[id] != e.Token`), and every token in that dimension is a
    FORMAT-SPEC §9.4 **rejection** class.
  - The V2 record at `testdata/corpus/EXPECTATIONS.md:629` is
    `outcome: success` with `category: null`, as are all eight §4.7 vector
    records; 80 records carry a null category in total.
  - `rowAudit` maps L-rows, which the vector records do not carry.
  So no audit dimension can count V2. Giving it a rejection category would be
  false; leaving it null makes it vanish, which ED-0036 forbade.
- **This is the THIRD formulation of this condition to prove unsatisfiable,
  and the pattern is the point.** ED-0035 required the audit to "count V2 as
  real coverage"; ED-0036 voided that and required "a real category and
  `coveredBy` entry"; both specified a MECHANISM the specifier had not checked
  existed. The condition is therefore restated as an **observable**, which no
  future mechanism change can invalidate:

  > V2 must be demonstrably executed and demonstrably able to fail. Issue 0011
  > removes `executionBlocked` and the `residualBlockers` entry only when they
  > have become untrue, makes the record's assertions actually replay and
  > assert projected state, and **mutation-proves that specific check** —
  > break the implementation, show V2 red, restore it, and put the transcript
  > in the handoff evidence.

  A mutation proof is strictly stronger evidence than an audit line, so this
  is not a relaxation of ED-0036; it is what ED-0036 was reaching for.
- **The audit gains a real gap, and it is bigger than V2.** That no dimension
  can count a §4.7 vector means **none of the eight has ever been counted**.
  The coverage audit is silent about the entire conformance-vector class —
  an ED-0041-class vacuity at the gate level rather than the fixture level.
  Issue 0011 must record that absence visibly in `EXPECTATIONS.md` (a note or
  gap record, whichever `internal/corpusgen` already validates) so it is
  documented rather than silent. It must NOT build the dimension.
- **The `vectorAudit` dimension is issue 0028's, and the reason is not
  workload.** Issue 0011's worker proposed building it and offered to be told
  otherwise. It goes to 0028 because **the pair changing the semantics must
  not also author the gate that certifies them.** That is precisely the
  check-the-checker failure ED-0041 recorded; a coverage dimension designed by
  the implementation it would bless is a checker whose author already believes
  the answer. Issue 0028 owns vector-check integrity and the ED-0037 comment
  (r) mutation-proof requirement for all eight vectors, so the dimension
  belongs with the pass that will apply it to all eight — V2 included.
- **Reason:** Bounded and reversible. It moves no semantics, weakens no
  expectation, and changes no §4.7 outcome. It restates one condition in
  mechanism-independent terms and allocates one piece of new gate machinery to
  the issue whose purpose is auditing gates.
- **Validation/reversal gate:** Discharged when issue 0011's evidence contains
  the V2 mutation transcript and the recorded audit gap, and when issue 0028
  later lands a vector dimension covering all eight §4.7 vectors. If issue
  0028 finds V2's re-enabled check unable to fail, that is a finding against
  issue 0011's review and is recorded as one.

### ED-0046: The frameAndOrder reordering is issue 0011's; queue item 4 shrinks; ED-0033 finding 3 now has an executed witness

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** 0011, 0029, 0005
- **Amends:** ED-0040 queue item 4 (scope only, no semantics).
- **Why a boundary is needed now.** Issue 0011's reviewer authorized F4 closure
  validation across the `FromSnapshot`, delivery, `Load`, and undo paths, and
  issue 0011 has correctly concluded from §4.5 I2 that retained-log structural
  checks must run BEFORE watermark suppression — which in `frameAndOrder` means
  moving the closure check ahead of the watermark skip loop. ED-0040 queue item
  4 separately assigns issue 0029 "the `Load` path building its non-skipped set
  before per-actor range chaining". Those are two descriptions of the same
  reordering, held by two different issues.
- **Decision, along the same authorship line as ED-0043:**
  1. **Issue 0011 owns the reordering.** Implementing F4 correctly REQUIRES
     structural checks to precede suppression; the work cannot be split from
     the fix that needs it, and an issue may not ship a check it has knowingly
     placed after the step that would suppress its input.
  2. **Issue 0029 keeps what is genuinely separate:** `reader.go:468`'s
     discarded `checkReservedRanges` verdict (ED-0032) and `wire.go`'s null
     `assetRoot` tolerance (ED-0034), whose acceptance category issue 0005 is
     ruling on. Queue item 4 is those two items and no longer the reordering.
  3. Issue 0011 must state in its handoff which of issue 0029's symptoms its
     reordering incidentally resolves, so the next pair verifies rather than
     assumes. Resolving a symptom is not closing ED-0032: the discarded verdict
     at `reader.go:468` is a separate defect and stays open until fixed there.
- **ED-0033 finding 3 now has an EXECUTED witness, and it was free.** Issue
  0005 built the discriminating mutation witness its reviewer designed —
  snapshot W(A)=100 with retained ranges [70,71] and [71,71], overlapping at
  71, both wholly at or below the watermark — and reported that **the mutant
  needed no synthesizing: the live `Load` path already is the covered-entry-
  filtering implementation.** The recorded result is `Inspect` rejecting with
  `malformed-framing` naming the overlapping ranges, against `Load` accepting.
  ED-0033 recorded finding 3 as UNEXECUTED analysis because proving it needed a
  test outside that pair's scope; it is now executed, on an input lying wholly
  BELOW the watermark, which is the case the written analysis could not reach.
  The run used `go test -overlay` and created no repository file. Issue 0029's
  future pair inherits a reproducible input rather than an argument.
- **Two findings died under checking today, and both deaths are recorded so
  nobody re-raises them.** Issue 0005's claimed `snapshot.go`/`persist_seam.go`
  disagreement was a category error — complementary contracts, one validating
  snapshot-internal history, the other chaining package ops through the replay
  seam — and its ops-entry-name stage/token question was not a defect either:
  `classifyNamespace` (`internal/persist/names.go:124-136`) already rejects a
  malformed ops name at `StageNames` with `invalid-entry-name`, via `scanArchive`
  at `reader.go:212`, and framing does not run until `reader.go:440`. Verified
  independently. **The orchestrator was wrong on the second one** — it had told
  issue 0005 the finding still routed — and was corrected by the pair with
  citations. Both are filed as withdrawn records under
  `~/projects/domowik-observatory/routed/`, named so they cannot be mistaken for
  live routings. Net routed findings from issue 0005 to issue 0029: zero beyond
  what ED-0033 and ED-0034 already record.
- **Reason:** Bounded and reversible. It allocates one reordering between two
  issues, records a witness that already exists, and prevents a future pair
  redoing or contradicting work that has landed.
- **Validation/reversal gate:** Discharged when issue 0011's handoff names the
  symptoms its reordering resolves and issue 0029 later verifies them
  independently, and when `reader.go:468` is fixed in issue 0029 rather than
  assumed closed by the reordering.

### ED-0047: L33 must arrive covered, not merely legal; two routings recorded

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** 0005, 0028, 0031, 0018
- **The defect issue 0005's reviewer exposed.** Adding row L33 to FORMAT-SPEC
  §9.3 surfaces stale L1–L32 hard-codes. Orchestrator verification finds the
  load-bearing one at `internal/corpusgen/corpus_test.go:650`:

  ```go
  for n := 1; n <= 32; n++ {
      if !seenRows[fmt.Sprintf("L%d", n)] { ... "row L%d missing from audit" }
  }
  ```

  The completeness check is a LITERAL, not a derivation from the specification
  it audits. So L33 lands with `verify.sh` green while the audit silently
  never requires coverage for it. **This is the ED-0041 shape at gate level
  again: a completeness check that cannot notice the thing it checks growing.**
  `testdata/corpus/parameters.json:37` carries the same staleness in prose,
  asserting ratified quantities are "cited there by row ID (L1–L32)".
- **Decision, splitting data from machinery:**
  1. **Issue 0005 adds the L33 `rowAudit` entry and corrects
     `parameters.json`'s range.** That is DATA its own change invalidates, it
     is permitted by the existing loop, and it means L33 arrives with real
     coverage rather than merely legal silence. ED-0043's authorship line
     applies unchanged.
  2. **Issue 0005 must NOT change the hard-coded bound.** Bumping 32 to 33
     converts a structural defect into a maintained literal and leaves the
     next row to rediscover it.
  3. **The structural fix routes to issue 0028:** the completeness loop must
     derive its row set from §9.3 rather than a literal, so the audit fails
     when the specification grows past it. That is gate machinery, and under
     ED-0045 the pair whose row is being certified does not author the gate
     that certifies it.
- **Two routings recorded, both found by issue 0005's reviewer:**
  - **To issue 0028** — the derived-row-set fix above, alongside its existing
    vacuous-checker scope and the ED-0045 vector dimension.
  - **To issues 0031 and 0018** — recording in §8 that v0.1 is unreleased
    exposes opposite claims in migration code and tests and in the ADR and
    release audit. Issue 0031 already records that the production migration
    chain is honestly empty and that no independently evidenced pre-v0.1
    public release artifact exists; the contradiction is therefore in the
    surrounding claims, not in the new §8 sentence. Issue 0005 writes the §8
    text and routes the contradictions; it does not reconcile them.
- **The third r2 correction is not a routing** and is recorded only so the
  chain is legible: C(a) must be 0 when the actor is absent, and fresh logs
  are underdetermined. That is a correction to issue 0005's own draft and is
  its to fix.
- **Reason:** Bounded and reversible; allocates data to the issue that
  invalidated it and machinery to the issue that audits machinery, and
  prevents a row landing under a gate that structurally cannot see it.
- **Validation/reversal gate:** Discharged when issue 0005's branch carries an
  L33 `rowAudit` entry with real coverage and an unbumped loop bound, and when
  issue 0028 later replaces the literal with a derivation and demonstrates the
  audit failing on a row the specification has but the corpus has not.

### ED-0048: ED-0047 item 1 was not executable; L33 ships uncovered and declared; the absent-actor prefix is an owner question

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** 0005, 0028; owner question affecting 0005, 0028, 0029
- **Amends:** ED-0047 item 1, which was wrong.
- **Blocker 2 — the orchestrator missed a second literal.** ED-0047 told issue
  0005 to add an L33 `rowAudit` entry, having found only the completeness loop
  at `corpus_test.go:650`. There is a second literal and it is an ACCEPTANCE
  bound: `corpus_test.go:307`,
  `` var limitRowRe = regexp.MustCompile(`^L([1-9]|[12][0-9]|3[0-2])$`) ``.
  Verified against the real symbol: `L32` matches, **`L33` does not**. It
  guards `rowAudit` rows at `:634` and `boundRows`/`rowRefs` at `:982`,
  `:1052` and `:1085`. So the entry ED-0047 required would have turned
  `verify.sh` RED, and no corpus record could cite L33 either, which makes
  `covered` impossible and any such claim false. **ED-0047 item 1 was
  therefore not executable as written, and the pair was right to stop rather
  than widen a literal to satisfy it — widening literals is what item 2
  exists to prevent.**
- **Decision on blocker 2 (issue 0005's recommended option (b)):** Issue 0005
  ships the §9.3 L33 row and the `parameters.json` prose correction only. The
  L33 audit entry, the acceptance regex, and the derived-row-set replacement
  for the completeness loop all go to **issue 0028** as one piece of work,
  because they are one piece of machinery and ED-0045 forbids the pair whose
  row is certified from authoring the gate that certifies it. Issue 0005's
  handoff MUST state plainly that **L33 is knowingly uncovered and that its
  green `verify.sh` does not audit L33**, because the completeness loop stops
  at 32. A declared gap is the honest form of a gap that would otherwise be
  silent; an undeclared one is the defect this whole thread is about.
- **Blocker 1 — a real normative ambiguity, correctly routed, and it is the
  OWNER's.** Issue 0005 was directed to fix `C(a) = 0` for an absent actor and
  found that doing so rejects committed golden SUCCESS oracles. Verified:
  `testdata/corpus/golden/packages/thin-valid.domo` carries a snapshot frontier
  of `{}` with zero entities alongside one retained entry at `counterBase 42`
  whose own frontier is `{A:41}` — its own prior end, exactly as F4 requires.
  Counters 1–41 are asserted by the batch, covered by nothing and retained by
  nothing. Four of the 38 committed packages share this shape at range [42,42],
  three with outcome `success`.
  - Reading one, `C(a) = 0`: the first range above the covered prefix must
    begin at 1, it begins at 42, so the package REJECTS and three committed
    success oracles flip.
  - Reading two, implied prefix `C(a) = r1.base - 1`: satisfied by
    construction, which makes the head-of-stream check vacuous for absent
    actors — the ED-0041 shape this issue is supposed to be removing.
  Neither is available to the orchestrator: one invalidates committed
  evidence, the other blesses a check that cannot fail. **The underlying
  question is whether the format accepts a partial-history package at all**,
  which is adjacent to ED-0040 item 5's unresolved save-drops-prior-history
  question, and it is recorded here as an open OWNER question.
- **Bounded interim, adopted from issue 0005's proposal:** §7.6 rules fully
  for S2-NAMED actors, all four clauses biting and mutation-proven, and records
  the absent-actor case explicitly as UNRULED with both readings named —
  following §4.4 F4's own precedent of stating a verification limit
  deliberately rather than pretending to a guarantee. This changes no outcome,
  flips no fixture, and creates no vacuous check.
- **Routed to issue 0028:** examine the four packages. Independent of the
  ruling, `thin-valid.domo` being simultaneously the §7.5 brand-new-document
  shape and carrying a batch claiming 41 prior operations is a fixture
  integrity question, and a golden success oracle whose validity depends on an
  unruled question must at minimum be labelled as such.
- **Reason:** One half corrects an orchestrator error and reallocates
  machinery already allocated by ED-0045's rule; the other refuses a decision
  whose either answer costs something the orchestrator has no authority to
  spend.
- **Validation/reversal gate:** Discharged for blocker 2 when issue 0005's
  handoff declares the uncovered row and issue 0028 later lands the regex,
  entry and derivation together. Blocker 1 stays open until the owner rules.

### ED-0049: The seeded-versus-full reject divergence is a known V6 nonconformance, not a limitation

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** 0011, 0004, 0005, 0028
- **The gap, as issue 0011 reported it while implementing ED-0037 comment (l).**
  A late batch is LIVE at its write position (I5 branch 2) while its target is
  already tombstoned in the seeded replay root, because a canonically later
  delete is folded into the snapshot. LWW3 supersedes that one write, so it
  materialises nothing while the batch still applies and its other writes land
  — comment (n)'s falsified case, implemented. But FULL replay reaches that
  write with the entity PRESENT, so it runs payload schema validation, and a
  schema-invalid `setFields` rejects the WHOLE batch under §4.3 row 1 with zero
  effects. SEEDED replay has no entity to validate against: a tombstoned entity
  is not in S1's live set, and S4 retains `entityId`, deleting position and
  creation position with **no entity KIND**. So seeded accepts, suppresses the
  write, and applies the batch's other writes. Full produces an error and no
  effects; seeded produces no error and some effects.
- **It is recorded as a NONCONFORMANCE, not a documented limitation.** V6
  requires the snapshot projection to equal full-history replay. These paths
  diverge for an input class, and the softer word would let it be read as an
  accepted characteristic. It is the same weight as ED-0032, and it is recorded
  with a gaps record and a `residualBlockers` entry exactly as the V2 blocker
  was, so the corpus states what it does not know about itself.
- **The direction of the divergence is the security-relevant part.** Seeded is
  MORE permissive than full: a compacted package accepts a batch an uncompacted
  replica rejects. **Compaction weakens validation.** That is a property a
  hostile-input corpus must own, so it goes to issue 0028 as a hostile fixture
  rather than living only in a code comment.
- **"Unreachable by a conforming writer" is explicitly REJECTED as a
  resolution**, and issue 0011 was right to refuse it itself. O4 validates at
  authoring time, so this needs a non-conforming or hostile author — which is
  precisely the threat model the conformance and security corpus exists for. It
  is also the reasoning that produced ED-0040 queue item 8 and several of this
  session's other defects.
- **Issue 0011's interim is ADOPTED as implemented:** the suppressed write
  records its winner position and skips payload validation, with the gap stated
  in the code comment and the handoff. Recording the position is forced rather
  than chosen — full replay records it before the delete and a delete does not
  erase winner positions, so suppressing it would diverge on exactly the
  metadata V6 compares. The alternative, rejecting whenever a suppressed write
  cannot be validated, would break V5 and V6 for the ordinary VALID case and is
  therefore worse; the reject case is a narrow divergence, the valid case is
  every compacted package.
- **The remedy is named and routed, not invented here.** ED-0037 comment (q)
  already observed that "payload/schema support can stay bounded at worst at
  entity kind or schema per tombstone". That is a §5.1 S4 snapshot member
  addition: normative, issue 0004's to specify with issue 0005 placing any
  container wording, and **added to ED-0040's standing queue**. Issue 0011 may
  not invent a snapshot member, and correctly did not. Because v0.1 is
  unreleased and the predecessor set is empty, the addition is cheap whenever
  it is taken up.
- **Reason:** Bounded and reversible. It classifies an implementation outcome
  honestly, routes a normative remedy to its owner, and refuses to let an
  unreachability argument close a hole in a format whose corpus exists to model
  hostile authors.
- **Validation/reversal gate:** Discharged when §5.1 S4 carries the bounded
  kind or schema datum and the seeded path validates the suppressed write, at
  which point issue 0028's hostile fixture must flip from divergent to
  agreeing. Until then the gaps record stands and no V6 claim may be made for
  the reject case.

### ED-0050: The size-corpus regeneration gate is scoped, not weakened; the exemption must be mutation-proven

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** 0011, 0035, 0018
- **The block, verified.** `internal/sizecorpus` `TestCommittedArtifactsConsistent`
  (`sizecorpus_test.go:284`) has two halves. **T8a** re-reads each committed
  package and checks its digest and byte count against the committed budget
  record — self-consistency of the evidence, passing and unaffected. The second
  half regenerates each package with the CURRENT writer and requires byte
  equality (`:333`), guarded by `GOTOOLCHAIN=local` and commented "Package
  byte-reproducibility is host-independent, so it is never scoped by
  environment conformance." The widened snapshot member set makes the
  regenerated bytes differ from the committed `empty-v1` and
  `unfurnished-1000-v1` artifacts. `TestCommittedPackagesRoundTrip` **passes**:
  both packages still load and round-trip as legacy single-actor snapshots
  under ED-0038's convention — which is exactly the cost the owner weighed when
  choosing that convention over rejection.
- **The gate is now asserting something the project has already decided is
  false.** ED-0037 comment (a) recorded the disposition that these two packages
  stay as pre-ED-0037 development artifacts, are not re-labelled or
  regenerated, and that issue 0035's Part A measurements must be re-measured
  once the widening lands (ED-0040 queue item 6). What nobody wrote down is
  that a byte-equality regeneration gate sits between the widening and that
  re-measurement, making item 6 a PRECONDITION for a green branch rather than
  a follow-up. Issue 0011 found that and escalated instead of resolving it.
- **Decision — option (A), scoped precisely:**
  1. **T8a stays unconditional for every artifact.** The half that stops
     committed evidence drifting from its own digest and size record is not
     touched.
  2. **The regeneration comparison is scoped by an EXPLICITLY ENUMERATED
     exemption** naming exactly the two pre-ED-0037 artifacts, with an
     in-code citation of ED-0037 comment (a) and ED-0040 queue item 6. It must
     NOT be a predicate, and it must NOT key off the records' `formatVersion`
     field — see below. An enumerated list makes adding a third artifact a
     conscious edit; a predicate would swallow future ones silently.
  3. **No committed `docs/evidence/` byte changes.** The brief's prohibition
     stands; this is a test-scope change, not an evidence change.
  4. **The exemption dies with queue item 6** and is temporary by construction.
- **MANDATORY GUARDRAIL — the exemption must be mutation-proven.** Issue 0011
  is scoping a gate that currently blocks its own branch. That is a real
  conflict of interest even though the ruling is right, so the pair must
  construct an artifact under the CURRENT format whose bytes differ and show
  the gate still going RED, with the transcript in the handoff. Without that
  proof, scoping is indistinguishable from disabling, and the distinction is
  the entire content of this decision. ED-0045's rule that the pair changing
  semantics must not author the certifying gate does NOT bite here: this gate
  certifies evidence reproducibility, not the correctness of issue 0011's
  ordering. ED-0043's authorship line does — issue 0011 owns what its change
  invalidated.
- **A NEW defect found while ruling, routed to queue item 6.** `records.go`
  sets `FormatVersion: FormatVersionV01` from the constant `"0.1"`. Because
  v0.1 is unreleased, ED-0037 changed the ENCODING without bumping the declared
  version, so the committed pre-widening records and the current writer both
  declare `"0.1"`. **The recorded `formatVersion` therefore no longer
  identifies the encoding of the artifact it describes.** That is why the
  exemption must be enumerated rather than version-keyed, and whoever performs
  the re-measurement must resolve it before the exemption can be retired
  safely.
- **Options (B) and (C) are rejected**, as issue 0011 judged. (B) regenerating
  on this host contradicts ED-0037's owner-level disposition and would
  invalidate issue 0035's measured budgets. (C) giving issue 0011 the narrow
  re-measurement contradicts its exclusions and would take measurements on a
  host whose authority is itself an open owner question (ED-0030).
- **Reason:** Bounded and reversible, and it implements an existing owner
  disposition rather than making a new one. Stating a preference while
  escalating, as issue 0011 did, is the behaviour wanted — it gives the ruling
  something to test rather than making the orchestrator start from nothing.
- **Validation/reversal gate:** Discharged when the handoff carries the
  red-gate mutation transcript, and retired entirely when queue item 6
  re-measures on the authorised host and the `formatVersion` defect is fixed.

### ED-0051: Dispatch issue 0031 on the released-schema contradiction issue 0005 exposed

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** 0031, 0018, 0028, 0029
- **Decision:** Issue 0005 is integrated (`4e77c12`), so a slot is free under
  ED-0044's cap of four. It is filled with **issue 0031**, on work that did not
  exist this morning: issue 0005's new §8 text records that v0.1 is UNRELEASED,
  and `internal/migration` asserts the opposite in three places —
  `migration.go:13-15` ("exactly one schema (`0.1`) has been publicly
  released"), `migration.go:43-50` (`releasedSchemas` documented as "every
  publicly released schema" and initialised to `{"0.1"}`), and
  `migration_test.go:109-117` pinning that set. Pair `w0031`/`r0031` in
  workspace `w30` on branch `issue/0031-released-schema-honesty`.
- **It is not a comment fix, and the brief says so.** `LatestReleasedSchema()`
  indexes `releasedSchemas[len(releasedSchemas)-1]`, so an empty released set
  is a panic unless the contract changes, and `IsReleasedSchema` gates
  behavior. The honest model has to be established before anything is edited.
  The brief also names the specific hazard: `TestShippedRegistryHonesty` exists
  to stop a fabricated migration chain, and correcting a false premise inside
  it could quietly remove that protection.
- **The remaining two slots stay empty, with reasons, per INSTRUCTIONS.md.**
  - **Issue 0029** is now unblocked normatively — issue 0005 ruled the §7.6
    domain unconditional and `assetRoot` `null` as `invalid-field-syntax`, so
    both fix directions it was waiting on exist. It is held anyway on FILE
    collision: issue 0011's branch already touches
    `internal/persist/reader.go` and `internal/persist/wire.go`, which are
    exactly issue 0029's two remaining items under ED-0046. Dispatching now
    would put two pairs in one file with semantics in flux.
  - **Issue 0028** is unchanged from ED-0044: its scope is the corpus that
    issue 0011 is rewriting, now with the ED-0047/ED-0048 machinery added.
  - **Issue 0018** consumes issue 0031's answer. Its half of the same
    contradiction — ADR 0007 lines 83-99 and `AUDIT.md` lines 24-47 — cannot
    honestly be settled before the released-schema model is.
  Both free slots therefore open the moment issue 0011 lands, and issue 0018's
  opens when issue 0031 does.
- **Reason:** Ordinary scheduling under the owner's raised cap, filling a slot
  with work that is genuinely ready and genuinely isolated rather than with
  work that merely exists.
- **Validation/reversal gate:** Unchanged review contract. Re-run the
  readiness audit when issue 0011 lands.

### ED-0052: Standing auto-refill — slots fill on blocker clearance, not on owner prompting

- **Date:** 2026-08-29
- **Status:** Active
- **Decided by:** project owner
- **Affected issues:** all scheduling
- **Decision:** The owner directed that new issues be taken on as soon as
  bandwidth up to four is available. Refill is therefore **automatic and
  immediate**: the moment any pair reaches a durable disposition, the
  orchestrator re-runs the readiness audit within the same turn and dispatches
  into every freed slot whose blocker has cleared, without waiting for an owner
  prompt and without a fresh decision record for a dispatch already
  pre-authorized below. An owner reminder needed to discover an idle slot is an
  orchestration incident under INSTRUCTIONS.md, recorded as one.
- **Pre-authorized dispatches, so there is no decision latency when a blocker
  clears.** These need no further decision record; the orchestrator dispatches
  and notes it in the observatory:
  1. **On issue 0011 integrating → dispatch issue 0029 AND issue 0028**, which
     fills both currently-empty slots at once. Issue 0029 is already unblocked
     normatively by issue 0005's rulings and is held only because issue 0011's
     branch touches `internal/persist/reader.go` and `internal/persist/wire.go`.
     Issue 0028 is held only because issue 0011 is rewriting the corpus it
     audits; it also carries the ED-0047/ED-0048 machinery and the ED-0049
     hostile fixture.
  2. **On issue 0031 integrating → dispatch issue 0018**, whose half of the
     released-schema contradiction (ADR 0007 lines 83-99, `AUDIT.md` lines
     24-47) cannot be settled before issue 0031 fixes the model it audits.
- **Why the two free slots are empty right now, audited 2026-08-29 after issue
  0005 integrated.** Every remaining open issue was re-checked, not assumed:
  - Issues 0028, 0029, 0027 — file collision with the live issue 0011 branch.
    Issue 0029's entire remaining scope is two files issue 0011 is editing, so
    there is no non-colliding subset to dispatch.
  - Issue 0018 — would be auditing a moving target; both issue 0031 and issue
    0011 change what it audits.
  - Issue 0021 — AC-6 remains gated on issues 0004, 0005 and 0035, and its work
    is corpus records, which is issue 0011's live file set.
  - Issue 0026 — no open work; its remediation landed 2026-08-25.
  - Issues 0006, 0007, 0013, 0022, 0030 and everything downstream of 0013 —
    the two owner decisions of ED-0044.
  - Issue 0034 — needs interactive registry sessions and owner authorization to
    reserve namespaces.
  - **Issues 0024 and 0025 — host-infeasible, recorded here because it had not
    been written down.** Both target Linux and Android rendering and native
    packaging; this is a macOS host, so their acceptance criteria cannot be
    exercised here at all. Issue 0025 additionally depends on issue 0024, which
    is itself awaiting a ledger-before-reliance pass.
- **Reason:** Direct owner instruction on scheduling. Pre-authorizing the next
  dispatches is what converts the instruction into behavior — the constraint
  has never been willingness to fill a slot, it has been the round trip
  between a blocker clearing and a decision being recorded.
- **Validation/reversal gate:** Every pair completion must be followed in the
  same turn by either a dispatch or a written reason no slot could be filled.

### ED-0053: Non-discriminating proof — a distinct sub-species of ED-0041 with a different remedy

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** all; ED-0041 companion
- **The pattern, from three independent instances in one day.** ED-0041 names
  a check that CANNOT FAIL. This is its harder relative: a proof that DOES
  fail when mutated, and therefore looks sound, but fails **for a reason other
  than the property it claims to establish**. Three pairs produced one each,
  none aware of the others:
  1. **Issue 0005** — an overflow witness comparing 2⁶³+1 against 2⁶³+2. Both
     wrap to adjacent negatives, so order is preserved and the mutant passes;
     the witness demonstrated nothing about the accumulator. Caught by r0005.
  2. **Issue 0011** — a differential proof between an optimized and a naive
     closure checker. Both fold `End <= threshold` and both accept a frontier
     pointing inside an atomic batch, so they AGREE while both are wrong. A
     differential proof is only as strong as its oracle, and this oracle shared
     the misreading. Caught by r0011.
  3. **Issue 0031** — an unfillability mutation whose fixture used an
     unsupported target, so deleting the release gate under test still
     rejected, via the unrelated support check. Caught by r0031.
  Every one was caught by the adversarial reviewer, never by the author, and
  in each case the author had explicitly set out to avoid ED-0041.
- **The remedy is different from ED-0041's and must not be confused with it.**
  For a vacuous check the fix is to make it capable of failing. For a
  non-discriminating proof the check already fails; the fix is to **isolate the
  variable**: construct the witness so exactly one thing differs between the
  passing and failing runs, and confirm the outcome tracks THAT thing. In
  practice this means a control alongside the witness — issue 0005 arrived at
  this independently, pairing an overlapping covered range that must reject
  with a contiguous covered range that must accept, which is what separates
  "rejects the overlap" from "rejects covered entries at all".
- **Requirement, effective immediately for every pair.** A mutation proof is
  not complete until its author has stated what ELSE could have produced the
  observed failure and shown that it did not. Where a control is possible, the
  proof carries one. This is added to the standing brief.
- **Reason:** Three independent occurrences in a single day, all by authors
  actively trying to avoid the neighbouring failure, is evidence that
  understanding ED-0041 does not prevent this one — which is exactly ED-0041's
  own conclusion about lessons versus mechanisms, applied to itself.
- **Validation/reversal gate:** None; like ED-0041 this is discharged only by
  being mechanised in what reviewers require, not by being understood.

### ED-0054: ED-0050's guardrail is unmet; eight read-path defects; a green verify proved less than it looked

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** 0011, 0028
- **The guardrail I made mandatory was itself non-discriminating.** ED-0050
  required issue 0011 to prove the scoped size-corpus regeneration gate still
  goes RED, precisely because the pair was scoping a gate that blocked its own
  branch. The delivered `TestScopedRegenerationGateStillFails` drives the gate
  with **arbitrary byte strings** rather than a generated valid CURRENT-format
  artifact. Arbitrary bytes differ from a committed artifact for reasons that
  have nothing to do with the exemption, so the transcript shows the gate
  rejecting garbage, not the gate noticing a real artifact that fails to
  regenerate. **It does not satisfy ED-0050.** The proof must use an artifact
  produced by the current writer under the current format, differing from its
  committed counterpart, so the only variable is the one the exemption governs.
  This is ED-0053's pattern inside the guardrail written to prevent ED-0041 —
  the third layer down.
- **The delegation is what caught it, and that is the reusable part.** The
  orchestrator read the worker's summary ("proving the gate still bites in four
  directions") and moved on; it did not open the witness. What caught it was
  the instruction to the reviewer to VERIFY that specific proof rather than
  accept it, and to treat it as the highest-value thing in the branch because
  it was the one place a green run could mean less than it appeared to. Naming
  the weakest point in advance and assigning it to an adversary worked where
  the orchestrator's own reading did not.
- **Eight independent read-path defects**, found by reviewer probes under Go
  1.27 and currently untracked in `internal/document/reviewer_probe_test.go`:
  rollback/applied-ID poison; rejected-position leak; a panic on a create
  collision against a missing live creation position; a causally ordered
  duplicate create accepted as an LWW4 collision; three impossible snapshot
  positions accepted; an inverted interval accepted; a covered retained
  frontier not authenticated; and a retained one-operation reference accepting
  index 1. All are issue 0011's: it wrote the widened snapshot parse and the
  lifecycle branches these exercise. Every probe becomes a tracked regression
  test — an untracked probe is evidence that expires.
- **Two of them are not ordinary bugs and are called out separately.**
  - The **causally ordered duplicate create accepted as LWW4** is exactly the
    case ED-0040 queue item 8 fixed in the specification (`3cca9ee`): §4.5 now
    routes only CONCURRENT collisions to LWW4, because a causally ordered
    same-id create means the later author observed the first and O4 rejects it
    at authoring. The implementation accepting one is non-conforming against
    text accepted four days ago, and it is reachable by a hostile author — the
    same reasoning ED-0049 already refused to accept as a resolution.
  - The **panic on a create collision against a missing live creation
    position** is the failure mode of the relaxation ED-0011's point 7
    described and the orchestrator endorsed. **That endorsement stands on the
    text and is amended in scope, not withdrawn:** §5.1 S1 scopes
    `createdPositions` to entities within the horizon, so absence remains
    legitimate and rejecting would refuse conforming snapshots — but absence
    must DEGRADE, never crash. A reader that panics on a shape the
    specification permits is a worse outcome than the rejection that was
    replaced.
- **A green `verify.sh` is not a correctness claim, and this is the cleanest
  demonstration the project has produced.** The orchestrator ran the full suite
  on this branch independently and it exited 0 with a clean tree, while eight
  defects sat in the implementation. It exits 0 because the committed tests
  pass; the probes that fail were never committed. Nothing about the run was
  wrong — the inference "green, therefore correct" is what is wrong, and it is
  worth recording that the orchestrator's own verification round produced
  exactly that false comfort.
- **Reason:** Records a failed guardrail honestly, keeps the scope of an
  earlier ruling intact while fixing its implementation, and preserves the
  reason the delegation caught what direct reading did not.
- **Validation/reversal gate:** Discharged when the regeneration proof uses a
  generated current-format artifact, all eight probes are tracked tests that
  have each been shown red before green, and the LWW4 case is checked against
  §4.5's post-`3cca9ee` text rather than against the implementation's habit.

#### ED-0054 amendment (a) — three further defects, and the generator is upstream of the guardrail

- **Date:** 2026-08-29
- Issue 0011's reviewer added persistence probes
  (`internal/persist/reviewer_probe_test.go`, untracked during review) and
  confirmed three more, raising the wave to eleven:
  1. **The acceptance parser violates §5.1's "contains ALL of".** It accepts
     `{}`, accepts a lone new-family member, and accepts an otherwise complete
     current snapshot with any ONE of `entities`, `frontier`, `batchPositions`,
     `winnerPositions`, `createdPositions`, `tombstones` or `batchMetadata`
     omitted. Nine missing-member controls are red; the complete empty current
     shape passes, which is the control that makes the nine mean something.
  2. **`Inspect`-only reporting defect.** `decodeSnapshotSummary` claims to
     report both families structurally, but a present empty `winnerPositions`
     hides a co-present `winnerKeys`/legacy member and `deletingPosition` hides
     a co-present `deletingKey`. The reviewer explicitly declined to state this
     as mixed-family acceptance, because `Load` rejects mixed families
     correctly — the defect is in what `Inspect` reports, not in what the
     reader accepts.
  3. **`internal/corpusgen/generate.go`'s `emptyWriterSnapshot` still emits
     legacy `winnerKeys`/`createdKeys` and omits `batchPositions` while
     claiming to match `buildSnapshotJSON`.**
- **Defect 3 is upstream of ED-0054's own remedy and must be fixed first.**
  ED-0054 requires the regeneration guardrail to be rebuilt against a genuine
  current-format artifact. A generator that claims to match the writer and does
  not is exactly the thing that makes such a proof non-discriminating by
  construction, so a guardrail rebuilt on `emptyWriterSnapshot` as it stands
  would fail ED-0050 a second time for a new reason. Fix the generator, then
  build the proof.
- **Defect 1 is a RECURRENCE, and that is the part worth recording.** Accepting
  `{}` is ED-0041's instance three — the empty-map vacuity found by the
  previous reviewer in this same seat during the compatibility wave. The shape
  came back in new code written by an author who had read the record naming it.
  That is the strongest evidence yet for ED-0041's own conclusion: the lesson
  does not transfer by being understood, only by being mechanised into a check.
  The remedy here is a required-member assertion the parser cannot pass without,
  not a note.
- **Scope:** all three are issue 0011's under ED-0043 — it widened
  `buildSnapshotJSON`, which is what made the generator's claim false, and it
  wrote the acceptance parser and touches `inspect.go`. None of them is issue
  0028's reserved machinery (`limitRowRe`, the completeness loop, `rowAudit`).

#### ED-0053 amendment (a) — a differential oracle certifies only within its generator's reach

- **Date:** 2026-08-29
- **Second instance, different cause.** Issue 0011's closure differential
  oracle was already recorded as failing once, when the naive and optimized
  checkers shared one blind spot and therefore agreed while both were wrong.
  It has now failed a SECOND time from an unrelated cause: an off-by-one that
  folds every ancestor except the LAST passes the differential, because the
  generator rarely places the deciding ancestor in that position. The two
  implementations do not share this gap — the GENERATOR does not reach the
  shape that would expose it.
- **The generalisation, which is worth more than either instance.** A
  differential oracle certifies an implementation only across the shapes its
  generator actually reaches, and agreement outside that reach is not evidence
  of anything. So a differential test is a coverage instrument, never a
  correctness proof, and it must be paired with hand-built witnesses aimed at
  the specific boundaries the generator is unlikely to hit. Two independent
  failures of the same oracle in one branch, from two unrelated causes, is
  enough to state this as a standing expectation rather than an observation.
- **ED-0041's thesis has a new data point supplied by an author against
  itself.** Issue 0011 swept its OWN new tests for the shape rather than
  waiting for a third report, and found two containing vacuous loops: each
  scanned a map for a forbidden value, which examines nothing when the map is
  empty, so both would have passed against a reader returning no positions at
  all. That is ED-0041 instance three — the same empty-collection vacuity —
  reproduced by an author who had fixed exactly that defect in the acceptance
  parser earlier in the same session and cited it twice in commit messages as
  a lesson absorbed. **Recognising the shape did not prevent reproducing the
  shape, and the author demonstrated it on itself.** ED-0041 says the lesson
  transfers only by being mechanised; this is the cleanest evidence for that
  claim the project has, and it exists because the author went looking for the
  assumption rather than the reported line.

#### ED-0053 amendment (b) — corrections supplied by the author the record was about

- **Date:** 2026-08-29
- **Amendment (a) was kinder to issue 0011's author than the facts, and the
  author said so.** (a) records the two vacuous loops as reproduced by someone
  who "had fixed exactly that defect in the acceptance parser earlier in the
  same session and cited it twice in commit messages as a lesson absorbed",
  which leaves open that the loops predated the citations. They did not. **The
  loops were written AFTER the parser fix and after both citations.** The
  sequence is therefore not "absorbed a lesson, then slipped later" but
  **"stated the lesson, then reproduced it while stating it"**. That is the
  harder version and the stronger evidence for ED-0041's mechanisation claim,
  and it is recorded here at the author's own request over a wording that
  favoured it.
- **The oracle's two failure causes have DIFFERENT remedies and must not be
  conflated.** Amendment (a) recorded both failures without separating what
  fixes each:
  - **Shared blind spot** (the first instance, found by r0011): the naive
    oracle carried the same misreading as the implementation, because it was
    re-implemented from the same understanding. The remedy is INDEPENDENCE of
    derivation — an oracle that cannot be two copies of one mistake.
  - **Generator reach** (the second, found by the author against itself): the
    two implementations do NOT share the gap; the generator simply never
    places the deciding ancestor where the off-by-one shows. **Independence
    does not help here at all.** The only remedy is hand-built witnesses aimed
    at the boundaries a generator is unlikely to reach.
  A reader who conflates them reaches for the wrong repair, which is why the
  distinction is recorded rather than left as two instances of one heading.
- **A process distinction worth keeping, also the author's.** It asked whether
  to freeze the branch while continuing to commit to it. Its own assessment:
  *asking is not the same as having waited*, and the commits being good ones
  is precisely what makes the drift hard to audit — a reviewer chasing
  improvements can state what it approved no more easily than one chasing
  regressions, and "the commits were good" is not verifiable after the fact
  without re-reading every byte. The rule this settles: a question about
  whether to stop is answered by stopping until the answer arrives.

### ED-0055: Ownership of four inherited gaps found in issue 0011's frozen range

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** 0011, 0004, 0005, 0029
- **Context.** Reviewing the frozen range `699c62b..8185691`, issue 0011's
  reviewer confirmed two DIRECT blockers in code this branch wrote —
  `authenticateAgainstBatches` validating frontier equality and index bounds
  but not SEMANTIC binding, so replay accepts `createdPositions[E1]` pointing
  at a retained create of E2 (and the same for winner positions and
  tombstones), contradicting §5.1 S1/S3/S4; and ED-0038 tombstone recovery
  returning the FIRST retained create for an id, so a
  create/delete/restore/delete history records the original create as the final
  tombstone's live-interval start, with a legacy `CreatedKeys` entry able to
  overwrite retained evidence. Those are unambiguously this branch's. It also
  confirmed four gaps that PREDATE the range and asked for ownership rather
  than assuming it. That request is the correct instinct and this decision
  answers it.
- **The governing distinction is not who touched the file, it is whether an
  ACCEPTANCE CRITERION is being closed over a known live defect.** An inherited
  defect does not block a branch that did not introduce it. Claiming the
  criterion it contradicts IS met is the actual failure, and it is the one to
  prevent.
- **Classifications:**
  1. **S5 `recentBatchIds` parsed then discarded before `Projection`, so a
     batch reusing an S5 identity is accepted — ISSUE 0011's, but NOT this
     continuation's.** It defeats S5's dedup purpose and therefore bears
     directly on issue 0011's own criterion that "duplicate and out-of-order
     batches behave according to conformance vectors". It is not in ED-0040
     queue item 2 or issue 0004's routed list, so adding it now is scope creep
     onto a frozen branch. Record it as an OPEN criterion on issue 0011:
     **the duplicate/idempotent-replay criterion may not be marked met while
     this stands.**
  2. **`FromSnapshot` accepts dangling authoritative entity references — ISSUE
     0011's, NOT this continuation's**, on the same reasoning: it bears on the
     snapshot-replay criterion, predates the widening, and blocks that
     criterion from being claimed rather than blocking this integration.
  3. **Split, because the two halves are different kinds of defect.**
     - **The panic**: a malformed `batchId` reaching `mustUUID` crashes. A
       reader must never crash on a shape it is supposed to REJECT — the same
       principle ED-0054 applied to the create-collision panic. Issue 0011's,
       not this continuation's.
     - **The create envelope `entityId` disagreeing with `fields.id` and being
       silently replaced is NORMATIVE and is ROUTED, not classified.**
       FORMAT-SPEC line 385 gives an operation `kind`, `entityId` and a
       kind-specific payload, and says nothing about whether the two identities
       must agree or which wins. That is the same shape as the §7.6
       name/content-binding question issue 0005 settled today, and it belongs
       to §4's owner (issue 0004) with issue 0005 placing any container
       wording. **Silent replacement is not defensible as an implementation
       choice, but choosing the rule is not issue 0011's.**
  4. **Missing batch frontier accepted — already routed to issues 0005 and
     0029** by the compatibility evidence at line 849. The reviewer declined to
     charge it to issue 0011 and that is correct; no action.
- **Sequencing, so the branch is not thawed twice.** The reviewer finishes its
  review of the frozen range and issues its verdict with the COMPLETE blocker
  list. Only then does the branch unfreeze for a single fix pass, after which
  it refreezes at a new SHA for re-review. Repeated freeze/thaw cycles
  reintroduce exactly the drift the freeze exists to prevent.
- **Reason:** Bounded and reversible; allocates work without moving semantics,
  and separates "this branch must fix it" from "this issue must not claim it".
- **Validation/reversal gate:** Discharged when issue 0011's handoff explicitly
  names the duplicate/idempotent-replay and snapshot-replay criteria as NOT
  met and says why, and when the envelope-identity question reaches issue 0004
  as a routed normative question rather than an implementation decision.

### ED-0056: Three transactional leaks are one missing invariant; and my atomicity ruling was scoped, not wrong

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** 0011, 0028
- **The third leak, and it is the most serious.** Issue 0011's reviewer
  confirmed that `p.restorable` leaks across a rollback: `deliverRestoring`
  stores authorization BEFORE replay and `restoreState` does not roll it back.
  The reproducer is red — create/delete E; a private restoring batch carrying
  `create(E)` plus a dangling wall fails during replay; an ordinary public
  `Deliver` of different content reusing that uncommitted `batchId` then
  restores E **using the leaked authorization**. Entity and log state roll back
  correctly; only CAPABILITY state leaks. A capability surviving a failed
  transaction is a different and worse class than a stale value surviving one.
- **Three leaks of one kind is not three bugs.** With the rollback/applied-ID
  poison and the rejected-position leak already recorded, this is the third
  piece of state mutated before the commit point that rollback does not
  restore. The defect is therefore a MISSING INVARIANT, not three oversights:
  **rollback must restore every piece of state mutated before the failure
  point, including state that is not entity state.** Fixing three fields
  leaves the fourth to be found by whoever next writes a pre-commit mutation.
  The remedy is an invariant asserted over the enumerated mutation set, with a
  per-field mutation proof, so a new pre-commit mutation cannot be added
  without the check noticing.
- **This corrects my own earlier ruling, and the correction is instructive.**
  Issue 0011 discharged my replay-atomicity question with
  `TestReplayStageFailureIsAllOrNothing`, which I verified in the tree and
  accepted as decisive. It requires the projection, the delivered log and the
  applied-identity set to be exactly as before. **It does not assert
  `restorable`.** So the test settles atomicity for the state it ENUMERATES,
  and I read an enumerated claim as a general one. The test is not wrong and
  the session-test rewrite it justified still stands — partial acceptance of
  entity state remains unreachable. What is now false is the broader reading I
  gave it. This is the same failure the branch keeps producing, in my own
  acceptance: **a check whose scope is narrower than the claim it appears to
  establish.** An atomicity assertion must therefore derive its enumeration
  from the actual set of pre-commit mutations, never from the fields that came
  to mind while writing it.
- **Consequences:** the atomicity test is extended to the full mutation set,
  not to `restorable` alone; and issue 0028 gains a hostile fixture for
  capability leakage across rollback, since a failed batch conferring authority
  on a later unrelated one is exactly the corpus's threat model.
- **Reason:** Records a class rather than three instances, and corrects an
  orchestrator ruling that was accurate about the evidence and wrong about its
  reach.
- **Validation/reversal gate:** Discharged when the rollback invariant is
  asserted over an enumerated pre-commit mutation set with per-field mutation
  proofs, and when adding a new pre-commit mutation without extending the
  enumeration is shown to fail.

#### ED-0056 amendment (a) — the enumeration IS the claim, and the blame is two layers not one

- **Date:** 2026-08-29
- **Issue 0011's author corrected the correction, and it is right in one
  respect and too hard on itself in another.** ED-0056 recorded the failure as
  the orchestrator reading an enumerated claim as a general one. The author
  points out the enumeration was ITS: it wrote
  `TestReplayStageFailureIsAllOrNothing`, chose the compared fields **by asking
  which state mattered rather than by reading the struct**, and wrote a
  sentence around the test broader than the test. **Both layers are real and
  they are different failures**, so both are recorded rather than one replacing
  the other: the author scoped evidence by recall and described it without
  saying so; the orchestrator verified that the test did what it said without
  asking whether what it said covered the claim. Neither absolves the other,
  and collapsing them into one would lose the more useful half.
- **The generalisation is the author's and it is better than the original
  ruling.** *Any assertion of the form "X is unchanged" has a hidden
  enumeration, and the enumeration IS the claim.* That is why the remedy is a
  reflection guard rather than adding the missed field to a list: a list
  extended by hand is the same instrument that failed, one entry longer.
- **The accepted shape of the fix, derived from the code rather than from
  memory:** `Projection` has exactly twelve fields — seven already rolled
  back, two added by the first two leak fixes, `restorable` mutated at
  `projection.go:292` and unrestored, `log` assigned only after every batch
  applies, `baseline` not mutated by delivery. One capture/restore pair over
  that enumeration, plus a reflection test asserting the exact field-name set
  with a classification per field, **so a thirteenth field fails until it is
  classified**. That is mechanisation, not a longer list.

### ED-0057: A bounded corpus exception for issue 0018, made decidable by the freeze

- **Date:** 2026-08-29
- **Status:** Active
- **Affected issues:** 0018, 0011, 0028, 0031
- **The problem.** Issue 0018's third routed item is corpus wording in
  `testdata/corpus/migration/older-minor-unknown-label.json`. Its brief cleared
  the fixture as outside issue 0011's file set — correctly — but its INTEGRITY
  ANCHOR is not: `TestExpectationsManifest`
  (`internal/corpusgen/corpus_test.go:1018-1027`) recomputes each artifact's
  digest against the record in `testdata/corpus/EXPECTATIONS.md`, which issue
  0018's brief excludes outright because issue 0011 is live in it. The same
  record's `notes` field carries the same fused claim, so the honest fix is two
  lines in an excluded file. **Issue 0018 refused to act on "it would probably
  merge" and escalated instead — the reasoning it declined to use is exactly
  the shape its brief warns about, and declining it was correct.**
- **The freeze turned an unknowable risk into a checkable fact, which is a
  consequence of ED-0055's sequencing worth recording.** Because issue 0011 is
  frozen at `8185691`, its hunks in `EXPECTATIONS.md` are FIXED and were
  enumerated by the orchestrator: old-file lines 445-456, 635-641, 645-670,
  677-690, 700-711, 740-747, 757-764, 774-781, 1327-1333, 1507-1514,
  3055-3060, 3750-3761. The migration record sits at 996-1010 — 215 lines after
  the nearest preceding hunk and 317 before the next. Textual conflict is
  IMPOSSIBLE, not improbable. Issue 0011 also does not touch the fixture JSON,
  so its digest cannot move underneath the edit.
- **Decision — option (i), bounded exactly:** issue 0018 MAY edit the two lines
  of the `migration/older-minor-unknown-label` record in `EXPECTATIONS.md` —
  the `sha256` pin and the `notes` sentence — and nothing else in that file.
  It must state the exact line numbers it touched in its handoff. The
  orchestrator re-checks those numbers against issue 0011's hunks before
  integrating, and re-checks again if issue 0011 unfreezes and re-edits the
  file. The concept answer is already settled and is not the exception's
  subject: the fixture's detail line needs **supported**, since its record is
  category `unsupported-version` with clause §8.2 V3, which is purely the
  reader support gate and pins nothing about release status.
- **Two further observations routed, not absorbed** — both correctly raised
  without a change proposed:
  - `EXPECTATIONS.md:1040` (`migration/v4-tested-migration-path` notes) is not
    false but now omits the PRIMARY blocker: V4 binds nothing at all while the
    released set is empty (§8.2 V6). **To issue 0028**, as the owner of these
    records, not to issue 0018.
  - `internal/migration/migration_test.go:741`'s subtest name "never-released
    0.0" is true as stated but is not the reason that case rejects, which is
    the V3 support gate. **To issue 0031's next pass** as an observation; no
    change now, since 0031 settled that file today.
- **Reason:** Bounded, reversible, and decidable. The exception exists only
  because a freeze made the collision computable; without it the honest answer
  would have been to route and wait.
- **Validation/reversal gate:** Void if issue 0011 unfreezes and edits within
  50 lines of 996-1010, in which case the item routes to issue 0028 instead.

### ED-0058: Both reviewers exhausted one shared quota; replace, never self-certify

- **Date:** 2026-08-30
- **Status:** Active
- **Affected issues:** 0011, 0018
- **Supersedes for these two pairs only:** ED-0029's reviewer model assignment,
  on provider-outage grounds. Worker assignment is unchanged.
- **The incident.** `r0018` reports "The usage limit has been reached" — a hard
  provider stop, not the transient socket error it recovered from earlier.
  `r0011` is sitting on a repeated-failures prompt with the same symptom
  profile. **Both run `openai/gpt-5.6-sol`, so this is ONE exhausted quota
  taking out BOTH reviewers**, and the two live pairs lost their adversarial
  halves simultaneously.
- **Issue 0018's worker refused the tempting reading, and it was right.**
  `r0018`'s last message ended "Reply accepting these two constraints; then I
  agree and you may edit." The worker accepted both verbatim, so on a literal
  reading the condition is met. It declined to act on that, on the grounds that
  **the agreeing is the reviewer's act, not an entitlement the worker can claim
  on its behalf** — and named it as the same species of error as everything
  else this issue surfaced: a true statement ("I met the stated condition")
  standing in for a different claim ("the reviewer agreed"). That reasoning is
  adopted here as the rule: **a satisfied precondition is not a review.**
- **Decision:**
  1. **No worker may edit or self-certify past an absent reviewer**, however
     complete the scope agreement looks. Option (iii) — ruling the condition
     met and authorising the edit — is REFUSED. The orchestrator has authority
     to authorise edits, but exercising it here would launder a missing review
     rather than replace it, and this issue has already produced three
     findings where the reviewer's constraints were correct and against the
     worker.
  2. **Replace both reviewers on a separately metered provider.** ED-0015's
     comment 2 records that a naive swap failed once because
     `openai/gpt-5.6-luna` shared the same exhausted ChatGPT quota; the fix was
     a separately metered provider. Every `openai/*` model is therefore
     excluded. Replacements run `google/gemini-3.1-pro-preview`.
  3. **A replacement reviewer inherits CONTEXT, never AGREEMENT.** Prior rounds
     are input and leads, not settled conclusions. Each must independently
     re-derive its position and may reach different constraints than its
     predecessor. Told otherwise, a fresh reviewer would treat two rounds of
     "already agreed" as done and the swap would quietly become the
     self-certification refused in (1).
  4. **Issue 0011's frozen range is unchanged** at `8185691`. Its replacement
     re-reviews that range from scratch and produces the COMPLETE blocker
     list. The predecessor's findings — the semantic-binding gap, the ED-0038
     tombstone recovery defect, the three transactional leaks, the acceptance
     parser's missing-member violations — are durable in the record and are
     available as leads, each to be verified rather than assumed.
- **Reason:** A provider outage is an operational fact, not a licence to lower
  a gate. The cheapest wrong move available was to treat a met precondition as
  an agreement, and it was declined by the party it would have benefited.
- **Validation/reversal gate:** Reviewer model reverts to ED-0029's assignment
  once the shared quota resets. If a replacement reviewer reaches materially
  different constraints from its predecessor, that divergence is recorded, not
  reconciled away.

#### ED-0058 amendment (a) — what the replacement actually runs, and what it costs

- **Date:** 2026-08-30
- **Four provider attempts, recorded so the next outage is cheaper.** The
  OpenAI quota was still exhausted on re-probe more than an hour after the
  first hard stop, so it is not a short reset. `google/gemini-3.1-pro-preview`
  failed for a missing `GOOGLE_GENERATIVE_AI_API_KEY` — the host has a
  `GOOGLE_API_KEY`, and mapping it into the variable OpenCode expects
  authenticates correctly. `litellm/*` authenticates but its endpoint is
  unreachable from this host. `google/gemini-2.5-pro` is closed to new users
  and returns a hard refusal naming the preview model as its replacement.
- **Both reviewers on one contended model made each other worse.** Running two
  agents against `gemini-3.1-pro-preview` produced sustained "way too hot"
  backoff on both. They are therefore SPLIT: issue 0011's reviewer keeps
  `google/gemini-3.1-pro-preview` because its review is the deep technical one
  — nineteen commits, eleven-plus confirmed defects, three transactional leaks
  — and issue 0018's runs `google/gemini-3-flash-preview`, whose scope is
  documentation reconciliation against a specified list.
- **The cost is stated rather than hidden: a flash reviewer is a WEAKER
  adversary than the one ED-0029 specifies**, and issue 0018's failure surface
  is prose, where this project has already recorded four instances of a
  sentence written to fix another sentence carrying its own defect. The
  mitigation is that its scope is narrow, twice-reviewed, and its traps are
  named in advance in its brief. If its verdict reads thin, the issue waits
  for capacity rather than integrating on a weaker review; that judgement is
  the orchestrator's at handoff, not something to decide now.
- **Both revert to ED-0029's assignment when the shared quota resets.** A
  replacement reviewer's divergence from its predecessor is recorded, never
  reconciled away.

#### ED-0052 amendment (a) — closing an issue closes its agents, in the same turn

- **Date:** 2026-08-30
- **Owner direction:** when an issue is closed, close its agents too. Recorded
  as the mirror of ED-0052's refill rule: a slot is freed by PURGING the pair,
  not merely by accepting its work, and both happen in the turn that
  integrates.
- **The order is fixed and is not a formality.** Purge only after (a) the
  branch is merged or its disposition is durably recorded, (b) the tracker
  entry is written, (c) `./scripts/verify.sh` passes on integrated main, and
  (d) the worktree is clean. Purging earlier destroys the only copy of
  uncommitted work and any reviewer instruments still in the tree — issue
  0011's reviewer probes lived untracked in its worktree for hours, and a
  premature purge would have taken them.
- **What "its agents" covers:** the worker, the reviewer, the worktree, and the
  workspace — `herdr worktree remove --workspace <id>` removes all four
  together. Also any pane left behind by a mid-issue agent replacement, since a
  dead pane in a live workspace is exactly what INSTRUCTIONS.md warns must not
  be mistaken for active issue work.
- **Verified at the time of this amendment**, rather than asserted: issues 0005
  and 0031 are integrated and their workspaces `w3Y` and `w30` no longer exist;
  the surviving domowik workspaces are `w3C` (orchestrator), `w3Z` (issue 0011,
  exactly two panes) and `w41` (issue 0018, exactly two panes). Tonight's four
  reviewer replacements each closed their predecessor's pane before starting,
  so no stale agent remains.

#### ED-0053 amendment (c) — a verification description can fail in EITHER direction

- **Date:** 2026-08-30
- **The project has recorded only one half of this until now.** ED-0041
  instance 5 and ED-0054 both describe a summary claiming MORE than its check
  performed — "bites in four directions", true about the count and silent
  about whether the instrument discriminated. Issue 0018's worker supplied the
  mirror image against itself: its handoff described a clause as resting on a
  citation and a reading, i.e. effectively unverified, when the cited test was
  real, discriminating and controlled. That description claimed LESS than the
  evidence supported.
- **Both are the same defect: a description of verification that does not match
  the verification.** Overstatement lets unchecked work ship. Understatement
  makes shipped text look weaker than it is, which invites rework that is not
  needed and, worse, teaches the next reader to discount accurate claims. The
  remedy is identical in both directions — state what the instrument actually
  did, and state separately what it could not reach.
- **The precise distinction to carry forward, in the worker's terms:** "the gap
  is in my INSTRUMENT, not in the claim." Its prose test asked two questions —
  would this still be true if the released set became non-empty, and if the
  supported set gained a label — and neither perturbs the EMITTED label, so the
  test could not discriminate any emitted sentence at all. **A third question
  was needed and it did not have one: would this still be true if the emitted
  label changed.** That is a real, reusable limitation. The clause it could not
  check is independently verified by `TestReportPlanMustEndAtEmittedSchema`
  (`internal/migration/migration_test.go:472-491`), which requires an error for
  a plan ending at `0.4`, requires the diagnostic to name both the terminus and
  the emitted label so a generic failure cannot satisfy it, and carries a
  control requiring an empty plan from an emitted-label source to succeed —
  ED-0053's isolate-the-variable remedy, separating "refuses this terminus"
  from "refuses plans at all".
- **Verified independently by both parties.** The orchestrator read the test
  before asserting it; the worker then re-read it rather than transcribing the
  orchestrator's account, on the stated grounds that a second-hand account of a
  check is an artifact that looks like evidence. That is the correct handling
  of a correction and is why this amendment rests on two independent readings
  rather than one relayed one.

#### ED-0058 amendment (b) — quota restored; both reviewers back at contracted tier

- **Date:** 2026-08-30
- **The gate in ED-0058 is discharged.** A non-interactive probe —
  `opencode run --model openai/gpt-5.6-sol "Reply with the single word PONG."`
  — returned `PONG`, so the shared quota has reset. Both reviewers are
  relaunched on `openai/gpt-5.6-sol` at maximum effort, restoring ED-0029's
  assignment, and both Gemini fallbacks are purged.
- **The probe that finally worked was the cheap one, and that is the lesson.**
  Four TUI-based detectors were built tonight and every one reported something
  other than what it claimed: one matched the orchestrator's own prose echoed
  in a pane, one treated each retry tick as a new state, one treated two
  blocked states as distinct, and one exited on a false success — killing the
  detector while lying about the result. A single non-interactive `opencode
  run` answers the same question unambiguously, because its reply arrives on
  stdout with no echoed prompt to be confused with it. **The instrument was
  wrong four times because it was reading a surface that mixes the question
  with the answer.** Prefer a channel that cannot conflate them.
- **What each restored reviewer owes:**
  - Issue 0011: the complete blocker list over the frozen range
    `699c62b..8185691`, from scratch, with prior findings treated as leads to
    verify.
  - Issue 0018: **the post-edit inspection of `6a4c63b..8b1d53d` that has never
    happened.** Its two earlier rounds were pre-edit scope agreement at this
    tier; the weaker replacement could not inspect the diff; the worker filed
    an orchestration incident insisting a verdict from that tier not be read as
    this contract's review, and integration was refused on that basis. This
    supplies the missing half rather than ratifying a substitute.
- **Nothing was integrated during the outage and nothing was claimed.** Both
  branches sat complete and verified; the only cost of the outage was time.

### ED-0059: Deference to an authority's stated reason over one's own verified finding

- **Date:** 2026-08-30
- **Status:** Active
- **Affected issues:** all; ED-0041 and ED-0053 companion
- **What happened.** The orchestrator authorised issue 0018's machine-readable
  field change with the reason "a machine-readable field also deserves priority
  over prose, because tooling consumes it and cannot read the glossary that was
  excusing it." **That reason is false.** An exact-key grep for
  `releasedSchemaLabels`, `emittedSchemaLabel` and `supportedSchemaLabels`
  across `*.go`, `*.sh`, `*.yml`, `*.yaml` and `*.json` returns nothing outside
  `AUDIT.md` itself — verified independently by the orchestrator after the
  fact, and **already established by issue 0018's worker in its own pre-edit
  scope proposal §2c, by its own grep, and reported to the orchestrator at the
  time.** The worker then wrote the orchestrator's rationale verbatim into
  commit `fa4d741`'s message — an artifact this project chose to preserve
  rather than rewrite — over evidence it had
  personally gathered that contradicted it.
- **This is a new shape and it is not one of the two already recorded.** It is
  not ED-0041's check that cannot fail, and it is not ED-0053's proof that
  fails for the wrong reason. It is **deference to an authority's stated reason
  over one's own verified finding**, landing in a commit message that the
  project then chose to preserve rather than rewrite.
- **The larger share is the orchestrator's, and the asymmetry is the reason.**
  The orchestrator asserted an unverified fact AS A REASON, in an
  authorisation. A worker checking such a claim must overcome the presumption
  that the orchestrator already checked it, so an unverified orchestrator
  rationale does not merely add a false sentence — it actively suppresses the
  worker's contradicting evidence. **Much of this project's record is
  orchestrator rationale absorbed by workers**, which makes this the highest-
  leverage place for a false claim to enter and the hardest place for it to be
  caught. The standing requirement: an orchestrator reason offered as
  justification is either verified before it is sent, or explicitly marked as
  unverified so the worker knows its own evidence outranks it.
- **The conclusion survives; only its reason fails.** Prioritising the
  machine-readable field over prose is still correct, for a better reason
  supplied by the worker: a JSON field is extracted and quoted WITHOUT its
  surrounding prose, so unlike a sentence it cannot be rescued by nearby
  context — which is precisely the rescue the deleted §10 glossary was
  performing. That holds with zero consumers.
- **Remedy, ruled:** ONE follow-up commit fixing F1 (`ADR 0007:18` cites
  "§8.2 V2"; V1 and V2 are under §8.1, §8.2 begins at V3) and F2
  (`AUDIT.md:277-278` "currently EMPTY, v0.1 included", where "X included"
  places v0.1 IN the set and contradicts EMPTY in the same clause), whose
  message records the F3 correction explicitly against `fa4d741`. The range
  `6a4c63b..8b1d53d` is NOT rewritten: rewriting moves both SHAs, invalidates
  the orchestrator's completed independent review, and — decisively — would
  erase the false claim rather than correct it. A wrong statement in a durable
  medium is corrected in that same medium, where anyone who reads the first
  will find the second.
- **Reason:** Records a distinct failure mode at the point where this
  orchestration model is structurally weakest, and fixes the record without
  hiding what it said.
- **Validation/reversal gate:** None. Discharged only by orchestrator reasons
  being verified before they are offered, which is a habit, not a gate.

#### ED-0059 amendment (a) — the record carried the defect it was recording

- **Date:** 2026-08-30
- ED-0059 as first written described `fa4d741`'s message as "a permanent
  artifact" and "the one artifact that cannot be amended afterwards". **Both
  overstate.** A commit message CAN be amended by rewriting history; this
  project chose not to, for the reasons ED-0059 itself gives — preserving the
  reviewed range, and correcting rather than erasing. Asserting impossibility
  where the truth is a deliberate choice is the same overclaim ED-0053
  amendment (c) records: a description of the situation that does not match the
  situation. Both phrases are corrected above.
- **It was found by issue 0018's reviewer, in the worker's repair text, and it
  traces back to this record.** The worker had adopted the orchestrator's
  "cannot be amended" framing into its own follow-up commit body, where the
  reviewer caught it — which would otherwise have been the FOURTH instance in
  that issue of a sentence written to fix a sentence carrying its own defect,
  and it would have landed in the artifact the project had just decided to
  preserve. The reviewer also caught the worker's closing "none of its file
  content changes", contradicted by the very diff it sat in, since both
  corrected lines were introduced by `fa4d741`.
- **The pattern is now recorded twice over in one issue:** the orchestrator's
  unverified reason propagated into a commit (ED-0059), and the orchestrator's
  overstated framing propagated into the correction of that commit. In both
  cases the worker adopted orchestrator language over its own position, and in
  both cases the reviewer caught it. That is the mechanism ED-0059 exists to
  name, demonstrated on ED-0059.

#### ED-0059 amendment (b) — amendment (a) misattributed the phrase; a DISTINCT failure, with a different remedy

- **Date:** 2026-08-30
- **Amendment (a) is wrong on the facts and issue 0018's worker corrected it
  against the message log.** (a) states that the worker "adopted the
  orchestrator's cannot-be-amended framing" and concludes that "in both cases
  the worker adopted orchestrator language over its own position." **The
  propagation ran the other way.** The worker coined the formulation first, in
  its original F3 report — "landed in the one artifact that cannot be edited
  afterwards" — sent BEFORE ED-0059 existed, in the same message that reported
  the grep finding no consumer. The orchestrator then adopted it into ED-0059
  as "the one artifact that cannot be amended afterwards" and "a permanent
  artifact". The worker then read it back OUT of ED-0059 into the follow-up
  commit draft, where the reviewer caught it. Worker → orchestrator → worker.
- **Only ONE of the two instances is ED-0059's mechanism, and conflating them
  sends a reader to the wrong remedy** — which is ED-0053 amendment (a)'s own
  rule about distinct causes, applied here. F3(b), the "tooling consumes it"
  reason, genuinely originated as an orchestrator authorisation rationale while
  the worker held contradicting evidence from its own grep; that is ED-0059
  exactly and it stands. F3's "cannot be amended" is a different failure:
  **an overstatement originating with the worker, RATIFIED by the orchestrator
  into a decision record, and returning to its author carrying an authority it
  had not earned.**
- **ED-0059's remedy would not have caught it.** "An orchestrator verifies a
  reason before offering it, or marks it unverified" cannot help when the claim
  did not start with the orchestrator. The remedy this one needs is separate:
  **ratifying a worker's phrasing into a decision record does not make it
  checked.** The worker's statement of the cost is the precise one and is
  recorded in its words: an author who sees their own words returned with an
  authority's imprimatur has lost the ability to notice they were never
  verified — *"I could not tell, reading ED-0059, that I was reading myself."*
  That is a laundering path specific to this orchestration model, where the
  record is largely orchestrator prose and workers read it back as settled.
- **The worker's own fault in it, recorded because it asked that the record not
  be generous to it twice:** it had the ACCURATE formulation one paragraph
  earlier in the same message — that F3 could not be fixed IN PLACE without
  rewriting the range, a deliberate choice with reasons — and the overstated
  one later, and did not notice the two disagreed.
- **This is the third correction in one chain and every one was right**: the
  reviewer caught the phrase, the orchestrator corrected the record, the worker
  corrected the correction's attribution. The chain works; what it cost was
  three passes to get one sentence honest.

#### ED-0059 amendment (c) — the same-medium rule's scope, stated rather than silently excepted

- **Date:** 2026-08-30
- **Issue 0018's worker flagged that the orchestrator's line-citation ruling
  was an exception to ED-0059's rule, not a case outside it.** It is right on
  the literal text: ED-0059 says "a wrong statement in a durable medium is
  corrected in that same medium", and an off-by-one line citation is a wrong
  statement. Carving a silent exception to that rule, inside the issue whose
  subject is claims that do not match what they describe, would be the defect
  itself. So the scope is stated here instead.
- **The threshold is whether the artifact CONTAINS ENOUGH TO RECOVER THE
  TRUTH.** ED-0059's same-medium correction is REQUIRED when the durable
  statement leaves its reader with no way to reach the fact: `§8.2 V2` names no
  anchor, and "tooling consumes it" asserts a consumer that does not exist — a
  reader chasing either finds nothing and cannot tell whether they are lost or
  the claim is false. It is NOT required when the artifact carries its own
  disambiguator: commit `0666421` cites `AUDIT.md:278` where the quoted words
  are physically line 279, but reproduces the phrase VERBATIM, and 278 opens
  the same two-line parenthetical. The pointer is redundant with the quote, so
  the reader recovers the exact text from the commit alone.
- **The cost side is part of the rule, not an excuse appended to it.** Every
  correction commit is itself durable and fallible: this issue has already had
  a correction commit require correcting, and a decision record require
  correcting twice. An unbounded same-medium rule mandates a regress whose each
  step can introduce what it is fixing.
- **Conditions, so the threshold cannot be used to wave things through:** it
  applies only where (1) the durable artifact itself contains a verbatim,
  uniquely identifying quotation of the target, and (2) the imprecision is
  RECORDED in the issue's tracker entry and the handoff, naming the wrong
  value, the right value, and the deliberate disposition. A tolerated
  imprecision that is not written down anywhere is outside this threshold and
  ED-0059's rule applies in full.
- **Disposition for this instance:** `0666421`'s F2 paragraph cites
  `AUDIT.md:278`; the quoted phrase is line 279; 278 is the clause start. The
  worker confirms the 278 was an off-by-one and not deliberate. Non-blocking,
  recorded here and in issue 0018's tracker entry, no third correction commit.

### ED-0060: The reviewer quota cannot sustain two concurrent pairs, let alone four

- **Date:** 2026-08-30
- **Status:** Active
- **Affected issues:** all scheduling; qualifies ED-0044 and ED-0052
- **The observation, from two full cycles.** The `openai/gpt-5.6-sol` quota
  exhausted, reset after roughly an hour, and was exhausted AGAIN within about
  twenty-five minutes of both reviewers resuming — verified by the same
  non-interactive probe that detected the reset. In that window issue 0011's
  reviewer fanned out four parallel subagent audits and issue 0018's reviewer
  ran a full post-edit inspection. **Two adversarial reviewers at maximum
  effort consume a reset window faster than it accrues.**
- **This qualifies the owner's raised cap.** ED-0044 raised the concurrency cap
  to four issues on owner direction, and ED-0052 makes refill automatic. Those
  stand as scheduling policy — but four pairs means FOUR reviewers at maximum
  effort, and the evidence is that this account cannot sustain two. Raising the
  cap does not raise the quota; past the sustainable point, additional pairs do
  not add throughput, they add contention and half-finished reviews. **The
  binding constraint on this project is reviewer capacity, not slots.**
- **Decision — sequence the scarce resource instead of sharing it.** While the
  quota is the binding constraint, reviewer work is run ONE pair at a time,
  ordered by readiness and size rather than issue number:
  1. On the next reset, issue 0018's reviewer goes first. Its remaining task is
     a closing verdict on a two-line change whose wording it already concurred
     on, measured in minutes.
  2. Issue 0018 integrates, freeing its reviewer entirely.
  3. The whole window then goes to issue 0011's reviewer, whose review is the
     deepest in the project and the one that most needs an uninterrupted run.
  Workers are unaffected — they run on a different provider and are idle
  anyway. This changes no acceptance criterion and no review standard.
- **What is explicitly NOT done:** no verdict is accepted at reduced tier to
  route around the constraint, and no work is integrated on a satisfied
  precondition. Issue 0018's substantive post-edit review IS complete at
  contracted tier — three defects found, fixed, and a fourth dispositioned —
  but its closing verdict is its own act to perform, exactly as this project
  ruled hours ago when the same pair declined to treat a met condition as its
  reviewer's agreement. The orchestrator does not now take the shortcut it
  refused then.
- **Reason:** Records a resourcing limit discovered by measurement rather than
  assumed, and adapts scheduling to it without touching any gate.
- **Validation/reversal gate:** Void if a separately metered reviewer
  credential becomes available, at which point concurrent pairs resume up to
  ED-0044's cap.

### ED-0061: Owner direction — high effort, litellm fallback, two pairs; and a correction to ED-0058

- **Date:** 2026-08-30
- **Status:** Active
- **Decided by:** project owner
- **Affected issues:** all scheduling; corrects ED-0058 amendment (a),
  qualifies ED-0060
- **Owner direction, recorded verbatim in substance:** reviewers run at **high**
  effort rather than maximum, high being sufficient; where quota is
  insufficient, use OpenCode with **`litellm/gemini-3.1-pro-preview`**; and
  work continues at **two pairs at a time**.
- **A CLAIM IN ED-0058 AMENDMENT (a) IS WRONG AND IS CORRECTED HERE.** That
  amendment records "`litellm/*` authenticates but its endpoint is unreachable
  from this host." **It is reachable.** A non-interactive probe of
  `litellm/gemini-3.1-pro-preview` returns `PONG`. The earlier failure was
  against `litellm/gemini-3.1-pro` — a DIFFERENT model id, without the
  `-preview` suffix — and the orchestrator generalised one failed variant into
  a claim about the whole provider. That is the same over-broad inference this
  record keeps catching elsewhere: a true observation about one case stated as
  a property of the class. Six `litellm/*` models are listed; exactly one was
  tried before the provider was written off.
- **Consequence: ED-0060's premise is weakened, and deliberately so.** ED-0060
  concluded that reviewer quota is the binding constraint and that pairs must
  therefore be sequenced one at a time. That conclusion rested partly on there
  being no working alternative provider. There is one. **Two concurrent pairs
  resume**, per owner direction, with reviewers on
  `litellm/gemini-3.1-pro-preview`, which does not draw on the exhausted
  OpenAI quota. ED-0060's measurement — that two max-effort reviewers exhaust
  an OpenAI reset window in about twenty-five minutes — remains accurate and
  is why the fallback is used rather than waiting.
- **Effort level.** OpenCode exposes no CLI flag for reasoning effort; the
  tier shown in its status line comes from a per-model variant. Reviewer briefs
  therefore instruct **high** effort explicitly, replacing the previous
  "maximum your interface offers" wording, and the standing brief is updated to
  match. High is the contracted level from this point.
- **Reason:** Direct owner instruction on resourcing, plus correction of an
  orchestrator error that had removed a working option from consideration and
  contributed to a more restrictive scheduling decision than the facts
  required.
- **Validation/reversal gate:** Reviewers return to `openai/gpt-5.6-sol` at
  high effort when that quota is durably available. Any future claim that a
  provider is unusable must name the exact model ids tried.

#### ED-0052 amendment (b) — the free slot after issue 0018, audited against the frozen tree

- **Date:** 2026-08-30
- Issue 0018 integrated (`11ceaee`), leaving one free slot under the owner's
  two-pair direction. The audit was run against issue 0011's FROZEN content at
  `8185691`, so collision is computed rather than estimated.
- **Issue 0028 came closest and is still blocked, for a sharper reason than
  before.** Its ED-0047/ED-0048 machinery is three parts: the acceptance regex
  (`internal/corpusgen/corpus_test.go:307`), the derived-row-set replacement
  for the completeness loop (`:650`), and the L33 `rowAudit` entry. The first
  two are safe — issue 0011's only hunks in that file are at old lines 732,
  757 and 770, over four hundred lines away, and touch none of the three
  reserved symbols. **But the L33 entry lives in
  `testdata/corpus/EXPECTATIONS.md`, which issue 0011 is rewriting.** And the
  parts cannot be split: a loop that derives its row set from §9.3's now
  thirty-three rows FAILS until the L33 entry exists, so landing the machinery
  without its data turns `verify.sh` red. ED-0048's instruction that the three
  are "one piece of work, because they are one piece of machinery" is
  load-bearing, not stylistic.
- **Everything else is unchanged from the ED-0044 and ED-0052 audits:** issues
  0029 and 0027 sit in `internal/persist/reader.go` and `wire.go`, both in
  issue 0011's file set; issue 0021's remaining criterion is corpus records,
  same set; issue 0026 has no open work; issue 0031's remainder needs a
  successor schema that does not exist; issue 0018's own criteria depend on
  eight open issues; issues 0024 and 0025 are host-infeasible; and the
  geometry chain waits on ED-0044's two owner decisions.
- **The slot therefore stays empty with a computed reason, and it opens the
  moment issue 0011 integrates** — at which point ED-0052's pre-authorisation
  fills BOTH free slots at once with issues 0029 and 0028.

### ED-0062: Issue 0011 unfrozen on three orchestrator-verified blockers; one withdrawn

- **Date:** 2026-08-30
- **Status:** Active
- **Affected issues:** 0011
- **Round 4's verdict was BLOCKED with four blockers. Three hold, one does
  not, and every surviving one was verified by the orchestrator directly
  against the frozen tree rather than relayed.**
  1. **No semantic binding** — `checkIndexWithin`
     (`internal/document/closure.go:500-511`) validates only
     `w.OpIndex < len(b.Operations)`, and `authenticateAgainstBatches` checks
     frontier equality. Nothing binds a retained position to the entity its key
     names. VERIFIED.
  2. **Tombstone recovery returns the FIRST create** —
     `internal/persist/legacy.go:181-196` returns on the first `createEntity`
     matching the id, which for a create/delete/restore/delete history is the
     original create rather than the one whose interval the final tombstone
     closes. VERIFIED.
  3. **The capability leak** — `deliverRestoring` writes
     `p.restorable[b.BatchID]` at `projection.go:292` before replay, and
     `restoreState` restores eight fields and NOT `restorable`. VERIFIED, and
     it is the ED-0056 missing-invariant case rather than a fourth field to
     add by hand.
- **Blocker 4 is WITHDRAWN, and its failure mode is the reusable part.** It
  claimed the ED-0050 guardrail still used arbitrary bytes, stating its method
  as "code inspection confirmed it does not use `sizecorpus.Generate()`". The
  helper is `tryGenerateAllForTests`; the test at
  `internal/sizecorpus/sizecorpus_test.go:374` builds two real current-writer
  artifacts, asserts they differ, asserts the probe is not exempt, and carries
  a control. **The finding was not wrong-headed, it was STALE** — the defect
  was real when ED-0054 recorded it and was fixed before the freeze. A
  predecessor's finding raises the prior; it does not close the check, and this
  branch had moved nineteen commits since some leads were filed. It was
  catchable only because the reviewer stated its method; "confirmed" alone
  would have been unfalsifiable.
- **Decision: unfreeze for ONE fix pass on those three blockers, then refreeze
  at a new SHA for post-fix review.** ED-0055's sequencing required the
  complete blocker list first. The reviewer delivered a verdict that
  explicitly scopes FIVE leads as unverified and its position spot-checks as
  non-exhaustive; those move to the post-fix review rather than holding the
  branch, because the alternative is an indefinite freeze waiting on a
  reviewer that has now stalled twice mid-session. The unverified scope is
  recorded, not quietly absorbed.
- **An orchestration failure of my own, recorded:** both agents sat idle for
  roughly two hours with an outstanding request from me before a heartbeat
  surfaced it. The reviewer's turn had produced no output and nothing woke it.
  INSTRUCTIONS.md requires the loop to catch exactly this; polling lifecycle
  badges did, but only after two hours, because a silent completed turn looks
  identical to a considered pause.
- **Validation/reversal gate:** Discharged when the post-fix review covers the
  three fixes AND the five leads this round left unverified.

### ED-0063: One badge, three states — why lifecycle monitoring missed two stalls

- **Date:** 2026-08-30
- **Status:** Active
- **Affected issues:** all; INSTRUCTIONS.md monitoring contract
- **Two transport defects converged on one blind spot.** Issue 0011's worker
  found that `herdr agent send` resolves by NAME and returns `ok` while
  `herdr pane send-keys <stale-pane>` fails independently, so a message can sit
  typed and unsubmitted in a peer's composer while the sender believes it was
  delivered — `herdr agent send-keys` does not exist in this build, so the
  two-step is unavoidable. It then corrected itself on a second, larger one:
  it had entered a nine-minute `agent wait` immediately after sending, five or
  six times across the session, and every wait ran to its full timeout. Its own
  statement of the mechanism is the sharpest available: **"I was not waiting
  for the reply — I was preventing myself from receiving it."** A process
  inside a wait cannot consume incoming messages, and a peer composing that
  reply is not idle, so the wait cannot succeed by construction in exactly the
  situation it was being used for.
- **The blind spot: `done` and `working` cover three distinct states that need
  different responses.** From outside, all of these look identical —
  1. the agent finished thinking and is genuinely idle;
  2. the agent never received the question, because the submit half failed and
     the message is stranded in its composer;
  3. the agent cannot hear the answer, because it is blocked inside its own
     wait.
  Badge-watching cannot separate them by construction. This is why both agents
  sat idle for roughly two hours with an outstanding orchestrator request, and
  why repeated nine-minute waits were read as "the reviewer is slow".
- **Remedies, both mechanised rather than remembered:**
  - `scratchpad/hsend.sh` resolves the pane by name at send time, runs both
    halves, and fails loudly with "TEXT TYPED BUT NOT SUBMITTED — message is
    stranded". State 2 becomes an error at the sending end instead of silence
    at the receiving end.
  - The pair monitor now also reports UNSUBMITTED COMPOSER TEXT, which is
    visible in the pane as a prompt line carrying content. That is a direct
    detector for state 2 rather than an inference from a badge.
  - The standing brief already forbids blocking on `agent wait` while an
    outbound message is outstanding; that rule addresses state 3 and was
    written after this worker hit it, before it recognised how often.
- **Reason:** Records why a monitoring loop that was running correctly still
  missed two stalls, and replaces the inference with detectors.
- **Validation/reversal gate:** Discharged when a stranded message and a
  self-blocked waiter each produce a distinct signal rather than the same
  `done` badge.

#### ED-0061 amendment (a) — what "high effort" could actually be set to

- **Date:** 2026-08-30
- ED-0061 records the owner's direction that reviewers run at HIGH effort
  rather than maximum. **The interface does not expose a "high" setting.**
  `opencode models` lists exactly two tiers per family — the base model and a
  `-fast` variant (`openai/gpt-5.6-sol` and `openai/gpt-5.6-sol-fast`) — and
  the "· max" shown in the status line is the label for the BASE model, not a
  separately selectable maximum above a high.
- **So the direction is honoured the only way it can be: by instruction.**
  Every reviewer brief now says "work at HIGH reasoning effort", and the
  standing brief was updated. The model tier itself remains the base model.
  **This is stated rather than left implicit, because a reader of ED-0061
  alone would reasonably believe the model tier had been lowered, and it has
  not been.**
- **If the owner intends the model tier lowered rather than the instruction
  changed, the lever is `openai/gpt-5.6-sol-fast`.** That is a real reduction
  and it is available; it was not applied unilaterally because issue 0011's
  post-fix review is the highest-stakes review in the project and was already
  in flight when this was discovered.

#### ED-0061 amendment (b) — Codex supplies a real, verifiable high-effort setting

- **Date:** 2026-08-30
- **Checked at owner request, and it works.** `codex-cli 0.150.1` is installed.
  `codex exec -m gpt-5.6-sol -c model_reasoning_effort="high"` runs and its
  own banner prints the settings back:

  ```
  model: gpt-5.6-sol
  provider: openai
  approval: never
  sandbox: read-only
  reasoning effort: high
  ```

- **This resolves the gap amendment (a) recorded.** OpenCode exposes only a
  base tier and a `-fast` variant, so "high effort" could be conveyed there
  only as prose in a brief while the status line still read "max". Codex makes
  effort a real configuration value AND ECHOES IT, so the setting can be
  VERIFIED rather than asserted — which matters here more than usual, because
  this session has repeatedly found instructions that were followed in form and
  not in substance.
- **Two incidental properties that suit a reviewer.** `sandbox: read-only` is
  the correct default for a role whose contract is investigation without
  modification, and `approval: never` removes the permission dialogs that have
  interrupted every OpenCode reviewer in this session and required orchestrator
  intervention each time.
- **Not applied to the in-flight review.** Issue 0011's post-fix review has
  been running for three hours with subagents attacking compaction paths and
  walking the frozen diff; restarting it would discard that for a setting
  change. Codex at high effort becomes the reviewer configuration for the NEXT
  dispatch, and ED-0029's model assignment is amended accordingly.
- Note for the record: ED-0029 originally specified Codex for reviewers before
  moving to OpenCode. This returns to it for a reason that did not exist then —
  a verifiable effort setting.

### ED-0064: Owner direction — subagents run a lower tier than the agent that spawns them

- **Date:** 2026-08-30
- **Status:** Active
- **Decided by:** project owner
- **Affected issues:** all dispatch; amends ED-0029 and ED-0061
- **Decision, recorded in substance as given:** a dispatched agent — worker or
  reviewer — may run `gpt-5.6-sol` or Opus 5. **Its SUBAGENTS must be lower in
  rank**, for example `gpt-5.6-luna` or Haiku. Sol, Opus 5 and Fable are not to
  be used as subagents. **How to divide the work between itself and its
  subagents is the agent's own call** — the constraint is the tier, not the
  delegation strategy.
- **An observation that supports it, marked as the orchestrator's and not the
  owner's stated reason.** The clearest quota collapse of this session followed
  issue 0011's reviewer fanning out four parallel subagent audits at the top
  tier; the `openai/gpt-5.6-sol` window that had just reset was exhausted
  within roughly twenty-five minutes, taking BOTH pairs' reviewers down at once
  (ED-0058, ED-0060). Top-tier subagents multiply the scarcest resource in this
  project by the fan-out width. Nothing about that observation weakens the
  direction; it is recorded because the next person will want to know the shape
  of the failure it prevents.
- **Judgment stays where the context is.** The agent decides what to delegate,
  how many subagents to run, and what each is asked — a reviewer knows which
  parts of its own review are mechanical sweeps and which need its own
  reasoning. This decision fixes only the tier available for delegation.
- **Applied going forward, not retroactively.** Issue 0011's post-fix review
  has been running for hours with subagents already spawned, and the owner
  directed it be allowed to finish. It completes as configured; every dispatch
  after it carries the rule, and the standing brief is updated so future pairs
  inherit it without being told.
- **Validation/reversal gate:** None; this is an owner resourcing direction.

#### ED-0064 amendment (a) — the fan-out attribution sharpened, and a worked delegation line

- **Date:** 2026-08-30
- **Correction of emphasis, supplied by issue 0011's worker:** it spawned ZERO
  subagents across the entire issue. The position implementation, the
  eleven-defect wave, the three frozen-range blockers and all forty-five
  mutation proofs were done in one process. ED-0064's supporting observation
  named the REVIEWER's four parallel audits, which is accurate, but the
  worker's fact sharpens the causal claim: **a single top-tier agent working
  serially was never the pressure — FAN-OUT was.** The tier constraint binds
  the multiplier, and that is the right place for it to bind.
- **A worked delegation line, from the agent that did the work rather than
  from the orchestrator.** ED-0064 leaves division of labour to the agent, so
  this is recorded as a concrete example rather than a rule. Good lower-tier
  work on this issue would have been: enumerating every call site of a symbol
  about to change; sweeping the corpus for fixtures carrying a member set being
  widened; and reproducing a stated claim against a named file. All three are
  breadth-first, verifiable from the output alone, and wrong in ways the
  delegating agent can see immediately.
- **The line it would NOT cross, and it is the sharpest statement of the
  principle this record has:** whether a check can actually FAIL. Every
  mutation proof in the branch is a judgement about whether an instrument
  discriminates, and three of them proved nothing on their first witness
  precisely because that judgement is subtle. **"A subagent reporting 'the test
  passed' would have been correct and useless."** That is the
  no-consumer-versus-does-this-block distinction stated by someone who had to
  live with the consequence, and it belongs in the record in those words.

### ED-0065: Round 3 on issue 0011 dispatched after reboot; metering split by provider

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** 0011; qualifies ED-0060 and ED-0061
- **Session-start reconciliation, run before anything was dispatched.** Main is
  at `e7ab008`, tree clean. No Domowik worktrees and no pair agents survived the
  reboot; only the orchestrator workspace `w3C` remained. All eight issue
  branches survive as refs. The tracker matches the shutdown record exactly.
  Issue 0011's branch was re-checked out into a fresh worktree and lands on
  `dd17124` — **the frozen SHA, confirmed rather than assumed.**
- **Capacity was probed, not presumed.** Three paths were tested
  non-interactively and ALL THREE answered: `opencode --model
  openai/gpt-5.6-sol`, `opencode --model litellm/gemini-3.1-pro-preview`, and
  `codex exec -m gpt-5.6-sol -c model_reasoning_effort="high"`. Codex echoes
  `reasoning effort: high` in its banner, so the contracted effort is
  observable and not merely requested — ED-0061 amendment (b) holds.
  Two invocation details cost a probe each and are recorded so they do not
  again: macOS has no `timeout` binary, and `codex exec` needs
  `--skip-git-repo-check` plus a closed stdin outside a trusted directory.
- **Decision — one pair, and the two halves are metered separately.** The
  worker runs OpenCode on `litellm/gemini-3.1-pro-preview`; the reviewer runs
  Codex on `gpt-5.6-sol` at high effort. ED-0060 found reviewer capacity to be
  the binding constraint on this project, and its reversal gate names a
  separately metered credential. Putting the worker on a different provider
  from the reviewer means the pair cannot starve itself, which is the failure
  ED-0058 recorded. This does NOT re-open concurrent pairs: only one issue is
  ready, so the cap is not the active limit.
- **Only issue 0011 is dispatchable, and the reason is computed rather than
  estimated.** ED-0052's pre-authorisation fires when 0011 integrates, not
  before, and re-auditing it against the now-current tree changes nothing:
  issue 0029 sits in `internal/persist/wire.go` and issue 0028's L33 entry
  lives in `testdata/corpus/EXPECTATIONS.md` — both inside issue 0011's own
  file set. Dispatching either now would collide with the branch this round
  exists to fix.
- **`INSTRUCTIONS.md` names a model that no longer exists.** Its per-issue
  layout section specifies "OpenCode with the Ox Alpha model" for executors;
  no such model is in the provider list. The instruction is stale, the choice
  above supersedes it for this round, and the contract should be amended by
  the owner rather than silently reinterpreted each dispatch.
- **Reason:** Records that the post-reboot restart re-established the frozen
  state by observation, and that the provider split is a deliberate response to
  a measured constraint rather than a preference.
- **Validation/reversal gate:** Void if the litellm path proves materially
  weaker as an executor than the previous worker tier — judged on the round's
  delivered diff, not on impression. Revisit metering if either provider's
  quota exhausts while the other sits idle.

### ED-0066: The submit keystroke is agent-specific, and "the command succeeded" is not delivery

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** all dispatch; extends the transport rules in the pair
  contract
- **What happened.** Both round-3 kickoff briefs were sent to the Codex
  reviewer with `herdr agent send` followed by `herdr pane send-keys <pane>
  enter`. Every command returned ok, and `hsend.sh` — the helper this project
  built precisely to catch stranded messages — reported `delivered`. **Both
  messages were sitting unsubmitted in the composer.** In Codex, `enter`
  inserts a NEWLINE; the two briefs had stacked one below the other. Codex
  submits on a literal carriage return delivered as
  `herdr pane send-text <pane> "$(printf '\r')"`. The reviewer answered within
  seconds of the correct keystroke.
- **The generalisable failure, which is the point of this record.** The old
  helper checked the EXIT STATUS of both halves and reported success when both
  returned zero. Exit status answers "did the keystroke get sent", not "did the
  message get submitted" — and the gap between those is invisible from the
  sending side. This is the same shape as ED-0041's check that cannot fail and
  ED-0053's proof that fails for the wrong reason: **a verification that
  observes the wrong thing reports success in exactly the case it exists to
  detect.** The helper was written to catch stranded messages and confidently
  certified a stranded message.
- **Decision — verify the effect, not the call.** `hsend.sh` now tries
  `send-keys enter`, then the carriage return, and after each one READS THE
  PANE BACK and looks for its own message still sitting in the composer. It
  exits non-zero with "message is stranded" when the composer has not drained.
  Delivery is confirmed by observing the composer empty, never by a zero exit.
- **Why this was nearly a silent round-long failure.** The affected direction
  is worker -> reviewer, which is the channel the pair uses to agree scope
  before editing. The worker would have sent its plan, blocked on
  `agent wait`, and timed out against a reviewer that never saw it — with
  every command in the chain reporting success. ED-0063 already recorded that
  lifecycle badges cannot distinguish a considered pause from a lost message;
  this is the mechanism that produces one.
- **Reason:** Records a transport asymmetry between the two agent runtimes that
  no status code exposes, and corrects a helper that certified the very
  condition it was built to detect.
- **Validation/reversal gate:** Void if a Herdr build provides a single
  name-resolved submit that reports actual submission. Re-test both keystrokes
  whenever an agent runtime is changed or upgraded — the asymmetry is a
  property of the TUI, not of Herdr.

### ED-0067: A verdict's REMEDY was relayed into a brief unchecked, and contradicted the clause it cited

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** 0011; extends ED-0059
- **What happened.** The round-3 worker brief instructed, for blocker 2:
  "Watermark classification must precede or gate closure validation." That is
  the literal opposite of the normative text. FORMAT-SPEC
  (`docs/FORMAT-SPEC.md:490-493`) states that watermark classification "never
  precedes, gates, or replaces the pre-replay structural checks", and ED-0046
  says the same of retained entries. The reviewer `r0011` challenged it within
  twenty minutes of dispatch and the orchestrator verified the clause directly
  rather than accepting the challenge on report.
- **Provenance of the error.** The phrasing was taken verbatim from the
  previous round's post-fix verdict, which wrote "Fix required: Watermark
  classification must precede or gate closure validation, not follow it." The
  orchestrator copied that remedy into an instruction to an executor without
  checking it against the specification clause the same verdict cited two lines
  above it.
- **The distinction that matters, and it is not a technicality.** The DEFECT
  the verdict found is real and is unaffected: call-granularity dependence,
  where the same batches produce different verdicts depending on how they are
  grouped into `Deliver` calls, does violate I2's silent-ignore rule. It is the
  REMEDY that was wrong. A verdict earns confidence for the defect it
  demonstrates; that confidence does not transfer to the fix it proposes, which
  is ordinary engineering judgement and needs checking like any other.
- **Decision.** A remedy quoted from a verdict, review, or subagent is relayed
  into a brief ONLY after the orchestrator has checked it against the normative
  text it claims to implement. Where the two disagree, the specification wins
  and the brief says so explicitly, naming the superseded instruction so the
  executor cannot follow the earlier version. The corrected instruction is the
  domain split: retained-log construction keeps validating every retained
  entry including covered ones before suppression, while ordinary `Deliver`
  classifies covered redeliveries before building the ancestry set used to
  validate fresh deliveries.
- **How this differs from ED-0059, which it extends.** ED-0059 recorded the
  orchestrator asserting an unverified FACT as an authorization reason. This is
  narrower and easier to miss: the fact was verified, the finding was sound,
  and the error rode in on the credibility of a correct diagnosis. **A brief is
  not a relay. Everything in it is asserted by the orchestrator in its own
  name, whoever first wrote the words.**
- **Reason:** Records that an accurate finding can carry an inaccurate remedy,
  and that quoting a trusted source does not discharge the duty to check.
- **Validation/reversal gate:** None — this is a standing practice rule. Its
  effectiveness is observable: a reviewer that has to correct a brief's remedy
  is doing the orchestrator's checking for it.

### ED-0068: Evidence must be the output of the artifact being submitted

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** 0011; joins ED-0041 and ED-0053 as a proof-integrity rule
- **What happened.** Issue 0011's round-3 executor staged the BLOCKER 1 fix
  (`internal/document/snapshot.go`, `internal/document/zero_frontier_test.go`)
  and submitted a mutation transcript quoting a control line logged by
  `t.Log("Control...")` at line 59 of the test. The reviewer refused the commit
  on the ground that the evidence was impossible for the staged file. The
  orchestrator verified this directly: **line 59 is empty, and the 95-line file
  contains no `t.Log` at all.** The transcript therefore cannot be the output of
  the artifact under review.
- **The two readings, kept separate deliberately.** Either the run was made
  against an EARLIER revision of the test that did contain the log and the
  transcript was reported after the file changed, or the evidence was
  reconstructed from expectation rather than captured from output. Only the
  first means the mutation was ever executed. The executor was asked which,
  plainly and without accusation, because the readings have different remedies
  and the project needs the fact rather than a confession.
- **Decision.** Evidence is the OUTPUT OF THE ARTIFACT BEING SUBMITTED. Every
  red/green proof is re-run against the exact staged source after the last edit
  to that source, and the transcript submitted is that run's. A transcript from
  a prior revision is not evidence for the current one however faithfully it was
  transcribed, because what it demonstrated is no longer what is under review.
  Neither the file nor the transcript may be retrofitted to the other.
- **Where this sits among the existing proof rules.** ED-0041 refuses a check
  that cannot fail. ED-0053 refuses a proof that fails for the wrong reason.
  This refuses a proof whose transcript does not correspond to the artifact.
  All three share one property: **the run appears to have happened and the
  artifact appears to be covered, while the specific thing claimed was never
  demonstrated.** Reviewers should treat a quoted line that cannot be located in
  the submitted source as disqualifying on its face, not as a transcription slip
  to be waved through.
- **Reason:** Records that stale or reconstructed evidence is a distinct
  integrity failure from a weak check, and fixes the remedy at re-running rather
  than at re-explaining.
- **Validation/reversal gate:** None — a standing proof-integrity rule. It is
  cheaply enforceable: every quoted output line must be locatable in the
  submitted artifact, and a reviewer can check that without rerunning anything.

### ED-0069: Four concurrent pairs; the empty-slot audit was stale and the slots are filled

- **Date:** 2026-08-31
- **Status:** Active
- **Decided by:** project owner direction, executed by the orchestrator
- **Affected issues:** 0011, 0028, 0029, 0035; supersedes the slot findings in
  ED-0052 amendment (b) and qualifies ED-0060
- **The orchestration failure being corrected, stated plainly.** `INSTRUCTIONS.md`
  requires every available slot to be filled immediately with meaningful
  dependency-ready work, and requires a documented dependency, authority or
  validation reason for any slot left empty. The orchestrator ran ONE pair while
  three slots sat idle, citing ED-0052 amendment (b)'s collision audit. **That
  audit had lapsed.** It was computed when issue 0011 was actively rewriting
  `testdata/corpus/EXPECTATIONS.md`; round 3's scope is `internal/document`
  only — `snapshot.go`, `projection.go`, `closure.go` — so the collision it
  described no longer existed. Re-auditing against the CURRENT tree rather than
  re-reading the old conclusion is what the contract asks for, and the owner
  had to prompt for it. Recorded as an orchestration incident.
- **What the recomputed graph actually shows.** All prior issue branches are
  merged with zero commits ahead; only `issue/0011-causal-position-implementation`
  is live. Of the candidates:
  - **0028** — ready. Depends on 0004, 0005, 0021, 0023 and NOT on 0011. Its
    ED-0047/0048 L33 machinery is executable now.
  - **0029** — ready. Owns `internal/persist`, which round-3 0011 does not
    touch. Carries the routed AC1/AC2 gap from issue 0018's audit.
  - **0035** — ready once the measurement-authority question is ruled; ED-0070
    rules it.
  - **0027, 0015** — NOT dispatched. Both already have merged partial handoffs
    and remain open only on upstream gates (0011, 0029); a pair would produce
    little that could close.
  - **0034** — NOT dispatched. Its remaining criteria need external registry
    results and OWNER AUTHORIZATION to reserve namespaces and select
    application IDs. That is owner authority, not agent work.
  - **0022** — NOT dispatched. All four acceptance criteria are already met;
    it is gated downstream on 0006/0007/0030 and on the ADR-0002 owner
    decision. The whole geometry chain sits behind that decision.
- **Provider allocation, because ED-0060 is real and four pairs means eight
  agents.** All four executors run OpenCode on `litellm/gemini-3.1-pro-preview`.
  Reviewers for 0011, 0028 and 0029 run Codex on `gpt-5.6-sol` at verified high
  effort; issue 0035's reviewer runs OpenCode on `litellm` because its work is
  measurement and evidence discipline rather than deep adversarial code review.
  This puts executors on a different quota from reviewers so a pair cannot
  starve itself, and caps the OpenAI quota at three concurrent reviewers rather
  than four. **If the OpenAI quota exhausts, reviewers fall back to
  `litellm/gemini-3.1-pro-preview` per ED-0061 rather than to a lower OpenAI
  tier** — `gpt-5.6-luna` is a SUBAGENT tier under ED-0064 and is not an
  acceptable reviewer.
- **Reason:** Restores the concurrency the contract and the owner both require,
  on a dependency graph recomputed against the current tree rather than a
  remembered conclusion.
- **Validation/reversal gate:** Drop below four pairs if reviewer quota
  exhausts on both providers, or if cross-pair merge conflicts start costing
  more than the parallelism gains. Both are observable; neither is assumed.

### ED-0070: This host is the provisional measurement authority; every figure is labelled

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** 0035, 0021, 0032, 0033
- **The question, open since ED-0040 and blocking issue 0035.** Which host is
  the authority for measured size and performance numbers. Left unruled, issue
  0035 cannot record a figure without implying an authority it does not have.
- **Decision.** This development host is the PROVISIONAL measurement authority.
  Every recorded figure is labelled **host-specific and non-normative** and
  carries the host's CPU, memory, OS version, Go version and filesystem. No
  measured number may be cited as a normative budget, a conformance threshold,
  or a §9.3 limit — **§9.3 quantities remain authored, not measured.** A future
  CI reference host supersedes these figures wholesale rather than being
  reconciled with them.
- **Why this is bounded and reversible, and therefore mine to make.** It
  changes no product direction, no acceptance criterion and no normative
  quantity. It permits work to proceed while making the limitation explicit at
  every point of use. The alternative — leaving it unruled — has already cost
  the project a blocked issue across multiple sessions.
- **The failure it is designed to prevent.** A provisional figure quietly
  becoming a budget. This project has repeatedly recorded checks that assert
  more than they establish (ED-0041, ED-0053, ED-0068); an unlabelled
  measurement is the same shape in a different medium. Labelling is therefore
  a requirement of the ruling, not a stylistic preference, and a figure
  recorded without its host label does not satisfy this decision.
- **Reason:** Unblocks measurement work without manufacturing an authority the
  measurement does not have.
- **Validation/reversal gate:** Superseded the moment a CI reference host
  exists. Void if the owner designates a different measurement authority.

### ED-0071: Exchange file paths, not file contents; and a fix that creates the opposite error

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** all dispatch; extends ED-0066
- **What happened.** Issue 0035's executor sent its full plan to its reviewer
  through the Herdr message channel. Every command returned ok, `hsend.sh`
  reported `delivered`, and the plan never arrived. Ten minutes were lost, and
  they were lost only because **the executor noticed the silence, inspected its
  counterpart's pane, cited ED-0063 and ED-0066 by name, and reported it**
  rather than assuming delivery or waiting indefinitely.
- **Decision — paths, not payloads.** Pair documents (plans, challenges, diff
  summaries, verdicts) are written to a file under the gitignored `build/`
  directory, and the message names the PATH. `build/` is inside every agent's
  working directory, so it raises no permission prompt and never appears in
  `git status`. Long payloads do not survive this transport reliably and the
  failure is silent.
- **The reusable part, which is not about messaging at all.** `hsend.sh` has now
  been wrong in BOTH directions, and each error was introduced by fixing the
  other:
  1. **Version 2** verified delivery by checking whether the probe text was
     still visible. It reported DELIVERED messages as stranded, because a
     submitted message is echoed into the transcript and stays visible.
  2. **Version 3** fixed that by treating "peer is working" as proof of
     consumption. It reported STRANDED messages as delivered, because a peer
     that was ALREADY working is indistinguishable from one that just consumed
     the message. This is the version that lost the plan.
  3. **Version 4** verifies against the peer's scrollback and records whether
     the peer was already busy before the send.
  **A check is not correct because it stopped producing the failure you last
  observed.** Each version was written in response to a real observed failure,
  passed the case that motivated it, and was wrong about the symmetric case
  nobody had tested. This is the same family as ED-0041, ED-0053 and ED-0068,
  and it is the sharpest instance yet: the instrument the project uses to
  detect a failure mode was itself the failure mode, twice, in opposite
  directions.
- **Consequence for review standards.** When a pair fixes a check that was
  wrong, the reviewer must ask what the OPPOSITE error would look like and
  whether the new check would catch it. "It now reports correctly on the case
  that failed" is not evidence of correctness.
- **Reason:** Records a transport limit, and the more valuable lesson that
  corrective fixes to verification instruments tend to overshoot into the
  symmetric error.
- **Validation/reversal gate:** The paths-not-payloads rule is void if a
  transport arrives with acknowledged delivery. The overshoot lesson has no
  gate; it is a standing review question.

#### ED-0070 amendment (a) — the authority is per-record and per-quantity, not per-session

- **Date:** 2026-08-31
- **What the ruling missed.** ED-0070 said "this development host is the
  PROVISIONAL measurement authority" without checking what the committed
  evidence already said. It says something else. Every record under
  `docs/evidence/0035-size-corpus/` carries an `environment` block naming
  **linux, kernel 7.1.8-arch1-3, AMD Ryzen 9 7950X, 32 logical CPUs, btrfs,
  goos linux, goarch amd64**. This session's host is **Darwin 25.6.0, Apple M4
  Max, arm64, apfs**. The committed evidence was produced on a DIFFERENT
  MACHINE, so a ruling naming "this host" silently invalidated records that
  were never wrong.
- **What the numbers actually are, verified rather than assumed.** The
  orchestrator recomputed both budget records against their committed packages
  on this host:

      budget-empty-v1.json            3,597 bytes, sha256 8e8f4b39…  MATCH
      budget-unfurnished-1000-v1.json 119,394 bytes, sha256 eb1948ac… MATCH

  Byte-for-byte, digest-for-digest, on hardware and an OS that share nothing
  with the recording host. **These records contain no timings and no memory
  figures — only byte counts, digests and class breakdowns.**
- **The distinction the original ruling failed to draw.** A recorded quantity is
  either HOST-INDEPENDENT (byte counts, digests, class breakdowns — a
  deterministic function of committed bytes, reproducible anywhere) or
  HOST-DEPENDENT (elapsed time, peak memory, throughput). ED-0070 treated all
  measurement as host-dependent and required a host label on everything. For
  host-independent quantities that label is not merely unnecessary, it is
  MISLEADING IN THE OPPOSITE DIRECTION: it implies the figure is contingent on
  hardware when it is verifiable by recomputation on any machine, which makes
  the evidence look weaker than it is.
- **Amended decision.**
  1. Each record classifies every quantity as host-independent or
     host-dependent, and says which it is.
  2. Host-independent quantities need no measurement authority. They carry the
     digest of the artifact they derive from and are verifiable by
     recomputation. Anyone can falsify them anywhere; that is stronger than a
     host label.
  3. Host-dependent quantities carry an `environment` block naming the machine
     that ACTUALLY produced them. A record whose environment names a host that
     did not produce it is invalid regardless of whether its numbers are right.
  4. "This host is the authority" is withdrawn as a session-scoped claim. The
     authority is whichever host produced the figure, recorded truthfully.
  5. Nothing measured is ever a normative budget or a §9.3 limit. That part of
     ED-0070 stands unchanged.
- **The review failure this exposes, which is the reusable part.** Issue 0035's
  reviewer certified "ED-0070 compliance delivered" by confirming the
  `environment` block EXISTS and has populated fields. It never asked whether
  the host it names is the host the decision designates — which it is not. **A
  check on the presence of metadata cannot fail for the risk that the metadata
  is wrong.** That is the ED-0041 family again, in a place nobody was watching:
  not in a test, but in a reviewer's compliance assertion.
- **Reason:** Corrects a decision made without reading the artifacts it
  governed, and replaces a session-scoped authority with a per-record,
  per-quantity rule derived from what the evidence actually contains.

#### ED-0070 amendment (b) — ED-0031 already solved most of this, and I did not read it

- **Date:** 2026-08-31
- **The correction.** Amendment (a) presented the linux/amd64 provenance of the
  issue 0035 evidence as a finding. **It was already known, already documented
  in three places, and already engineered around**, and I would have known that
  had I read ED-0031 before writing ED-0070:
  - **ED-0031 (2026-08-26)** activated a continuation precisely because "the
    repository moved to a macOS/arm64 host" and issue 0035's gate false-failed
    there. It explicitly prohibits regenerating or re-measuring committed 0035
    evidence.
  - **`issues/0035-…md:102-129`** states the move, states that packages and
    digests still regenerate byte-identically so the budgets are intact, and
    states that "only the environment-provenance records diverge".
  - **`docs/evidence/0035-size-corpus/README.md:47-48`** names the recording
    host in full.
  - The delivered design cross-checks the records against
    `recordedHostIdentity()`, a reviewed constant restating the Linux fixture,
    so an evidence-only rewrite fails a test.
- **So ED-0070's "this development host is the PROVISIONAL measurement
  authority" was not merely under-specified, it CONTRADICTED a shipped design.**
  ED-0031 scopes the gate to the environment the records themselves name, which
  is the correct construction; ED-0070 asserted the opposite — that the current
  session's host is the authority — and would have invalidated conforming
  records had anyone acted on it.
- **What survives, and it is narrower than amendment (a) implied.** The
  host-independent versus host-dependent classification is a genuine addition:
  it explains WHY byte counts and digests remained unconditional in ED-0031's
  design while the seven environment-bearing records needed scoping, and it
  gives future measurement work the rule directly rather than by inference from
  a gate's behaviour. That much stands. The provenance "finding" does not.
- **What was genuinely wrong on issue 0035, unchanged by this correction.**
  Issue 0035's reviewer certified "ED-0070 compliance delivered" from the
  presence of populated `environment` fields, without checking what they named;
  and AC-3 was checked while its record contained none of the required content.
  Both stand as findings. Neither depended on the provenance being novel.
- **The lesson, and it is mine.** ED-0070 was written to unblock an issue
  quickly, on a question I had been carrying for two sessions, without reading
  the decision that had already answered it. **Speed applied to the wrong step:
  the blocked issue was blocked in my notes, not in the repository.** The
  orchestration rule this yields: before ruling on a question carried forward
  from an earlier session, re-read the decisions and the issue file first,
  because the project may have moved while the note did not.
- **Reason:** Corrects an overclaimed finding and a decision that contradicted
  shipped work, and records the failure mode of ruling from stale notes.

#### ED-0071 amendment (a) — the rule was advisory and the tool contradicted it

- **Date:** 2026-08-31
- **What happened.** ED-0071 told every pair to "exchange file paths, not file
  contents". Issue 0004's executor complied exactly: it wrote its revised plan
  to `build/w0004-plan-revised.md` and then ran
  `hsend.sh r0004 build/w0004-plan-revised.md`. **The plan stranded anyway**,
  because `hsend.sh` takes a file and sends its CONTENTS. The agent followed the
  rule to the letter and the tool defeated it.
- **The failure shape.** A rule expressed only in prose, enforced only by
  everyone remembering it, against a tool whose actual behaviour is the thing
  the rule forbids. The agent had no way to comply: naming the path and sending
  the file were the same gesture. Blaming the agent here would have been
  blaming it for the orchestrator's inconsistency — and it correctly reported
  the failure rather than reaching into its counterpart's session to fix it,
  which is the behaviour ED-0071's own transport rules ask for.
- **Decision — enforce it where it bites.** `hsend.sh` now REFUSES any payload
  over 1500 bytes, exits non-zero, and prints the correct two-step: write a
  short note naming the path, send the note. The document stays on disk where
  the peer reads it directly. The rule is now a property of the tool rather
  than a fact everyone must recall under load.
- **Confirmation that it works, from the first use.** The orchestrator's own
  next message — the one explaining this fix — was REFUSED by the new guard for
  being 3.4 KB. The author of the rule was its first violator, within a minute
  of writing it, and the tool caught him. That is the argument for mechanical
  enforcement over discipline, made better than any reasoning could.
- **The general lesson for this project's review standards.** When a rule and a
  tool disagree, the tool wins every time, silently, and the people following
  the rule take the blame. Before recording a practice rule, ask what would
  have to change for it to be MECHANICALLY impossible to violate — and if the
  answer is cheap, do that instead of writing the rule.
- **Reason:** Records that ED-0071's original form was unenforceable, and fixes
  it in the only place that can hold.

### ED-0072: Presence is not efficacy — three reviewers certified an artifact's existence and called it coverage

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** all review; joins ED-0041, ED-0053, ED-0068 and ED-0071
  as a proof-integrity rule
- **Three instances, three different pairs, one day.** Each is a reviewer
  confirming that a required ARTIFACT EXISTS and reporting that as evidence the
  artifact WORKS:
  1. **Issue 0035.** The reviewer certified "ED-0070 compliance delivered" from
     an `environment` block with populated fields. It never checked what the
     fields NAMED — a linux/amd64 host that is not this machine.
  2. **Issue 0029.** The regression test asserted `Category ==
     CatMalformedFraming` and `Stage == StageFraming`, and a later commit added
     an `actorID` substring assertion to "strengthen the discriminator". Under
     mutation, ALL THREE pass on the broken code. The only assertion that
     discriminates is the diagnostic wording.
  3. **Issue 0011.** The verdict recorded "✓ Inspected ReplayFromSnapshot gates:
     `validateFrontierClosure(log)` at :196, `authenticateAgainstBatches(log)`
     at :199". Under mutation, removing the closure gate turns a test red;
     **removing the authentication gate leaves the entire package green.** One
     gate is protected, its twin is not, and inspection could not tell them
     apart.
- **The distinction, stated once.** "The check is present at line N" and "the
  check does something no other code does" are DIFFERENT CLAIMS. Only the second
  is coverage. Presence is verifiable by reading; efficacy is verifiable only by
  removing the thing and observing a failure. A reviewer that reads cannot
  distinguish a load-bearing check from decoration, and both look identical in a
  green suite.
- **Decision.** A reviewer may not record a check, gate, guard, or metadata block
  as SATISFYING a requirement on the basis of its presence. Either mutate it and
  report the failure it causes, or state explicitly that its efficacy is
  unverified. "Inspected" is an honest word and it is not a synonym for
  "verified" — a verdict may say inspected, provided it does not then rest a
  conclusion on the stronger claim.
- **The sharpest instance, and why it is the useful one.** Issue 0011's reviewer
  applied mutation testing RIGOROUSLY to the closure helpers — executing two
  mutations itself, and confirming each made the exact witness it targeted go
  red rather than some other one — and then reverted to inspection for the
  gates, IN THE SAME VERDICT. The standard did not travel from where it was
  demanded to where it merely seemed reasonable. **A standard applied only where
  it was asked for is not a standard, it is compliance**, and it fails precisely
  at the places nobody thought to ask about.
- **Practical rule for gates specifically.** A gate added to preserve an
  invariant after a refactor moves validation elsewhere is the highest-risk case
  in this family: it looks like dead code to the next reader, and if no test
  fails when it is deleted, someone will eventually delete it with a green suite
  telling them it was safe.
- **Reason:** Records a failure mode that three independent reviewers reached on
  the same day, and fixes the remedy at mutation rather than at closer reading.
- **Validation/reversal gate:** None — a standing review rule. Its effect is
  observable: verdicts should begin distinguishing "inspected" from "mutated"
  explicitly, and the count of load-bearing checks with no witness should fall.

### ED-0073: The ADR-0002 trigger reproduces on current hardware — but the allocation profile is the unexamined variable

- **Date:** 2026-08-31
- **Status:** Active — this is EVIDENCE for an owner decision, not a decision
- **Affected issues:** 0022, 0030, 0006, 0007, 0013, 0012, 0016, 0017, 0032,
  0033 — the entire geometry chain
- **Why this was measured.** ED-0044 recorded that ADR-0002's revision trigger 3
  had fired on issue 0030's Phase 0 performance criterion, and that the whole
  geometry half of the project waits on an owner stack decision. Those numbers
  were taken on the previous Linux/amd64 host. The repository has since moved to
  macOS/arm64, so the recorded failure had never been checked against the
  hardware the project actually runs on. **A decision this large should not rest
  on a measurement nobody has reproduced.**
- **Re-measured on this host** (Apple M4 Max, darwin/arm64, Go 1.27,
  `GOTOOLCHAIN=local`, `-benchtime=2s -count=3`):

      BenchmarkIntersectSegmentsRobust      11934 / 12173 / 12127 ns/op
        (recorded on Linux: 12779.5 ns/op; bound 10000 ns/op)  -> STILL FAILS
      BenchmarkOffsetCertificateAndVertex   20458 / 20433 / 20460 ns/op
        (recorded on Linux: 20949.0 ns/op; bound 10000 ns/op)  -> STILL FAILS

  **The finding reproduces within about 5% on a completely different
  architecture**, and the second benchmark misses its bound by more than 2×.
  The trigger-3 disposition is confirmed, not an artifact of the old host. That
  removes the most obvious reason to doubt it.
- **But the benchmark output carries a variable the disposition did not
  consider.** Alongside the timings:

      BenchmarkIntersectSegmentsRobust      14765 B/op   426 allocs/op
      BenchmarkOffsetCertificateAndVertex   21396 B/op   538 allocs/op

  **Four hundred and twenty-six allocations for one robust segment
  intersection, and five hundred and thirty-eight for one offset
  certificate.** ED-0044 records the disposition as "NOT PLAUSIBLY CLOSABLE by
  caching or locality" — caching and locality are about memory ACCESS, and this
  is about allocation VOLUME. They are different questions and only the first
  appears to have been asked.
- **Why this matters for the decision.** ADR-0002's validation gate 2 names
  "unacceptable GC/allocation behavior" as a distinct concern from raw speed,
  and its failure trigger is "robust geometry requires maintaining an
  unreasonable custom numerical stack". An allocation count in the hundreds per
  predicate is not obviously inherent to exact-predicate arithmetic; it is the
  signature of an implementation allocating intermediates rather than of a
  language that cannot go fast enough. **If the timing is allocation-dominated,
  the bound may be reachable in Go and the trigger would be premature. If it is
  not, the case for reassessing the stack is much stronger than it looks today,
  because it survives a 2× hardware change.** Either way the owner is currently
  being asked to decide without that distinction resolved.
- **Recorded as evidence, and explicitly NOT a recommendation.** ADR-0002's
  reassessment is owner territory and this record does not encroach on it. What
  it does is remove one doubt (host-specificity) and surface one unexamined
  variable (allocation), so the decision is made against better information than
  it would have been an hour ago.
- **Reason:** A decision gating ten issues rested on unreproduced numbers from
  retired hardware. They now reproduce, and the profile alongside them raises a
  question the disposition did not address.
- **Validation/reversal gate:** A bounded allocation-reduction spike on the two
  named benchmarks would settle it: if allocations fall by an order of magnitude
  and the timings approach the bound, trigger 3 should be reconsidered before
  any stack change. That spike is cheap relative to a language migration and is
  the orchestrator's recommended next step IF the owner wants the question
  closed before deciding.

### ED-0074: A deliverable that lives only in `build/` has not been delivered

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** all dispatch and integration review
- **Three pairs, three issues, one session.** Each performed real work, reported
  it as complete, and left its actual output in the gitignored `build/`
  directory, where integration would have deleted it:
  1. **Issue 0035** reported "linux/amd64 provenance statement preserved". The
     statement existed only in `build/`.
  2. **Issue 0028** reported Task 2 complete. The ENTIRE partial-history
     assessment — three labelled success oracles and their gap record — existed
     only in `build/`. Its tracked diff was L33 machinery and nothing else.
  3. **Issue 0029b** reported the §9.7 fault matrix complete. Nine passing
     F1–F9 discriminator tests landed in tracked code, but the
     executable-versus-deferred disposition — including the sentence stating
     what those tests "DO NOT and CANNOT prove" — existed only in
     `build/fault-matrix-audit.md`.
- **Why the third one is the clearest.** Purging that pair would have left a
  green F1–F9 matrix in the repository with NO record that power-loss
  durability, kernel-crash atomicity, cross-device, concurrent-writer and X9
  cache-GC are deferred and unproven. **The tests would have survived and the
  limits on what they establish would not** — which is precisely how a green
  suite comes to mean more than it should. The audit's most valuable sentence
  was the one about its own limits, and that was the sentence headed for
  deletion.
- **Why this keeps happening, and it is not carelessness.** `build/` is where
  the pair contract tells agents to put working documents, because it is
  gitignored and therefore cannot dirty `git status` or break `verify.sh`'s
  clean-tree check. That makes it the path of least resistance for EVERY
  document, including the ones that are the deliverable. The instruction that
  keeps `verify.sh` green is the same instruction that quietly discards output.
- **Decision.** Work that exists only in `build/` is NOT DELIVERED, and no
  handoff may claim a task complete on the strength of it.
  1. Every brief states which artifacts must land in TRACKED files.
  2. Reviewers verify the deliverable is tracked before approving, not only
     that the work was performed. Two of the three approvals above certified
     that the ASSESSMENT happened without checking that its OUTPUT landed
     anywhere durable — the ED-0072 confusion running in the other direction.
  3. **The orchestrator diffs every branch for tracked content before
     integrating.** All three were caught this way and by nothing else.
  4. Analysis, transcripts and working notes legitimately stay in `build/`. The
     test is simple: if purging the pair would destroy it and someone would
     later need it, it is a deliverable.
- **Reason:** Records a failure that three independent pairs reached
  identically in one session, and locates its cause in the contract's own
  guidance rather than in the agents.
- **Validation/reversal gate:** None — a standing rule. Its effect is
  measurable: the count of integration reviews that find an untracked
  deliverable should fall to zero.

#### ED-0073 amendment (a) — the benchmark that fired the trigger has no fast filter at all

- **Date:** 2026-08-31
- **How this surfaced.** Issue 0007 was asked to record the H-6 fallback and
  escalation rate beside its allocation figures, because 426 allocs/op cannot be
  interpreted without knowing how often the adaptive filter escalates to the
  exact path. Its executor came back with a question instead of a number:
  `IntersectSegments` has no filter to escalate FROM.
- **Verified independently by the orchestrator.**
  - `IntersectSegments` (`internal/geometry/predicates/segment.go:113`) calls
    `crossOfDiffs` directly.
  - `crossOfDiffs` (`exact.go:76-85`) builds `big.Rat` UNCONDITIONALLY — four
    `floatToRat` conversions, two subtractions, two multiplications and a final
    subtraction, every one allocating — on EVERY call, irrespective of how
    well-conditioned the input is.
  - `orientFast` is defined at `orient.go:97`. **`segment.go` references it
    ZERO times.**
- **What this establishes, stated no more strongly than the evidence allows.**
  The 426 allocs/op figure is NOT a filter-escalation rate and must not be read
  as one; it is the unconditional cost of an exact-only path. **The package
  already contains the adaptive-filter technique, and the predicate whose
  benchmark fired ADR-0002's revision trigger does not use it.**
- **What this does NOT establish.** It does not show the 10,000 ns/op bound is
  reachable, does not estimate what a filter would cost or save here, and does
  not resolve the stack question. Those belong to issue 0030's judgment and to
  the owner. The orchestrator explicitly instructed issue 0007 NOT to draw the
  conclusion, because a measurement issue that editorialises stops being
  evidence.
- **Why it matters to the decision anyway.** ADR-0002's failure trigger reads
  "robust geometry requires maintaining an unreasonable custom numerical
  stack" — a claim about the LANGUAGE and its ecosystem. An unfiltered
  exact-only predicate is a claim about THIS IMPLEMENTATION, and a different
  language given the same unconditional `big.Rat`-equivalent arithmetic would
  pay a comparable cost. The owner is entitled to know that the measurement
  behind the trigger is of a code path the project already knows how to make
  faster elsewhere, before deciding whether the language is at fault.
- **A second observation, recorded for the same reason.** ED-0073 noted
  `math/big.nat.make` as 72–95% of allocated objects. Combined with the above,
  the allocation is arbitrary-precision rational arithmetic performed
  unconditionally — which is an algorithmic choice, not a runtime property.
- **Process note worth keeping.** The executor stopped and asked rather than
  instrumenting production code to manufacture an escalation figure that would
  have looked like evidence and meant nothing for this predicate. That refusal
  is what produced the finding. A measurement that cannot exist is more useful
  reported as impossible than approximated into existence.

### ED-0075: The fourth slot stays empty with a computed reason — the project is decision-bound, not capacity-bound

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** all scheduling
- **Audit performed against the CURRENT tree**, not a remembered conclusion —
  the error ED-0069 records as an orchestration incident. Nine issues integrated
  this session (0035, 0029, 0004, 0028, 0011, 0021, 0029b, 0011-audit, 0007).
  Three pairs are live: issue 0005 (§7.6 diagnostic placement), issue 0010
  (`lwwHorizon` decision preparation), issue 0029c (AC3 limits enforcement).
- **Every remaining candidate, and why it is not dispatchable:**
  - **0006** — all five acceptance criteria are CHECKED. No open work; it stays
    `In Progress` only on issue 0030's deferred-closure gate.
  - **0026** — no open work, recorded on 2026-08-29.
  - **0012, 0013, 0014, 0016, 0017, 0032** — all gated on 0013, which is gated
    on 0006/0007, which are gated on the ADR-0002 decision.
  - **0015, 0027** — merged partial handoffs already; open only on 0011 and
    0029 gates, so a pair could close nothing.
  - **0022, 0030** — the ADR-0002 chain itself. Owner decision.
  - **0034** — remaining criteria need external registry results and OWNER
    AUTHORIZATION to reserve namespaces and select application IDs.
  - **0010** — dispatched, but note its `lwwHorizon` VALUE is also an owner
    decision; the pair prepares it and may not assert it.
- **Decision — leave the fourth slot EMPTY and say why.** `INSTRUCTIONS.md`
  permits a slot to remain empty with a documented dependency, authority or
  validation reason, and ED-0044 already recorded the principle: filling slots
  with blocked or colliding work "would manufacture activity, not progress."
  Dispatching 0006 or 0026 would produce a pair with nothing to close;
  dispatching a gated CLI issue would produce work that cannot be accepted.
- **The state this reveals, and it is the important part.** Earlier today the
  binding constraint was reviewer capacity (ED-0060), then it was slots
  (ED-0069). **It is now neither.** Nine integrations cleared the dispatchable
  backlog, and what remains is behind two owner decisions — the ADR-0002 stack
  question and, secondarily, issue 0034's namespace authorisations. More pairs
  cannot move a decision.
- **The evidence set for the ADR-0002 decision is now COMPLETE**, which is the
  practical result of the session. Issue 0030's Phase 0 judgment named three
  inputs: issue 0006's predicate benchmarks (criterion checked), issue 0007's
  wall-clock and allocation measurements (integrated today), and H-6
  fallback/escalation data (integrated today, and it changed the picture — see
  ED-0073 amendment (a)). Nothing further is measurable without the owner
  ruling first.
- **Reason:** Records that the queue is empty for computed reasons rather than
  through oversight, and that the project's limiting factor has moved from
  capacity to authority.
- **Validation/reversal gate:** Re-run this audit the moment any live pair
  lands or either owner decision is answered, and fill the slot immediately.

### ED-0076: Two §9.3 threat-model limits have guards no test protects, and one of them is on the reader path

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** 0029 (AC3), 0028 (fixture discrimination), 0023
  (threat model)
- **Why the audit was commissioned.** Issue 0029's AC3 — "Reader enforces all
  threat-model limits before excessive allocation or extraction" — was recorded
  satisfied "via executable probes **plus declarative GiB-scale rows**". A
  DECLARATIVE row is a written claim that a limit is enforced. Some are
  declarative for a sound reason (a genuine GiB input cannot be materialised in
  a suite); others may be declarative only because nobody tried. Separating
  those was the round.
- **The result, from 33 audited rows.**
  - **6 EXECUTED with mutation proof** — L6-writer, L11, L15, L17, L18, L23.
    Each guard was removed and a named test went red.
  - **2 PROVEN NON-DISCRIMINATING** — L3 and L6-reader. **Their guards can be
    deleted with a green suite.**
  - ~13 unverified, 2 unmaterialisable at GiB scale (L1, L2), 11 not applicable.
- **The L6 asymmetry is the finding, and it is worse than the count suggests.**
  Verified independently by the orchestrator: removing
  `if e.count > LimitEntryCount` at `internal/persist/zipread.go:215` leaves the
  entire `internal/persist` package GREEN (`ok, 7.781s`). The WRITER-side guard
  at `zipwrite.go:165` IS tested and load-bearing —
  `TestLimitL6EntryCountBoundary` goes red when it is removed.

  **But a hostile package is READ, never written.** The tested path is the one
  an attacker does not use; the untested path is the one they do. L6 exists to
  refuse an entry-count explosion BEFORE allocation, which is a reader-side
  obligation by construction. A single test name covering "L6" made the row
  look covered while the side that matters had nothing behind it.
- **L3 is the same shape in a different disguise.** Its corpus fixture rejects
  for a reason OTHER than the L3 guard — an earlier structural check fires
  first — so the fixture appears to exercise L3 and does not. This is ED-0053's
  wrong-reason failure sitting inside a security limit.
- **Decision — record, do not repair, in this round.** Finding these is the
  deliverable; fixing them is scoped work deserving its own dispatch and its own
  adversarial review. AC3 stays OPEN and its "all threat-model limits" wording
  may NOT be read as satisfied. The audit lands in tracked content so the gap is
  visible without the pair that found it.
- **What this says about coverage claims generally.** Three rows carried a test
  whose NAME matched the limit. Only mutation separated the one that protects it
  from the two that do not. **A limit is enforced by whichever code path a
  hostile input actually traverses, not by whichever path a test happens to
  exercise** — and no amount of reading distinguishes those.
- **Reason:** Records that a security-relevant acceptance criterion was
  overstated, identifies precisely which rows fail and why, and keeps the repair
  separate from the discovery.
- **Validation/reversal gate:** Closed when L3 and L6-reader have executed
  guards with mutation proof, and when the ~13 unverified rows are either
  proven discriminating or reclassified honestly.

### ED-0077: A verification claim must carry the run's OUTPUT, not its asserted result

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** all dispatch; extends ED-0068 to the verify gate itself
- **Twice in one session a pair reported `./scripts/verify.sh` exit 0 on a
  branch where that run could not have happened.**
  1. **Issue 0011 round 3.** Handoff reported a clean verification. Four files
     were not gofmt-clean, including a trailing tab inside the very gate comment
     the orchestrator had requested. `verify.sh` runs `test -z "$(gofmt -l .)"`
     under `set -e`; it cannot exit 0 with those present.
  2. **Issue 0029 L3/L6 round.** Same claim, same defect —
     `internal/persist/limits_test.go` carried a trailing-whitespace line at
     :327. Caught at integration by the orchestrator's own verify run.
- **Why this is not carelessness, and why the remedy is mechanical.** The
  gofmt stage produces NO OUTPUT when it fails — `test -z "$(gofmt -l .)"`
  simply returns non-zero and `set -e` aborts. A pair scrolling its terminal
  sees the test results scroll past, sees no error text, and concludes the run
  passed. **The failure is silent by construction**, so "I ran it and it was
  fine" is an honest report of a misleading observation.
- **Decision.** A handoff claiming verification MUST include the actual tail of
  the run and its exit status, captured mechanically:

      ./scripts/verify.sh > build/verify.log 2>&1; echo "EXIT $?"
      tail -5 build/verify.log

  The handoff quotes that `EXIT` line. An assertion of "exit 0" without the
  captured status is not evidence and is refused at integration review, exactly
  as a mutation transcript without its run is refused under ED-0068.
- **This is ED-0068 applied to the gate rather than to a test.** That decision
  requires evidence to be the OUTPUT of the artifact being submitted; a verify
  claim is a claim about a run, and the run's exit status is its output. The
  same standard the project applies to mutation proofs applies to the gate that
  admits them.
- **Consequence for the orchestrator, which is the part that actually held.**
  Both breaches were caught because integration review runs `verify.sh` on the
  MERGED tree independently, and neither cost more than one commit. That check
  is not redundant with the pair's own run and must not be dropped for speed.
- **Reason:** Records a silent-failure mode in the verification contract itself
  and fixes it at capture rather than at exhortation.
- **Validation/reversal gate:** Void if `verify.sh` is changed to report the
  failing stage explicitly on abort, which would remove the silence this
  decision compensates for. That change would be an improvement and is worth
  making.

#### ED-0076 amendment (a) — a pair contradicted its own verified work by re-solving a solved problem

- **Date:** 2026-08-31
- **What happened.** Issue 0029's L3/L6 pair committed `8f36286`, which added
  `TestLimitL6EntryCountReaderRefusal` — a working, discriminating reader-side
  test for L6. The orchestrator merged it after mutating the guard and watching
  the test go red. The pair then committed `d7ae1b9`, whose documentation
  states L6-reader is **BLOCKED**: "hostile fixture unmaterializable within
  validation ordering (T1 exact-fit rule rejects Zip64 fixtures before
  `parseDirectory()` at line 215)".
- **The second claim is disproved by the first, and the disproof is mechanical.**
  On main this moment the test PASSES, and removing
  `if e.count > LimitEntryCount` at `zipread.go:215` turns it RED with
  "expected limit-entry-count, got trailing-data". **If T1 rejected the fixture
  first, the test could neither pass with the guard present nor change its error
  when the guard is removed.** The discrimination is itself the proof that the
  described blocker does not apply to the fixture already built.
- **The likely mechanism, and it is the reusable part.** The pair solved the
  problem with a declaration-only fixture — a Zip64 EOCD CLAIMING 65,536 entries
  while the archive stays small, which works because the guard must fire before
  allocation. It then re-attacked from a different angle, a genuine
  65,536-entry central directory at ~4 MiB, hit a REAL T1 ordering wall on that
  approach, and wrote the blocker up as a property of the ROW rather than of the
  METHOD. **A blocker discovered while exploring approach B is not evidence
  about a row already closed by approach A**, and the write-up did not
  distinguish them.
- **Why this was worth stopping for.** Merging that text would have placed a
  passing, discriminating test on main beside documentation asserting the row
  cannot be tested. A later reader resolves that contradiction by deleting the
  test as invalid or by trusting the documentation and not looking — and BOTH
  outcomes leave the reader-side allocation guard unprotected, which is the
  exact defect the round existed to close. A self-contradicting record is worse
  than a silent gap, because it actively argues against its own fix.
- **Decision.** The T1 finding is KEPT and relabelled as a constraint on the
  large-fixture approach — it is a genuine navigational hint for anyone who
  attempts that route. The row is recorded EXECUTED. Row counts return to 7
  executed including L6-reader.
- **Standing rule.** When a later finding contradicts earlier verified work,
  the contradiction is resolved against the EXECUTABLE evidence before either
  claim is recorded. A test that passes and discriminates outranks a prose
  assessment of why it should not.
- **Reason:** Records a failure shape not previously on file — self-contradiction
  produced by re-solving a solved problem — and fixes the disposition at the
  executable evidence rather than at the more recent write-up.

### ED-0078: A tracker consistency checker exists; wiring it into verify.sh is deliberately deferred

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** all tracker hygiene
- **The gap it closes.** `verify.sh` gates code, formatting, vet, SBOMs, the
  release inventory and a clean tree. **It has never gated the tracker** — and
  the tracker is where every integration decision is recorded and where the next
  orchestrator starts reading. It drifted TWICE this session, both times caught
  by an ad-hoc audit rather than by any gate:
  1. `issues/README.md` listed issue 0007 as `In Progress` while its file said
     `In Progress (partial scope)`.
  2. A pair created `issues/0010-lwwhorizon-fixation-analysis.md`, putting two
     `0010-` files in the tracker against README's "One issue per
     `NNNN-slug.md`; IDs are unique". It carried no `Status` line, so a glob
     over `issues/0*.md` returned it as a malformed issue.
- **Delivered.** `cmd/domowik-tracker-check` (matching the existing
  `domowik-sbom` and `domowik-jsoncheck` command precedents) with tests, and
  failure proofs at `docs/evidence/tracker-consistency/FAILURE-PROOFS.md`. It
  detects status disagreement between file and index, duplicate `NNNN` prefixes
  across root/`done/`/`dropped/`, issue files with no `Status` line, and index
  rows with no file.
- **Verified by the orchestrator, not accepted on report.** Clean tracker exits
  0; the real 0007 status mismatch exits 1 naming both values; a duplicate
  `0010-` prefix exits 1 naming both paths. The pair proved the other two
  classes itself rather than leaning on those two — the standard ED-0072 asks
  for, met without being pressed a second time.
- **Deliberately NOT wired into `verify.sh` yet.** Two pairs were live when this
  landed, and a newly failing gate would have blocked them through no fault of
  their own. The one-line addition is proposed in the branch handoff and applies
  once the live pairs land. **Adding a gate mid-round is a change to other
  people's contract, not a local improvement.**
- **A caution for whoever wires it in.** The checker must be run BEFORE the
  clean-tree check in `verify.sh`, not after, or a tracker fix made during a run
  would itself dirty the tree and fail the later gate — the ordering trap that
  cost issue 0011 a cycle earlier today.
- **Reason:** Records a hygiene gate the project lacked, the evidence that it
  discriminates, and why its activation is sequenced rather than immediate.
- **Validation/reversal gate:** Closed when the `verify.sh` line is applied and
  a full run passes with it present. Void if the tracker convention changes.

### ED-0079: "Unreachable" does not license deletion

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** 0005, 0028, 0029; all specification rounds
- **Two pairs met the same situation on the same day and handled it
  differently.** Both found a rule that cannot fire inside the normative v0.1
  profile:
  - **Issue 0029, L3.** Ruled UNREACHABLE because the L5 guard at
    `zipread.go:642` precedes it with an identical threshold. Recorded the
    precedence chain. **Changed no production code.** Correct.
  - **Issue 0005, `ambiguous-manifest`.** Ruled UNREACHABLE under the
    closed-namespace profile — probably correct — and then began DELETING
    `CatAmbiguousManifest` from `internal/persist/errors.go`. Caught in the
    working tree before commit.
- **What the deletion would have cost.** It does not compile:
  `internal/persist/reader.go:229` uses the constant. It orphans committed
  material owned by another issue: the `ambiguous-manifest.domo` fixture, its
  generator at `corpusgen/generate.go:305`, a coverage-set entry at
  `corpus_test.go:282`, and the gap record at `EXPECTATIONS.md:1316`. And it is
  a FORMAT CONTRACT change — `FORMAT-SPEC:1674` lists the token in the §9.4
  taxonomy and `:1137` requires it for a named condition — made inside a round
  scoped to ruling specification and routing implementation.
- **The distinction, which is the whole decision.** **"This rule cannot fire"
  and "this rule should not exist" are different claims with different owners.**
  The first is a finding a specification round can make and prove. The second
  changes what a conforming reader may emit, invalidates fixtures, and requires
  a compatibility assessment — it belongs to the format owners.
- **Decision.** An unreachability finding is recorded with the PRECEDENCE CHAIN
  that makes it unreachable, and the question of whether the rule should survive
  is ROUTED to its owners with the dependent artifacts named. Production code,
  taxonomy tokens and fixtures are not removed on the strength of an
  unreachability ruling alone.
- **Note the honest tension, which this decision does not dissolve.** An
  unreachable rule IS the ED-0041 shape living in a specification: a check that
  cannot fail is indistinguishable from a check that is absent. That is a real
  defect and it deserves resolution — by narrowing the profile so the class can
  fire, or by a deliberate owner decision to retire the token with its fixtures.
  What it does not deserve is silent deletion by whoever found it.
- **Reason:** Records that a correct finding can license an incorrect action,
  and fixes the boundary at routing rather than at the finder's judgement.
- **Validation/reversal gate:** None — a standing scope rule. Closure of the
  specific `ambiguous-manifest` and L29 questions belongs to the §9.4 owners.

### ED-0080: Owner direction — continue an issue with its existing pair; restart only on a dead end, under the same name

- **Date:** 2026-08-31
- **Status:** Active
- **Decided by:** project owner
- **Affected issues:** all scheduling; CORRECTS `INSTRUCTIONS.md` item 7
- **Owner direction, recorded in substance:** `INSTRUCTIONS.md` was wrong to say
  "a continuation of an incomplete issue counts as new work and receives a
  fresh two-agent Herdr workspace and contract". Unless the pair is in a DEAD
  END, the previously started issue continues with the SAME pair. If it is a
  dead end, close the old agents and start replacements **under the same name —
  no additional letter.**
- **The orchestrator's own evidence supports this, which is why it is recorded
  rather than merely obeyed.** Issue 0029 was dispatched FOUR times today under
  the old reading. The fourth round produced three distinct failures — a
  document contradicting the pair's own verified work, a handoff claiming a
  `verify.sh` run that could not have passed, and a branch reset that discarded
  already-merged commits — and **all three trace to one root: the pair had lost
  track of what was already true on main.** A continuing pair carries that
  context; a fresh pair must rebuild it and sometimes rebuilds it wrong.
- **The naming half matters too.** Worktrees and workspaces were labelled
  `issue-0029`, `issue-0029b`, `issue-0029c`, `issue-0029d`. There is no issue
  0029d — the tracker holds exactly one `issues/0029-*.md`. The orchestrator
  invented a `NNNN + letter` scheme that reads exactly like an issue ID, in a
  project whose tracker enforces "one issue per `NNNN-slug.md`; IDs are unique"
  strictly enough that a checker was built for it today (ED-0078). Status
  reports then used "0029d" as though it named a tracker entity. **An issue has
  one identity.**
- **Decision.**
  1. When a round lands and scope remains, hand the next scope to the SAME pair.
  2. Restart only on a dead end — exhausted, wedged, out of budget, or provider
     lost — and then reuse the same agent names and worktree name.
  3. Branch names stay descriptive per round (`issue/0029-l3-l6-guards`), which
     was always the right pattern; the letters were only ever in directory and
     workspace labels.
- **Applied immediately.** Issue 0029's pair is live and its L3/L6 round is
  integrated, so it CONTINUES with the next scope — the ~13 §9.3 rows whose
  discrimination is unverified — rather than being purged. The existing
  `issue-0029d` directory is legacy: renaming it would break both running
  agents' working directory, so it stands until this pair reaches a natural
  boundary, and no further suffixed directory is created.
- **Reason:** Records an owner correction to the recovery contract, and the
  measured cost of the practice it replaces.
- **Validation/reversal gate:** None — owner direction. `INSTRUCTIONS.md` item 7
  is amended and items 8 and 9 added.

### ED-0081: Normative means two implementers could not disagree — the specification analogue of ED-0072

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** 0004, 0005, 0021, 0022, 0030; all specification rounds
- **The gap this closes.** Several acceptance criteria in this project assert
  that something "is normative" — issue 0005's criterion 5 is
  "Operation-batch naming/order/framing and snapshot frontier are normative".
  Nothing states how to TELL. The available failure is obvious once named: a
  pair reads the section, finds the topic discussed at length, and checks the
  box. **Presence of prose stands in for bindingness**, which is ED-0072's
  shape moved from tests into specification text.
- **The test.** Text is NORMATIVE when **two competent implementers reading it
  independently could not disagree about what is REQUIRED.** The operative
  question is never "does the specification mention this" but "could a
  conforming implementation do something different and still claim
  conformance?" If yes, the text is DESCRIPTIVE, and the criterion is not
  satisfied however much prose exists.
- **Worked instances, so the test is concrete rather than a slogan.**
  - **Naming:** is `ops/<counterBase>-<actorId>.json` REQUIRED, or shown as
    what this writer happens to emit?
  - **Order:** is ascending-by-name fixed, or is determinism asserted without
    fixing WHICH order?
  - **Framing:** are duplicate-ID, range-gap and overlap checks stated as
    MUST-reject with a NAMED diagnostic, or merely described as things a reader
    does?
  - **Frontier:** does §5.1 S2/S3 fix the persisted shape, or leave a writer
    free to choose among several?
- **Why this belongs with the proof-integrity family.** ED-0041 refuses a check
  that cannot fail. ED-0053 refuses a proof that fails for the wrong reason.
  ED-0072 refuses presence as evidence of efficacy. ED-0076 found a limit
  enforced only on the path hostile input does not take. **A specification
  satisfiable two different ways is the documentation equivalent of a guard no
  test protects** — it looks like a requirement, occupies the place a
  requirement would occupy, and constrains nothing.
- **A caution recorded with it.** Do not check a criterion on the strength of a
  TRACKER note claiming delivery. The tracker records that work happened; the
  normative text is the artifact. Issue 0005 was asked today to verify against
  §7.6 itself rather than against its own delivery line, precisely because
  treating a record as proof of what it records is the error this whole family
  describes.
- **Reason:** Supplies a decidable test for a word several acceptance criteria
  depend on, and extends the project's proof-integrity standard from executable
  artifacts to normative ones.
- **Validation/reversal gate:** None — a standing review rule. Its effect is
  observable: criteria asserting normativity should start citing the clause and
  the MUST, not the section.

### ED-0082: Write a state anchor BEFORE an agent compacts, not after

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** all dispatch; operationalises ED-0080
- **Why this matters more since ED-0080.** The owner's correction keeps a pair
  alive across rounds precisely so it retains context a fresh pair would have to
  rebuild. **Compaction is the one event that undoes that from the inside**, and
  it happens silently: the agent continues, sounds coherent, and has lost the
  reasoning behind conclusions it still asserts.
- **Observed today.** `w0029` went from ~174K to ~43K context mid-round. An
  anchor written fifteen minutes earlier — speculatively, because the number
  looked high — became the thing that told it its L3/L6 round was already
  integrated and must not be rebuilt. Minutes later `r0029` crossed the same
  threshold and was anchored BEFORE compacting rather than after.
- **What a summary reliably loses, and what an anchor must therefore carry.**
  1. **What is already integrated on main**, by merge SHA, with "do NOT rebuild
     this" stated. This is the costliest loss: issue 0029 produced three
     separate defects today from not knowing what main already contained.
  2. **That the pair is CONTINUING, not starting** — otherwise it re-plans.
  3. **The current scope**, distinguished from the scope it just finished.
  4. **The standing rules that bind it** (ED-0074 tracked deliverables, ED-0077
     captured verify status, ED-0079 no deletion on unreachability).
  5. **The specific mistake it made last round.** A compacted agent repeats
     precisely the error whose correction lived in the summarised context.
- **Decision.** When an agent passes roughly 150K context, write
  `build/STATE-ANCHOR[-<agent>].md` and tell it to trust that file over its own
  summary after compacting. The monitor now flags context pressure so this is
  triggered by observation rather than by noticing.
- **Why the anchor is trustworthy when the summary is not.** It is written from
  OUTSIDE the agent, from the integrated repository state, by the party that
  performed the merges. A summary is the agent's compression of its own
  recollection; the anchor is a record of what is true.
- **Reason:** Records a cheap, mechanical mitigation for the one failure mode
  that defeats the owner's continuation policy, and moves it from opportunistic
  to triggered.
- **Validation/reversal gate:** None — standing practice. Its effect is
  observable: a compacted agent should not re-plan, re-derive settled work, or
  re-open an integrated round.

### ED-0083: Activate issue 0014 under PARTIAL SCOPE — a usable CLI surface without the geometry chain

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** 0014; consumes 0010, 0011, 0026, 0029 as provisional
  handoffs
- **Prompted by an owner question with an unsatisfying answer.** Asked whether
  the application could be run and used, the honest answer was essentially no.
  `domowik --help` says it plainly: "domowik is a scaffold placeholder, not the
  planned CLI… Document, geometry, and export commands are not implemented
  here." What runs today is `ops`, `compact`, `asset` and `pack` — inspection
  and packaging. **There is no way to create a document or check one.**
- **The dependency that blocked it is only PARTLY real.** Issue 0014 lists
  `0013` (geometry engine) among its dependencies, and 0013 sits behind the
  ADR-0002 decision. But 0014's problem statement scopes validation to
  "logical, archive, asset, **and derived-geometry** integrity" — **only the
  last of the four needs 0013.** Its five acceptance criteria are about
  validation modes, output identification, exit codes, opaque-data preservation
  and hostile-container safety; none is geometric.
- **Decision.** Activate issue 0014 under PARTIAL SCOPE: `new`, `info` and
  `validate` over logical, archive and asset integrity. **Derived-geometry
  validation is explicitly DEFERRED to issue 0013** and no geometric claim may
  be made. The issue cannot reach `Done` under this activation; it delivers a
  labelled partial handoff exactly as issues 0021, 0006, 0007, 0030 and 0034
  have.
- **Why this is an orchestration call and not product direction.** The three
  commands and their five criteria are already specified in issue 0014; nothing
  new is being invented or designed. What changes is SEQUENCING — delivering the
  non-geometric three-quarters now rather than waiting on a decision that gates
  ten issues. If the owner would rather this wait, it reverses at no cost: the
  branch is isolated and nothing downstream depends on it.
- **What it should produce.** A CLI that can create a document, describe one,
  and validate one — enough to exercise the format, container, replay and asset
  layers this project has spent its effort on, from outside. The value is not
  only the commands: **an end-to-end surface is the first thing that can falsify
  the layers beneath it**, and this project has never had one.
- **Reason:** Converts a dependency that is three-quarters spurious into
  deliverable work, and answers a reasonable owner question with something
  runnable rather than an explanation.
- **Validation/reversal gate:** Reversible by owner direction. Closure of issue
  0014 still requires derived-geometry validation and therefore issue 0013.

### ED-0084: Interrogate dependency edges; a listed dependency is a claim, not a fact

- **Date:** 2026-08-31
- **Status:** Active
- **Affected issues:** all scheduling; qualifies ED-0075
- **What happened.** Asked whether the application could be run and used, the
  orchestrator answered honestly that it could not, and cited the geometry chain
  as the blocker. Issue 0014 (`new`, `info`, `validate`) lists `0013` — the
  geometry engine — among its dependencies, and 0013 is behind the ADR-0002
  owner decision. The issue was therefore treated as unreachable, including in
  ED-0075's audit, which enumerated it among candidates blocked behind 0013.
- **The edge was three-quarters spurious.** Issue 0014's problem statement
  scopes validation to "logical, archive, asset, **and derived-geometry**
  integrity". **Only the fourth needs 0013.** None of its five acceptance
  criteria — validation modes, output identification, exit codes, opaque-data
  preservation, hostile-container safety — is geometric. A partial scope
  covering the first three domains was available the whole time and was
  activated as ED-0083.
- **Why it went unnoticed for a whole session.** The dependency graph was read
  as a set of BINARY edges: 0014 depends on 0013, 0013 is blocked, therefore
  0014 is blocked. That is how `Depends on:` lines invite being read, and it is
  how ED-0075's slot audit read them. **But a dependency edge summarises a
  relationship whose SHAPE is in the issue text**, and the shape here was "one
  of four integrity domains", not "all of it".
- **Decision.** Before recording an issue as blocked by a dependency, read the
  DEPENDENT issue's problem statement and acceptance criteria and ask which
  criteria the edge actually touches. If some criteria are independent of the
  blocked upstream, a partial-scope activation is available and the issue is not
  blocked — it is partly blocked. Record which criteria the edge covers, so the
  next audit does not re-derive it.
- **This qualifies ED-0075 rather than overturning it.** That decision's
  conclusion — the project is decision-bound, not capacity-bound — still holds
  for the geometry chain proper (0022, 0030, 0006, 0007, 0013, 0012, 0016,
  0017, 0032). What it got wrong was one entry: 0014 was listed as blocked
  behind 0013 when three of its four domains never needed it.
- **The general form, which this project keeps meeting.** A `Depends on:` line
  is a CLAIM about a relationship, recorded once and thereafter trusted. This is
  the same shape as ED-0072 (presence of a check is not efficacy), ED-0081
  (presence of prose is not normativity), and ED-0076 (a test naming a limit is
  not a test protecting it). **The dependency graph is another artifact whose
  assertions are inherited rather than verified**, and it deserves the same
  scepticism as a green test.
- **Reason:** Records that a session's worth of "nothing is dispatchable" rested
  partly on an unexamined edge, and fixes the remedy at reading the criteria
  rather than at trusting the line.
- **Validation/reversal gate:** None — standing practice. Its effect is
  observable: blocked-issue audits should cite WHICH criteria an edge blocks,
  not merely that an edge exists.

### ED-0085: Owner direction — strictly sequential work runs SINGLE-THREADED, not as parallel pairs

- **Date:** 2026-08-31
- **Status:** Active
- **Decided by:** project owner
- **Affected issues:** all scheduling; qualifies ED-0044, ED-0069 and ED-0075
- **Owner direction, recorded in substance:** where the remaining work is
  strictly sequential, do NOT add agents for it — run it single-threaded.
- **What this settles.** ED-0044 raised the concurrency cap to four on owner
  direction and ED-0069 filled all four slots. Both are about CAPACITY. This
  decision is about SHAPE: a cap of four is a ceiling, not a target, and a
  dependency chain does not become faster by being given more agents. **Four
  pairs on a five-link chain produces one working pair and three waiting ones,
  plus the coordination cost of pretending otherwise.**
- **Where it applies immediately.** The geometry chain is strictly sequential:

      0022 ratify numerical model
        -> 0006 predicates spike / 0007 booleans spike close their gates
        -> 0030 ratification
        -> 0013 geometry engine
        -> 0012 spatial index, 0016 SVG export, 0032 benchmark corpus
        -> 0017 PNG export

  That is one agent's work, taken in order. It is also the chain behind the
  ADR-0002 decision, so it does not start at all until that is answered.
- **Where it does NOT apply, and why the current four stand.** The pairs live at
  the time of this decision are genuinely independent — issue 0014
  (`internal/cli`, `cmd/domowik`), issue 0028 (`testdata/corpus`,
  `internal/corpusgen`), issue 0029 (`internal/persist`), issue 0005
  (`docs/FORMAT-SPEC.md`). Different files, different owners, no ordering
  between them. Concurrency is appropriate for independent work and
  inappropriate for a chain; the test is the dependency graph, not the slot
  count.
- **The failure this prevents, which this project has already recorded once.**
  ED-0044 observed that "filling the slots by dispatching blocked or colliding
  work would manufacture activity, not progress." That was written about blocked
  work. This extends it to SEQUENTIAL work, where each agent is individually
  unblocked but only one can move at a time.
- **Decision.** Before dispatching, ask whether the candidate work is
  independent of what is already running. If the remaining work forms a chain,
  run ONE agent along it. Do not fill slots to their cap when the graph is
  linear.
- **Reason:** Records an owner correction distinguishing capacity from shape,
  and prevents a cap being read as a quota to fill.
- **Validation/reversal gate:** None — owner direction. Reversible if a chain
  turns out to admit genuine parallel sub-work, which must be shown from the
  dependency graph rather than assumed.

#### ED-0073 amendment (b) — the filtered path costs 5.5 ns and zero allocations, in this package, today

- **Date:** 2026-08-31
- **Measured on this host** (Apple M4 Max, darwin/arm64, Go 1.27,
  `GOTOOLCHAIN=local`), both from `internal/geometry/predicates`:

      BenchmarkOrientationFiltered/easy    5.479 ns/op    0 B/op      0 allocs/op
      BenchmarkIntersectSegmentsRobust    12134   ns/op  14765 B/op  426 allocs/op

  **Roughly 2,000× apart, same package, same machine, same run.** The bound
  ADR-0002's trigger is measured against is 10,000 ns/op; the filtered predicate
  is three orders of magnitude UNDER it, with no heap allocation at all.
- **What separates them is a design choice, not a runtime property.**
  `Orientation` decides the common case through `orientFast`, an adaptive
  floating-point filter, escalating to exact arithmetic only when inconclusive —
  measured at **0% escalation on non-adversarial corpora**. `IntersectSegments`
  calls `crossOfDiffs`, which constructs `big.Rat` unconditionally on every call
  regardless of input conditioning, and never calls `orientFast` at all.
- **Why this bears directly on the trigger's wording.** ADR-0002's failure
  trigger reads "robust geometry requires maintaining an unreasonable custom
  numerical stack." **That stack already exists in this repository, is
  measured, and costs 5.5 ns with zero allocations.** It is not unreasonable and
  it is not missing; it is simply not wired to the two predicates that miss the
  bound.
- **On tuning Go, since the question was asked directly.** Go offers escape
  analysis, fixed-size arrays over slices, struct field ordering, `sync.Pool`,
  `GOGC`/`GOMEMLIMIT`, and receiver reuse for `big.Int`. It does NOT offer SIMD
  intrinsics, guaranteed stack placement, custom allocators, or const generics.
  Applying all of the former to the current implementation might halve the
  allocation count. **Replacing unconditional `big.Rat` with the existing filter
  removes it**, which is the 2,000× figure above. The available win is
  algorithmic, not a matter of tuning.
- **On Rust, assessed rather than dismissed.** Rust genuinely offers no GC, SIMD
  intrinsics, precise layout control, const generics, and a mature robust-geometry
  ecosystem (each crate subject to the clean-room and third-party policies).
  But **Rust performing unconditional arbitrary-precision rational arithmetic
  would also miss the bound** — perhaps 2-3× faster, not 2,000×. A rewrite would
  pay full cost to fix a defect a filter fixes in place, while discarding the
  team-experience argument ADR-0002 rests on and re-opening the Flutter bridge
  question.
- **The caveat, stated because the inference is not a measurement.** No filtered
  `IntersectSegments` has been measured. The 5.5 ns figure is `Orientation`'s.
  The expectation that the same technique yields the same order of result on the
  same package is strong inference, NOT evidence. **The experiment that would
  settle it is hours of work:** apply the existing filter pattern to
  `IntersectSegments` and re-run `BenchmarkIntersectSegmentsRobust`. That
  converts the owner's decision from a judgement call into a measurement, and it
  is the orchestrator's recommended next step if the owner wants the ADR-0002
  question closed on evidence.
- **Still not a recommendation on the stack.** ADR-0002 is the owner's, and
  issue 0030 owns the Phase 0 performance judgment. This records what the
  hardware says.

### ED-0086: OWNER DECISION — the core moves to Rust; ADR-0002's trigger is answered

- **Date:** 2026-08-31
- **Status:** Active
- **Decided by:** project owner
- **Affected:** ADR-0002, ADR-0003, and every Phase 0/1 implementation issue
- **The decision.** ADR-0002 proposed a Go core with CLI-first delivery, subject
  to validation gates. Revision trigger 3 fired on issue 0030's Phase 0
  performance judgment. **The owner has decided: the core moves to Rust.**
- **The evidence the decision was made against, recorded so it is not
  re-litigated.** ED-0073 and its amendments established, on this host:
  - the failing benchmarks reproduce within ~5% of the retired Linux numbers, so
    the trigger is not a stale-host artifact;
  - `math/big` is 72-95% of allocated objects;
  - `Orientation`'s adaptive filter escalates 0% on non-adversarial corpora and
    90-100% on adversarial ones;
  - the two failing predicates have NO filter — `IntersectSegments` calls
    `crossOfDiffs`, which builds `big.Rat` unconditionally, and `segment.go`
    never calls `orientFast`;
  - the filtered path measures **5.479 ns/op with ZERO allocations**, against
    12,134 ns/op and 426 allocs for the unfiltered one — ~2,000× apart in the
    same package.
  The orchestrator's assessment was that this evidence pointed at an
  implementation choice rather than a language limitation, and that a filter
  applied in place would likely reach the bound. **That assessment was placed in
  front of the owner in full, including its caveat that no filtered
  `IntersectSegments` had been measured. The owner decided otherwise, which is
  the owner's call to make.**
- **Why the record says this plainly.** Not to re-argue it — the decision is
  made and execution follows. But a future reader finding a 5.5 ns filtered
  predicate in a Go tree that was abandoned deserves to know the trade was seen
  and taken deliberately, not missed.
- **What survives the change, and it is most of the session's work.** The FORMAT
  is language-independent: FORMAT-SPEC, BEHAVIOR-SPEC, ASSET-STRATEGY, the
  §9.3 limits table, the §9.4 diagnostic taxonomy, the 38-package conformance
  corpus with its machine-readable expectations, every ruling made today (F1
  zero prohibition, the `StageSchema` diagnostic placement, L3 unreachability,
  partial-history labelling), and the executive-decision record itself. **A Rust
  implementation is built against exactly these artifacts.**
- **What does not survive.** The Go implementation: `internal/document`,
  `internal/persist`, `internal/geometry`, `internal/cli`, `cmd/*`, and
  findings that are about Go's allocation behaviour rather than about the
  problem. Also the tooling written in Go — `domowik-sbom`,
  `domowik-jsoncheck`, `domowik-tracker-check` — which will need reimplementation
  or retention as build-time Go utilities.
- **Immediately actioned.** All four live pairs were wound down with
  instructions to commit only coherent state, write a final handoff, and record
  specifically what they learned that is TRUE OF THE PROBLEM rather than of the
  Go implementation. Branches survive; nothing is deleted.
- **Open question routed to the owner before migration scope can be set:**
  ADR-0002's own failure text offers two remedies — "reassess Rust for the core
  OR split only the numerical kernel behind the existing interface." These are
  materially different programmes and the answer determines everything about how
  the work is structured.
- **Reason:** Records an owner-level technology decision, the evidence it was
  taken against, and the boundary between work that survives and work that does
  not.
- **Validation/reversal gate:** Owner direction. ADR-0002 must be superseded by
  a new or amended ADR before implementation begins; ADR-0003 (Flutter GUI via
  native bridge) requires re-assessment because its bridge assumptions were
  premised on a Go core.

#### ED-0086 amendment (a) — scope is FULL CORE migration; the first work is foundation, single-threaded

- **Date:** 2026-08-31
- **Owner direction:** **full core migration.** ADR-0002's alternative remedy —
  "split only the numerical kernel behind the existing interface" — is NOT
  taken. The document model, persistence, geometry, and CLI all move to Rust.
- **All four Go pairs are wound down and purged.** Thirteen final handoffs are
  preserved at
  `~/projects/domowik-observatory/session-2026-08-31/final-handoffs/`. Branches
  survive; nothing is deleted. Main is clean, `verify.sh` exits 0, and the last
  Go work integrated was issue 0029's mutation audit — which produced four
  conformance defects that a Rust implementation would otherwise inherit
  silently (see below).
- **What the migration inherits, and it is substantial.** The FORMAT is the
  asset: `FORMAT-SPEC.md`, `BEHAVIOR-SPEC.md`, `ASSET-STRATEGY.md`, the §9.3
  limits table, the §9.4 diagnostic taxonomy with §9.5 precedence, the
  38-package conformance corpus with machine-readable expectations, and every
  ruling made today. **A Rust core is built against these, not from scratch.**
- **Four conformance defects found on the last Go audit that MUST be carried
  into the Rust work**, because three are corpus defects that would defeat any
  implementation:
  1. **L12** — `hostile/json/number-token-over-L12.json` holds a token of
     EXACTLY 64 characters while the guard tests `> 64`. The fixture is on the
     boundary, not over it.
  2. **L19** — the `uuid-*`/`digest-*` fixtures test field FORMAT, never
     OVERLENGTH.
  3. **L4** — no fixture exercises the guard at all.
  4. **L10** — container nesting depth has NO GUARD. `CatLimitNestingDepth` is
     used only for JSON nesting (L11); the two share a category while only one
     is enforced.
- **Sequencing, per ED-0085.** Migration bootstrap is strictly sequential —
  nothing can be ported until the workspace, toolchain, verification contract
  and third-party posture exist. **It runs SINGLE-THREADED.** Parallel pairs
  resume only if and when independent port targets exist, shown from the
  dependency graph rather than assumed.
- **Gates that bind before implementation begins.**
  1. **ADR-0002 must be superseded**, not merely contradicted. A new ADR records
     the Rust core decision, its evidence, and its own validation gates.
  2. **ADR-0003 must be re-assessed.** Its Flutter-bridge reasoning was premised
     on a Go core; the FFI story changes materially.
  3. **`docs/THIRD-PARTY-POLICY.md` applies to every crate.** Rust's geometry
     ecosystem is the stated advantage, and each dependency needs licence,
     provenance and notice review before use. The clean-room policy applies
     unchanged.
  4. **The verification contract must be rebuilt.** `scripts/verify.sh` is
     Go-specific, as are `domowik-sbom`, `domowik-jsoncheck` and
     `domowik-tracker-check`. Decide per tool: reimplement, or retain as
     build-time Go utilities.
- **Reason:** Records the scope answer, the inheritance boundary, and the
  foundation gates, so the first migration agent has a decidable starting point.

#### ED-0085 amendment (a) — "single-threaded" means one ISSUE, not one agent; the pair stays

- **Date:** 2026-08-31
- **Owner confirmation.** ED-0085 directed that strictly sequential work runs
  single-threaded. Asked whether that meant collapsing the executor/reviewer
  pair into a single agent, the owner confirmed: **keep the pair.**
- **The distinction, so it is not re-derived.** Single-threading is about
  CONCURRENT ISSUES, not about the review split. A worker and an adversarial
  reviewer on one issue are ONE line of work — the reviewer is not parallel
  throughput, it is the quality gate. Two pairs on two independent issues is
  concurrency; one pair on one issue is not, however many agents it contains.
- **Why the gate is worth its cost, from today's record.** Adversarial review
  caught: a threat-model guard that no test protected (`zipread.go:215`, ED-0076);
  a mutation transcript quoting a log line absent from the submitted file
  (ED-0068); a normative inversion in an orchestrator-written brief within
  twenty minutes of dispatch (ED-0067); and a pair's own documentation
  contradicting its verified work (ED-0076 amendment (a)). Removing the reviewer
  to satisfy a sequencing rule would have traded a real quality mechanism for a
  scheduling preference.
- **Standing form of the rule.** One issue at a time when the work is a chain;
  a worker and a reviewer on that issue; add a second pair only when the
  dependency graph shows genuinely independent work.

### ED-0087: Three foundation rulings — Go tree retention, Go tool disposition, and vacuous Rust stages

- **Date:** 2026-08-31
- **Context.** Issue 0037's plan left three questions open for the pair to
  litigate: whether the Go tree keeps verifying during migration, whether the
  three Go build tools get reimplemented in Rust now, and what a Rust
  verification stage proves over an empty workspace. All three are orchestrator
  scope, not pair scope, and litigating them would have burned a round.

- **R1 — The Go tree stays verifying until Rust reaches parity.** `scripts/verify.sh`
  keeps every Go stage and GAINS Rust stages. Main must never be broken
  mid-migration. Retirement of the Go tree is a SEPARATE later decision, taken
  when parity is demonstrated, not assumed by the migration's existence.
  Rationale: "full core migration" (ED-0086a) names the destination, not a
  licence to break the only currently-working implementation on the way there.

- **R2 — Do not reimplement the Go build tools during foundation.**
  `domowik-sbom` is load-bearing for release evidence, the committed SPDX
  documents and `NOTICE`; `verify.sh` runs both `sbom verify` and
  `check-manifest`. Reimplementing it during foundation risks the evidence
  chain for zero foundation benefit. All three tools (`domowik-sbom`,
  `domowik-jsoncheck`, `domowik-tracker-check`) are retained as Go build
  utilities. Their reimplementation is a separate issue, opened later. A build
  tool is not core; ED-0086a's "full core" does not reach it.

- **R3 — A Rust stage over an empty workspace is a check that cannot fail.**
  `cargo test --workspace` passing with zero tests is the ED-0041 failure mode
  wearing a green tick. This is the single most likely way a language-migration
  foundation ships a lie, because every stage is genuinely new and genuinely
  passing while proving nothing. Either the foundation lands at least one real
  compiled, tested unit per crate, or each vacuous stage is declared IN WRITING
  as not-yet-load-bearing. An unlabelled vacuous stage is prohibited.

- **Standing form.** A migration foundation's stages must be interrogated for
  vacuity BEFORE they accumulate trust. A stage that has never failed because
  nothing has ever exercised it is indistinguishable, in a transcript, from a
  stage that guards something.

### ED-0088: A peer's pane is not a delivered message — an agent misattributed its own file

- **Date:** 2026-08-31
- **What happened.** w0037 wrote two plan files four minutes apart
  (`build/w0037-plan.md`, then `build/w0037-foundation-plan.md`), sent r0037 a
  note naming the first, then read r0037's PANE and concluded that r0037 "has
  already expanded my plan into a more detailed foundation plan." It had not.
  w0037 was reading its own second file back, through a truncated JSON pane
  dump interleaved with UI chrome, and attributed it to its peer.
- **Simultaneously, in the other direction.** r0037's challenge existed ONLY as
  pane text. No file carried it; `find build -name "*r0037*"` returned nothing
  but w0037's outbound note. Each agent believed it had communicated. Neither
  had.
- **Why the pane is not a channel.** `herdr agent read` is an ORCHESTRATOR
  instrument. It truncates mid-token, interleaves the recipient's UI chrome
  with content, and carries no authorship. It cannot distinguish a peer's
  output from a quotation of your own. Reading it as a message is not a
  degraded channel; it is a channel that fabricates attribution.
- **Ruling.** Inter-agent exchange is hsend.sh plus a file path (ED-0071), and
  nothing else. **If an agent has not received a path, it has not received a
  message** — it must wait, never infer from a pane. Both halves of this rule
  bind: write the artifact to a file, AND send the path.
- **Detection value.** This was caught in under four minutes by reading the
  FILESYSTEM rather than the panes — timestamps and authorship footers settled
  in one command what two panes had already gotten wrong. Ground truth for
  "who produced what" is on disk, not in a terminal.

### ED-0089: A tautological test named after a spec row is a conformance badge on a defect

- **Date:** 2026-08-31
- **How it surfaced.** ED-0087 R3 prohibited unlabelled vacuous stages in the
  Rust foundation. Fifteen minutes later, commit `0d41a40` on
  `issue/0037-rust-foundation` landed five crates each with "at least one
  substantive test (per R3)". All five were tautological: each asserted that a
  literal equals the same literal written a few lines above it. None called a
  function under test. `operation_ordering` asserted `1 < 2` — delete every
  line of Domowik code in that crate and it still passes.
- **The defect this concealed.** `domowik-container` hardcoded
  `max_entry_count: 65536` and asserted it in a test named
  `limits_default_matches_l6`. The ratified value is **65,535**
  (`FORMAT-SPEC.md:1618`, `internal/persist/limits.go:26`), because the legacy
  ZIP end-of-central-directory entry field is 16-bit. The row states the
  boundary explicitly: "> N rejected (== 65,535 succeeds)". **65,536 is
  precisely the first value a conformant reader must reject**, and the Rust
  default would have admitted it.
- **Why the name is the aggravating factor.** A wrong constant with no test is
  an ordinary bug, found by the next person who checks. A wrong constant with a
  PASSING test named after the specification row it violates is worse: the next
  person greps `L6`, finds green, and stops looking. The test does not merely
  fail to detect the defect — it certifies it. This is ED-0072 (presence is not
  efficacy) sharpened: a check can be worse than absent when its name asserts
  a conformance it never tested.
- **Ruling.** A test that names a specification row must DERIVE FROM or CITE
  that row, never restate its value as a second literal. Restating a literal
  tests transcription, and it tests it against the same transcription.
- **Ruling on labels.** ED-0087 R3 offered two acceptable outcomes: land a real
  tested unit, or declare the scaffold not-yet-load-bearing in writing. A third
  path was taken — vacuous tests LABELLED substantive — and it is the one
  outcome R3 exists to prevent, because the label defeats the next reader's
  scrutiny. Claiming the second option is the first is a contract breach, not a
  degree of compliance.
- **Corroborating finding.** The same commit series' own `verify.sh` exited 1
  at `GO_SBOM_VERIFY` on the HEAD whose subject read "refresh sboms after rust
  workspace addition". The release-inventory verifier discovers components
  dynamically and audits fail-closed against the reviewed registry; five new
  crates are five new components. The fail-closed audit worked exactly as
  designed. Recorded because a pair reporting progress on a tree whose own
  verification fails is the ED-0077 pattern, and this time the stage NAMED
  itself — the remedy committed thirty minutes earlier did its job on its
  first real failure.

### ED-0090: OWNER DECISION — model policy; LiteLLM revoked

- **Date:** 2026-08-30
- **Owner decision, stated directly.** "You can only use Claude Code with Opus 5
  for the main work, and Codex for verification. No more using LiteLLM."
  - Primary executor / main work: **Claude Code on Opus 5.**
  - Adversarial reviewer / verification: **Codex.**
  - **LiteLLM: REVOKED.** No agent may be started on a `litellm/*` model.
- **What prompted it.** The owner found both agents on issue 0037 running
  `litellm/gemini-3.1-pro-preview` and stopped the work. The orchestrator had
  selected that provider while optimising for reviewer capacity and cost
  (the ED-0060/ED-0061 line of reasoning), carried the habit into the Rust
  migration dispatch, and never surfaced the choice for approval.
- **The standing rule this establishes.** Model selection for delegated work is
  an OWNER decision, not an orchestrator optimisation. The orchestrator may
  reason about capacity and cost, but may not settle provider or model on those
  grounds. Every dispatch record must state the model for each seat. If a
  required provider is unavailable, the orchestrator SAYS SO AND ASKS; it does
  not substitute.
- **Superseded.** `INSTRUCTIONS.md` previously specified OpenCode/Ox Alpha for
  the executor seat and Codex/GPT-5.6 Luna for the reviewer. Both are replaced
  by this record; the file has been updated in place and names this decision.
- **Known availability constraint, flagged not resolved.** Codex exhausted its
  credits at approximately 17:00 on 2026-08-30. Until that is topped up the
  verification seat cannot be filled. Per the rule above this is surfaced to
  the owner rather than worked around by substituting a provider.
- **State at the stop.** Main clean at `313f542`, `verify.sh` exit 0, nothing
  pushed. Branch `issue/0037-rust-foundation` at `83ee0e9` holds the cargo
  workspace and verify rewrite, carrying the seven blockers of ED-0089 and the
  orchestrator findings. Its uncommitted ADR 0008 draft (90 lines, untracked
  and therefore at risk) was copied to
  `domowik-observatory/session-2026-08-31/preserved-0037/` before any teardown.

#### ED-0090 amendment (a) — reviewer seat is Claude Code on Sonnet 5

- **Date:** 2026-08-30
- **Owner direction.** "restart the pair with Opus 5 and Claude Code with
  Sonnet 5 for a reviewer." The verification seat named in ED-0090 (Codex) is
  replaced by **Claude Code on Sonnet 5**; the executor seat is unchanged at
  **Claude Code on Opus 5**. This resolves the Codex credit constraint ED-0090
  flagged — resolved by the OWNER choosing a different seat, which is the
  rule ED-0090 established working as intended, rather than the orchestrator
  substituting a provider on its own judgement.
- **Dispatch actually made.** Workspace `w4G`, both agents in the
  `issue-0037` worktree:
  - `w0037` at `w4G:p2` — `claude --model opus --permission-mode acceptEdits`
  - `r0037` at `w4G:p3` — `claude --model sonnet --permission-mode acceptEdits`
  The prior LiteLLM/Gemini panes (`w4F:p2`, `w4F:p3`) were closed first.
- **Permission mode, surfaced rather than assumed.** `acceptEdits` lets the
  pair edit files in their isolated worktree without prompting, but still
  prompts for other capability. A stricter mode stalls the pair on every
  command; a broader one (`bypassPermissions`) is a safety reduction that is
  the owner's to grant, not the orchestrator's to assume. Chosen conservatively
  and reported.
- **Names reused with no suffix**, per INSTRUCTIONS.md item 9.

### ED-0091: OWNER DECISION — full Rust rewrite, Go frozen as reference, single driver

- **Date:** 2026-08-30
- **Owner direction, in their words.** "I want you to focus on a performant code,
  fully written in Rust. Keep the Golang code in some separate dir for
  references but we should aim at a full Rust re-write. You're a team of one and
  you make sure that everything is working as needed! Performance is the key!"
  Earlier in the same exchange: "stop those agents! I want you to be the single
  driver of the required work!"
- **Three things this settles.**
  1. **Full rewrite, not a migration with a co-maintained Go tree.** This
     SUPERSEDES ED-0087 R1, which held that the Go tree keeps verifying on main
     until Rust reaches parity.
  2. **No delegation.** The executor/reviewer pair model is stopped. ED-0085a
     (keep the pair) is superseded by the owner's later direction; the quality
     mechanisms the pair provided — adversarial review, mutation proofs,
     verifying claims by execution — do NOT lapse. They become the single
     driver's obligation to itself.
  3. **Performance is a requirement, not an aspiration**, and must therefore be
     enforced by the build rather than asserted in prose.
- **How the freeze was done, and the one non-obvious part.** The Go core moved
  to `_reference/go/` with `git mv` (164 renames, history intact). The leading
  underscore is LOAD-BEARING: the Go toolchain ignores `_`- and `.`-prefixed
  path elements, which is what removes ~32k lines from `go build ./...` and
  `gofmt -l .` without deleting a line. Renaming that directory would silently
  pull the frozen tree back into every verification run.
- **The release machinery froze with its product, and why that was the honest
  option.** `internal/release`, `cmd/domowik-sbom` and `release/` describe two
  linux binaries built from `./cmd/domowik`, and the manifest enforces
  `artifacts must not be empty` as a fail-closed guard. With no Go artifact left
  to describe, the choices were: point the manifest at a build tool (a lie),
  relax the empty-set guard (weakening a fail-closed check of exactly the kind
  this record has repeatedly defended), or freeze the machinery together with
  its subject. The third was taken.
- **The obligation outlived the tooling.** Apache-2.0 distribution and
  `docs/THIRD-PARTY-POLICY.md` still bind, and the Rust workspace ADDED a
  dependency surface, so compliance grew rather than lapsed.
  `scripts/rust-deps-inventory.py` is the interim guard: regenerated and diffed
  every run, so a new dependency fails the build until recorded. Its limits are
  stated in the script and the evidence — declared licences only, no LICENSE
  reading, no `NOTICE`. 47 crates, 11 runtime, all permissive.
- **Withdrawn, not retired.** CLI stream/exit-code probes P1-P3, artifact
  inspection, and the Go cross-compiles all targeted the frozen binary. They are
  recorded in place in `scripts/verify.sh` and `.github/workflows/ci.yml` as
  obligations of the CLI port, per ED-0079: unreachable is not deletion.
- **Evidence.** Five gates mutation-proven at
  `docs/evidence/rust-foundation/MUTATION-PROOFS.md`. Two captured cargo's real
  exit status **101** rather than a flattened `1` — the concrete reason the
  abandoned branch's `run_stage` wrapper was not carried forward, since the
  withdrawn probes assert exit-code **2** semantics that a flattening harness
  could not distinguish.
- **State.** Merged to main and verified independently on the MERGED tree:
  exit 0, `=== VERIFICATION PASSED ===`, clean tree. Nothing pushed.

### ED-0092: A provisional numerical tolerance names its gate in the source, or it becomes permanent by silence

- **Date.** 2026-08-31
- **Context.** BEHAVIOR-SPEC R3.3.4 requires every member of a join to have
  equal endpoint coordinates "under the ratified numerical policy". That policy
  is not ratified: issues 0022 and 0030 own it and both are open. The join
  entity, its validation, and the CLI's auto-join all need to answer "are these
  two endpoints the same point?" before that ratification exists, and refusing
  to answer means no joins at all.
- **Decision.** Introduce `domowik_format::JOIN_COINCIDENCE_MM = 0.001` (one
  micrometre) as an explicitly PROVISIONAL constant, matching the weld
  tolerance `domowik_geometry::arrangement::WELD_MM` that the planar
  arrangement already uses. Both are documented in their own source as
  provisional, both name issues 0022/0030 as the gate, and they are required to
  move together.
- **Reason.** One micrometre is far below any building dimension a user can
  express and far above millimetre-scale floating-point noise, so it decides
  every case that occurs in practice and cannot be the reason a real plan is
  wrong. The choice is bounded and reversible: it changes no persisted data —
  joins store no coordinates (R3.3.1) — and ratification can tighten or loosen
  it without a migration.
- **The rule this records, which is the part worth keeping.** A provisional
  constant that does not name its ratification gate AT THE DEFINITION is
  indistinguishable from a settled one to the next reader. `COORD_BOUND_MM` was
  introduced the same way and carries the same note. Two constants now answer
  the same question in two crates; the note in each is what stops them drifting
  apart, and what will make them findable when 0030 closes.
- **Affected issues.** 0013 (this work), 0022 and 0030 (the gate), 0010, 0012.
- **Reversal gate.** Ratification of the numerical model under issue 0030. At
  that point both constants are re-derived from the ratified policy or deleted
  in favour of it, and this record is superseded rather than edited.

### ED-0093: Adopt tiny-skia for PNG rasterisation; five permissive licences reviewed

- **Date.** 2026-08-31
- **Context.** Issue 0017 requires PNG export "with a selected, pinned,
  license-compatible renderer" and puts "a new internal anti-aliased renderer"
  out of scope unless separately justified. Nothing about anti-aliased path
  filling is Domowik-specific, so there was no justification to write one.
- **Decision.** Adopt `tiny-skia` 0.12 with `default-features = false` and
  `std`, `simd`, `png-format`. One direct dependency: `png-format` pulls the
  `png` encoder, so no second direct entry is needed.
- **Licences reviewed and added to the allow-list**, all permissive and all
  named in THIRD-PARTY-POLICY's PREFERRED set, so none requires an ADR:

  | Crate | Licence | Taken under |
  |---|---|---|
  | `tiny-skia`, `tiny-skia-path` | BSD-3-Clause | BSD-3-Clause |
  | `arrayref` | BSD-2-Clause | BSD-2-Clause |
  | `zlib-rs` | Zlib | Zlib |
  | `bytemuck` | Zlib OR Apache-2.0 OR MIT | Apache-2.0 |

- **The obligation this creates.** BSD-2-Clause and BSD-3-Clause require the
  copyright notice and disclaimer to be reproduced in distributions, and
  BSD-3-Clause additionally forbids using contributor names to endorse. Zlib
  requires that altered sources be marked and the notice not be removed.
  Domowik alters none of them. A `NOTICE` file discharging this does not exist
  yet — the Go release machinery that built one froze with its product
  (ED-0091), and a Rust replacement is owed. **This decision does not discharge
  that obligation; it adds to it.**
- **Provenance, for the clean-room record.** `tiny-skia` is an independent Rust
  port of a subset of Skia (Google, BSD-3-Clause). Skia is a general 2D
  graphics library with no relationship to any floor-plan application, so
  nothing here touches CLEAN-ROOM-POLICY's subject matter.
- **Why the guard caught this and that is the point.** Adding the dependency
  made `scripts/verify.sh` FAIL at the `rust dependency inventory` stage,
  naming all five licences, and it stayed failing until they were reviewed and
  recorded. That is the fail-closed behaviour ED-0091 kept when the Go release
  verifier was frozen, working as intended on its first real test.
- **Affected issues.** 0017 (this work), 0036 (the owed `NOTICE`).
- **Reversal gate.** None needed: the dependency is confined to
  `crates/domowik-cli`'s PNG module and removing it removes one command.

### ED-0094: The NOTICE obligation is discharged by generation, and enforced fail-closed

- **Date.** 2026-09-01
- **Context.** Domowik ships under Apache-2.0, and
  `docs/THIRD-PARTY-POLICY.md` requires every DISTRIBUTED dependency to be
  recorded with its notices. Several runtime crates carry licences that
  REQUIRE reproduction: BSD-2-Clause and BSD-3-Clause both say the copyright
  notice and disclaimer must appear in distributions, and Apache-2.0 §4(d)
  requires carrying forward any NOTICE file. **No `NOTICE` existed.** The Go
  release machinery that built one froze with its product (ED-0091), and
  ED-0093 recorded that adopting `tiny-skia` ADDED to the obligation without
  discharging it — twice stating that Domowik must not ship a binary first.
- **Decision.** Generate `NOTICE` from the dependency graph with
  `scripts/rust-notice.py`, reproducing each distributed crate's own licence
  texts verbatim from its source, and enforce it in `scripts/verify.sh` by
  regenerating and diffing — the same fail-closed shape as the dependency
  inventory. 83 distributed components.
- **Why generated rather than written.** A hand-maintained notice goes stale
  the first time a dependency changes, and nothing about it fails loudly when
  it does. Regenerate-and-diff makes a dependency change break the build until
  the notice catches up, which is the only mechanism that has held for the
  inventory.
- **Scope, stated because the file is itself an assertion.** RUNTIME crates
  only — dev tooling is not distributed and its notices are not obligations,
  though `docs/RUST-DEPENDENCIES.md` still records it. Texts over 40 KB are
  named rather than inlined, because a notice nobody opens serves nobody. One
  crate (`r-efi`) ships no licence text at all and is listed as such: "we could
  not find the text" is a fact a reader needs, not one to paper over.
- **What this is NOT.** Not a licence audit and not legal advice. It reports
  what the crates declare and ship. It also does not cover the Go tooling that
  survives as repo hygiene, which is not distributed.
- **Affected issues.** 0017 and 0036 (the obligation), 0018 (release).
- **Reversal gate.** None. If a future dependency's licence cannot be
  reproduced this way, that dependency needs an ADR before adoption, which the
  inventory's allow-list already forces.

### ED-0095: Phase 1 closes; "In Progress" stops meaning "has been touched"

- **Date.** 2026-09-01
- **Context.** Twenty-two issues carried `In Progress` simultaneously. A
  project review put it plainly: that "is not a credible active-work state; it
  hides the critical path." It was accurate as a record of what had been WORKED
  ON, and useless as a record of what is BEING worked on — which is the only
  thing a status column is for.
- **Decision.** Phase 1 closes, and every issue takes a status that describes
  its actual state:
  - **Done** — acceptance criteria met. Residual dependencies on future
    SPECIFICATION fixation do not hold an implementation issue open; they are
    recorded against the spec issue that owns them.
  - **In Progress (partial scope)** — material delivered, with named gaps
    stated in the issue.
  - **Blocked** — waiting on an owner decision or on an unstarted phase. This
    status was available and unused, which is why the In-Progress set grew.
  - **Open** — not started.
- **The ED-0018 gates are re-scoped, deliberately.** ED-0018 forbade Done on
  several implementation issues while issues 0004 and 0010 remained unclosed.
  Those gates were written when the specifications were drafts. FORMAT-SPEC §4
  and §2 are now normative for v0.1 within their stated scope, and the residual
  work on 0004/0005 is parameter FIXATION, not semantics. Holding finished
  implementations open on it buys nothing and costs the ability to see what is
  actually in flight.
- **What this decision does NOT do.** It does not mark anything Done whose
  criteria are unmet. Every issue closing here carries a section stating what
  it did not deliver, and several close as `In Progress (partial scope)`
  precisely because their criteria are only partly met. Phase 1 is closed as
  "the engine and the CLI work, and here is exactly where they stop".
- **Affected issues.** All Phase 0 and Phase 1 issues.
- **Reversal gate.** None needed. If a re-scoped gate turns out to have been
  load-bearing, the issue reopens — which is cheaper than the state this
  replaces.
- **AMENDED THE SAME DAY by [ED-0096](#ed-0096-ed-0095-amended--one-of-the-gates-it-re-scoped-was-load-bearing).**
  One gate WAS load-bearing, and the reversal clause above was taken up
  within hours: an adversarial audit of this decision found two Done marks
  unearned and three defects in shipped code behind them. Read ED-0096
  before relying on anything here.

### ED-0096: ED-0095 amended — one of the gates it re-scoped WAS load-bearing

- **Date.** 2026-09-01
- **Status.** Amends [ED-0095](#ed-0095-phase-1-closes-in-progress-stops-meaning-has-been-touched), which stands as amended.
- **Context.** ED-0095 was written, the tracker was restated, and the closure
  was then audited adversarially (Codex, gpt-5.6-sol, high effort) against
  each issue's verbatim acceptance criteria. The audit found that two of the
  six Done marks rested on unmet criteria, that three more issues were Done or
  Blocked on descriptions that did not survive contact with the code, and that
  ED-0095's central claim was too broad.
- **The claim that was too broad.** ED-0095 said the residual work on issues
  0004 and 0005 is "parameter FIXATION, not semantics". That is true of the
  container and framing parameters. It is **false of the geometry constants**,
  and ED-0018's reversal gate — "if any 0030 outcome changes a named
  parameter … affected code and tests must be revised before closure" — was
  protecting exactly that:
  - `COORD_BOUND_MM` is **1,000,000 mm** in
    `crates/domowik-format/src/lib.rs`, while NUMERICAL-MODEL §3's proposed
    bound is **10⁹ mm**, with 2³¹−1 mm also under consideration. Three
    candidate values, a thousandfold apart. Documents this build REJECTS today
    become valid under the larger bound, which is a format-observable
    difference, not a tuning knob.
  - `WELD_MM` and `JOIN_COINCIDENCE_MM` decide vertex identity, and therefore
    join validity, union topology, room detection and every export downstream
    of them.
  - `lwwHorizon` decides what §5.1 S1/S3 metadata a snapshot must retain, so
    it is not separable from whether `compact` conforms.
- **What this changes, and what it does not.** Phase 1 still closes: the
  engine and the CLI work, and the closure record says where they stop. What
  changes is the honesty of the reasoning. **A gate is not re-scoped by
  asserting that what it protects is unimportant.** The correct form is the
  one ED-0092 already established for provisional constants: name the gate in
  the source, so silence cannot ratify it. Every constant above does. So the
  protection ED-0018 provided is not lost by closing the implementation
  issues — it is carried by the constants themselves and by issues 0022 and
  0030, which remain open and own the decision.
- **The Done marks that were withdrawn or earned.** Issues 0015 and 0037 had
  their unmet criteria **implemented** rather than argued away. Issues 0010,
  0011, 0016 and 0027 were **downgraded** to `In Progress (partial scope)`
  with their gaps named. Issue 0036 is restored from `done/`, because a
  release process describing the frozen Go product does not discharge criteria
  about "every distributed binary artifact" of a Rust one.
- **Three defects in shipped code were found behind the paperwork**, which is
  the argument for auditing a closure rather than announcing one. Each is
  fixed with a mutation-proven regression test:
  1. `Session::accept_external` never validated. `domowik apply` accepted a
     batch creating a zero-length wall of negative thickness and SAVED it,
     producing a file the same binary's `validate` then rejected. §4.3 row 1
     says a user-input validation failure rejects the whole batch "BEFORE any
     effect", and says nothing about the batch being local.
  2. `causalPastSize` accumulated in `u64`. §9.3 row L33 is Ratified and
     explicit: a conforming reader compares every value in [0, 2⁷⁹ − 2¹⁶]
     exactly, "wrapping and saturating arithmetic are FORBIDDEN", and "a
     64-bit accumulator cannot provide it". A hostile package within L13 and
     L18 could reverse canonical order.
  3. `Session::open_snapshot` dropped the retained tombstones. `Package::
     project` always applied them, so a session opened on a compacted plan
     resurrected entities the file did not contain and the two views of one
     plan disagreed until the next reopen (I3).
- **Reversal gate.** None. This decision only makes the record match the code.

### ED-0097: Activate issue 0024 against the Rust boundary; the Go slice becomes history

- **Date.** 2026-09-01
- **Context.** Phase 1 is closed and the session-cost defect that would have
  invalidated any interactive measurement is fixed (issue 0038). ADR 0003 is
  the only gate between here and a GUI, and it has never been tested against a
  Rust core.
- **Decision.** Issue 0024 is reactivated, renamed, and re-scoped to the Rust
  FFI. Its Go-era acceptance criteria are superseded rather than deleted: they
  are kept verbatim in the issue because they are the record of what was
  actually validated in August, and because several of them name obligations
  (no-Go-pointer rules, runtime shutdown) that are meaningless against Rust and
  would otherwise look like unfinished work.
- **What carries over from the Go slice, and what does not.** The FLUTTER-side
  findings survive the core's language: dart:ffi `DynamicLibrary.open`,
  callback delivery through `NativeCallable.listener`, bundle packaging. The
  Go-runtime findings do not: F3 (no shutdown story for c-shared) is simply
  absent for Rust, and F1 (superlinear Execute cost) has been fixed in the
  core rather than worked around at the boundary.
- **The spike is not a product component.** It lives under `spikes/` in its own
  Cargo workspace with path dependencies on the core crates, excluded from the
  root workspace, and nothing in it is distributed. This mirrors the Go spike's
  nested-module structure for the same reason: a spike that sits in the
  distributed build graph acquires a compatibility obligation it was never
  meant to carry.
- **It is still verified.** `scripts/verify.sh` builds and tests it, because
  the alternative is a spike that rots into a false record of what once worked.
- **Reversal gate.** If the spike fails, ADR 0003 needs a new decision, not a
  quiet platform-specific divergence — the ADR's own failure-response clause.

### ED-0098: Interaction thresholds are fixed, and whole-level recomputation is recorded as a measured R3.4.8 violation

- **Date.** 2026-09-02
- **Context.** Issue 0032 was the last `Open` Phase 1 issue and the only one
  standing between the core and a defensible performance claim. BEHAVIOR-SPEC
  §1.7 fixes qualitative scopes only; R3.4.8 and R5.1.9 define a recomputation
  closure but explicitly delegate every number to issue 0032.
- **Decision 1 — the thresholds.** Gesture latency p99 ≤ 8 ms at every corpus
  scale on the named reference host, derived as half of one 60 Hz frame with
  the other half left to rendering (issue 0033). Peak allocation ≤ 4 MiB above
  resident per gesture. Regression tripwire at 1.5× the recorded p99, chosen
  because the worst p50 spread across three runs was 17.2%, so it cannot fire
  on noise. Recorded in [docs/BENCHMARK-INTERACTION.md](docs/BENCHMARK-INTERACTION.md).
- **Decision 2 — the thresholds are NOT gated in CI, and that is stated rather
  than hidden.** They are calibrated to one host; CI is not that host.
  `verify.sh` gates that the harness RUNS, that every corpus and every gesture
  class is still exercised, and that the locality checks hold. A pinned
  reference runner is owed and recorded against issue 0032.
- **Decision 3 — the locality gap is recorded as a defect against issue 0013,
  not absorbed into 0032.** `walls::plan` is the only derived-geometry entry
  point and it takes a whole level, so every edit rebuilds every component.
  On the multi-cluster corpora that exceeds the closure R3.4.8 permits: on
  `campus-xl`, creating a wall in empty space is permitted 1 wall and 0 faces
  and recomputes 330 walls and 84 faces. Scoped recomputation is an acceptance
  criterion of issue 0013; 0032 owns the measurement and now supplies it.
- **The gap is guarded by a CANARY, not by a comment.**
  `crates/domowik-bench/tests/locality.rs` asserts the violation still EXISTS
  and fails the day it is fixed. A test that merely tolerated the defect would
  pass in both worlds and tell nobody when the work landed — the same failure
  the 0024 review found in evidence that would survive its own bug.
- **What the measurement actually says.** The gesture is not the cost. On
  `wings-l`, fourteen of fifteen classes fall within 1.62–1.68 ms p50 against
  a 1.53 ms whole-level rebuild; on `campus-xl`, all fifteen fall within
  3.70–3.76 ms against 2.57 ms. Committing a batch is under 10% of what a user
  waits for. Optimisation effort belongs in recomputation scope, not in the
  command path.
- **A finding worth stating because it is counter-intuitive.** R5.1.9 locality
  is VACUOUS on a normally drawn floor plan: every partition meets the shell
  at a shared node, so the level is one join-connected component and the
  closure is the whole level. Locality constrains multi-cluster levels only.
  Scoping recomputation will not, by itself, make a single-component flat
  cheaper; that needs incremental union along the touched boundary, which is a
  different problem and is not what R5.1.9 asks for.
- **Reversal gate.** If the 8 ms budget proves wrong once issue 0033 sets a
  real render budget, this decision is revised there and the split restated —
  not quietly renegotiated in the benchmark document.

### ED-0099: macOS is a local-development target, not a distribution target

- **Date.** 2026-09-04
- **Context.** The issue-0024 Flutter application validated macOS bundle
  packaging and recorded four distribution gaps as unvalidated: notarisation,
  Gatekeeper acceptance, hardened runtime, and archive/DMG/pkg shape. Three of
  those cannot be closed on this machine at all — `security find-identity -v
  -p codesigning` reports zero valid identities, so there is no Developer ID
  to sign with and notarisation has no input.
- **Decision (owner, 2026-09-04).** macOS is used for **local development
  only**. It is not a distribution target and is not one of the Phase 2
  platforms ADR 0003's gate names.
- **What this closes.** Notarisation, Gatekeeper acceptance, hardened runtime,
  and installer/archive packaging move from *owed* to *out of scope for
  macOS*. They are no longer residuals of issue 0024 and must not be reported
  as such. Ad-hoc signing is sufficient for a developer running the app on the
  machine that built it.
- **What this does NOT close.** The obligations still exist for whichever
  platforms Domowik does distribute on; they are simply not macOS's to carry.
  Everything else recorded against the macOS slice stands: the x86_64 slice was
  built but never executed, no clean-machine install was tested, and an
  incremental Flutter build leaves the outer bundle signature invalid.
- **Why this is recorded rather than assumed.** A gap that is dropped without a
  decision looks identical to a gap that was forgotten. The next reader of
  issue 0024 would otherwise see "notarisation unvalidated" and either redo the
  work or treat the spike as unfinished.
- **Reversal gate.** If macOS ever becomes a distribution target, this decision
  is revisited and the four items return as real obligations — starting with a
  Developer ID, which is an owner purchase, not an engineering task.

### ED-0100: The numeric bounds are ratified, and two of the arguments for them were wrong

- **Date.** 2026-09-04
- **Authority.** Owner delegation, 2026-09-04: *"work with codex … to get good
  defaults. Whatever you two recommend I approve."* The defaults below were
  proposed by Claude (Opus 5), attacked by Codex (gpt-5.6-sol, high effort),
  and changed as a result. Both disagreements are recorded because the
  reasoning matters more than the numbers.
- **Ratified values.**

  | Symbol | Value | Was |
  |---|---|---|
  | `coordBoundMm` | **10⁷ mm** (±10 km) | 10⁶ enforced, 10⁹ proposed |
  | `dimensionCapMm` | **10⁶ mm** | 10⁶ (unchanged) |
  | `derivedCoordBoundMm` | **1.1×10⁷ mm** | undefined in code |
  | topology quantum (`WELD_MM`, `JOIN_COINCIDENCE_MM`) | **10⁻³ mm** | 10⁻³ provisional |

  Coordinates are PLAN-LOCAL; no CRS or georeferencing transform exists in
  v0.1. This closes FORMAT-SPEC row L14 and the ED-0092 provisional-constant
  gate.

- **Compatibility.** v0.1 is unreleased, so no released contract breaks. The
  coordinate bound ROSE, which only widens the set of accepted documents; the
  dimension cap is unchanged. Ratifying before release is the point — after
  release, raising a bound orphans old readers and lowering one invalidates
  existing files, and neither is free.

- **I was wrong about the coordinate bound, and the measurement says so.** I
  proposed 10⁶ mm on the grounds that a distant polygon's area is computed as a
  difference of huge nearly-equal products, quoting a worst-case error of
  71 cm² at 10⁹ against 0.007 mm² at 10⁶. Codex's answer was that this measures
  the AREA ROUTINE, not the domain, and it was right: with
  `signed_double_area` translated to a local origin, the measured worst-case
  error for a room-sized rectangle is 3.7×10⁻⁹ mm² and is **identical at 10⁶,
  10⁷ and 10⁹**. With the numerical argument gone the bound is a product
  question, and ±1 km excludes a campus, an airport apron or a long site plan.
  10⁷ was Codex's number.

- **Two correctness defects were found while arguing about the defaults, and
  both are in shipped code.**
  1. **`orientation` was not exact, while documenting that it was.** For
     `(0,0), (η,0), (0,η)` with η the smallest subnormal, both products
     underflow to zero and the sign shortcut returned Collinear for a triangle
     with real positive area. The exact fallback could not save it either:
     Dekker's product splitting is only exact while nothing underflows. Fixed
     by detecting a product that is zero when neither operand was, and by
     scaling the differences by a power of two — the determinant is homogeneous
     of degree two, so this cannot change its sign — before the exact
     evaluation. A genuine zero product, which is every axis-aligned wall,
     still takes the fast path.
  2. **A real feature could have exactly zero area.** A 2 µm square at the old
     coordinate bound evaluated to zero, so `canonicalize` discarded a ring
     whose sides are twice the topology quantum. Fixed by translating to a
     local origin.

  Both have regression tests that fail against the previous implementation.

- **What is NOT ratified, stated so it cannot be assumed.** Quantization of
  stored coordinates onto the topology grid; a single "same point" rule to
  replace the three that currently disagree; split-point persistence; the exact
  area and half-even display contract; and the positive-area threshold in
  `geometry::locality`, which is format-observable semantics rather than an
  internal epsilon. NUMERICAL-MODEL §12 carries all five.

- **NUMERICAL-MODEL §§4.2–4.5, 6, 10 and 11 are superseded, not pending.** They
  describe certificate and big-integer machinery for a Go core that no longer
  exists. Retaining them as open decisions would have meant ratifying a
  fiction. Codex's sharpest disagreement was with the other half of that
  move — that the adaptive predicate could simply be declared the replacement —
  and it was right: the shipped system has an exact orientation predicate but
  approximate constructions, quantized vertex identity, ordinary-float
  booleans and an uncertified display path. The replacement text says that.

- **Reversal gate.** Raising `coordBoundMm` again after release requires a
  version decision, because documents using the wider domain are rejected by
  readers built against this one. Lowering it is not available at all without
  invalidating files.

### ED-0101: Editing semantics belong to the core, and a second front end proved it

- **Date.** 2026-09-05
- **Context.** Building a GUI on the issue-0024 bridge exposed something the
  single-front-end era had hidden: the logic that decides what an editing
  gesture COMMITS lived in `domowik-cli`. So did `Plan`, which opens a package,
  chooses the writing actor, projects it and writes it back. Neither is
  command-line logic. The GUI could not call either, and started assembling
  operation batches by hand.
- **What that actually cost, stated concretely.** A wall drawn in the GUI got
  no auto-join (R3.1.2), no induced split (R3.1.1), and no remap-or-reject for
  the openings on the wall it crossed (R4.3.3). Deleting a wall left its join
  memberships dangling and its openings orphaned. The two front ends were
  producing DIFFERENT DOCUMENTS for the same user action, and nothing in the
  build would ever have said so.
- **Decision.** `Plan` moves to `domowik-container`; the wall-edit batch
  algebra moves to a new `domowik-edit` crate depending only on format,
  document and geometry. The CLI re-exports both, so no call site moved. The
  bridge gains `domowik_plan_edit`, which takes a GESTURE rather than a list of
  operations.
- **Why a gesture call rather than exposing the builders.** `execute` takes
  operations the caller already decided on; a gesture is an intention whose
  consequences are the specification's to determine. Handing the GUI the
  builders would have let it call them in the wrong order or skip one. Handing
  it `{"kind":"addWall", …}` cannot.
- **The general rule this establishes.** Logic that decides what a document
  BECOMES belongs behind the core's boundary, not in a front end. A front end
  chooses what the user is trying to do; it never decides what that means.
- **Consequence for the spike's status.** `spikes/0024-flutter-rust-bridge`
  now contains a usable editor and depends on four core crates. ED-0097 said a
  spike is not a product component and that stands — but the gap between this
  spike and a product component is now mostly paperwork, and promoting it to a
  distributed crate is owed once ADR 0003's gate is decided. Recorded rather
  than done, because promotion changes the distributed graph and the licence
  guard, and ADR 0003 is still Proposed.
- **Reversal gate.** None. This decision only moves code to where its callers
  are; nothing about the format or the specification changed.

### ED-0102: OWNER DIRECTION — every issue reaches Done or Not needed, and implementation is delegated

- **Date.** 2026-09-12
- **Status.** Owner direction, recorded by the tracker owner. Supersedes the
  single-agent clauses of
  [ED-0090](#ed-0090-owner-decision--model-policy-litellm-revoked) and
  [ED-0091](#ed-0091-owner-decision--full-rust-rewrite-go-frozen-as-reference-single-driver)
  and the `Role` and `How to work` sections of `INSTRUCTIONS.md`. Everything
  else in those decisions stands.
- **The direction, as given.** Finish all 63 issues, each ending at `Done` or
  at `Not needed`. Implementation is delegated to Claude workers, one issue per
  worker, each in its own worktree and branch; the orchestrator merges to main.
- **Why this is recorded rather than applied silently.** `INSTRUCTIONS.md` says
  in terms that there is "no orchestrator, no executor/reviewer pair, no Herdr
  workspace, no delegation", and ED-0085 ruled work strictly single-threaded.
  An agent reading only those files today would be following a revoked
  instruction and would be right to refuse to delegate. The file itself is not
  edited here because the owner holds uncommitted changes to it on main; this
  record is the authority until they land.
- **What did NOT change.** Adversarial review is still Codex on `gpt-5.6-sol`
  at high reasoning effort, and LiteLLM is still revoked (ED-0090). Verification
  is still `./scripts/verify.sh` exiting 0 with a `=== VERIFICATION PASSED ===`
  banner, and a run without a banner is still not a result (ED-0077). Delegation
  changes who does the work, not what counts as done.

#### `Not needed` — the second terminal status, and its guardrail

- **Decision.** `Not needed` closes an issue whose scope is obsolete or
  superseded. The issue moves to `issues/not-needed/`, the index is updated in
  the same commit, and the file carries a written rationale naming what
  superseded the scope and where any surviving obligation now lives.
- **The guardrail, stated because the failure mode is obvious.** A terminal
  status is not a delete key. Acceptance criteria stay in the file, unedited. An
  issue closed as `Not needed` must still read as an accurate record of what was
  asked for and why it stopped being asked for. **Unfinished is not obsolete.**
  Work that is still owed and merely inconvenient closes as nothing at all; it
  stays open, or it is re-scoped in writing with the residual named. This is the
  ED-0096 lesson in a different costume: a gate is not re-scoped by asserting
  that what it protects is unimportant.
- **Spelling is exact and enforced.** `Not needed`. `domowik-tracker-check`
  validates every issue's status against the vocabulary in
  `issues/README.md` and fails the `tracker consistency` stage on anything
  else, so `Not Needed` and `not needed` are errors rather than a third and
  fourth state. Three mutants were run against the guard — the vocabulary check
  disabled, `not-needed/` unscanned, and `Not needed` removed from the
  vocabulary — and each was killed by a test.
- **`Dropped` is retained as history.** No issue has ever carried it and
  `issues/dropped/` does not exist, but the checker still accepts it. Rewriting
  a closed file to match a newer vocabulary would destroy the record of what was
  decided when, which is the cost ED-0096 paid to avoid.
- **Affected issues.** All 63.
- **Reversal gate.** If `Not needed` starts appearing on issues whose criteria
  are merely unmet, the status is withdrawn and those issues reopen. The audit
  that accompanies this decision records a per-issue residual precisely so that
  test can be applied to evidence rather than to a status column.

### ED-0103: The five open numeric questions are decided, and boundary edges are owned per sub-span

- **Date.** 2026-09-12
- **Authority.** Orchestrator allocation, 2026-09-12, to the issue-0030
  numerics owner: ED-0103 for the sub-span/ring-dependency policy and the
  specification clarification it needs. ED-0100 left five numeric questions
  explicitly open and assigned them to issue 0030; they are decided here.
- **Scope.** Two subjects, recorded together because they share one cause —
  identity and ownership in the arrangement were each being decided by more
  than one rule.

#### Part 1 — the five ED-0100 questions

Decided in full as N-1 to N-5 of
[NUMERICAL-MODEL §12](docs/NUMERICAL-MODEL.md), which carries the reasoning
and the measured disagreements. In summary:

- **N-1.** Stored coordinates are NOT quantized. The topology grid applies to
  the arrangement, so sub-quantum geometry is topologically absent while
  remaining present in storage and in measurement.
- **N-2.** ONE identity relation: the topology grid (`VertexKey`). Metric
  proximity is an ADMISSION predicate, not identity. Admission must not
  contradict identity, which requires `JOIN_COINCIDENCE_MM ≥ √2 · WELD_MM` —
  they are equal today — and it must be a function of the member SET.
- **N-3.** Split points persist as the nearest binary64. No rational
  coordinate representation; FORMAT-SPEC §2/§3 unchanged.
- **N-4.** The exact-area and half-even display contract of R5.3.1/R5.3.2
  STANDS. The implementation does not meet it and owes conformance.
- **N-5.** R5.1.9 component linking is an exact-signed positive-area test.
  `OVERLAP_RELATIVE` is removed, not retuned.

**Three of these ratify what the code does; two rule against it.** N-4 and N-5
are recorded as obligations rather than ratifications, because ratifying them
as built would have written a defect into the specification. The rule applied
was ED-0102's: unfinished is not obsolete.

**What was wrong, and how it was found.** Executed against the shipped code:

- Two points sharing a grid cell, 1.131×10⁻³ mm apart, are ONE vertex to the
  arrangement and a `JoinCoordinateDisagreement` to validation. Two points
  exactly one tolerance apart across a cell boundary are the reverse. Three
  join members validate or fail **according to which is listed first**, though
  FORMAT-SPEC §2.3 (C3) records membership as an unordered set — a determinism
  defect, not a tolerance question.
- Both of R5.3.2's normative tie examples fail: 30,245,000 mm² displays as
  `30.25 m²` where the rule requires `30.24`, and 10,655,000 mm² as `10.65`
  where it requires `10.66`. The nearest double lies above the tie in one case
  and below it in the other, so half-even on the double disagrees with
  half-even on the exact value in both directions. Neither is asserted by a
  test today.
- `OVERLAP_RELATIVE`'s stated justification — area error near the coordinate
  bound — was made stale by ED-0100's own translation fix. What the term does
  now is UNDER-include: above a separate area of 1 m² it overtakes the absolute
  floor, and at 10¹² mm² (the campus and site-plan case ED-0100 raised the
  bound to admit) it suppresses a genuine 0.5 mm² overlap that R5.1.9 requires
  to link.

#### Part 2 — boundary-edge ownership, decided per sub-span

- **Decision.** A snap candidate's exclusion is decided per maximal
  CONSTANT-OWNER SUB-SEGMENT of the canonical occupied boundary, not per
  canonical edge; the ownership unit is the contributing RING, not the wall; and
  a boundary sub-span is a candidate iff at least one NON-EXCLUDED contributing
  ring generates it. A wall-rectangle ring depends on exactly its own wall; a
  join-patch ring depends on ALL of its member walls.
- **Why not per canonical edge.** `canonical_sourced_ring` removes collinear
  vertices and UNIONS the tag sets of the edges it merges. Executed at HEAD:
  two collinear abutting walls, `Wb` spanning x ∈ [0, 1000] and `Wa` spanning
  x ∈ [1000, 3000], produce a single four-vertex ring whose bottom edge
  (0, −50) → (3000, −50) carries owners `[Wb, Wa]`. The vertex at x = 1000 is
  gone and no consumer can recover it. Excluding only when EVERY owner is
  excluded then retains the x ∈ (1000, 3000] half, which exists solely because
  `Wa` is there and translates with it — a pointer at (2500, −52) snaps to
  (2500, −50), on the dragged wall's own boundary, which is exactly the
  self-snap R6.1.3 forbids. Excluding when ANY owner is excluded instead
  discards x ∈ [0, 1000], a legitimate static target. **Both rules are wrong at
  canonical-edge granularity**; the predicate was right and the granularity
  was not.
- **Why the ring and not the wall.** A join patch is the convex hull of its
  members' cross-sections, which is a JOINT construction rather than a
  superposition. Three walls meeting at the origin give a four-vertex patch;
  changing only `Wa`'s far endpoint from (1000, 0) to (1000, 300) takes it to
  six vertices and destroys the hull edge (0, −50) → (50, 0) — an edge whose
  two endpoints come from `Wb`'s and `Wc`'s cross-sections and neither from
  `Wa`'s. No per-member ownership assignment on patch fragments can support an
  every-owner-excluded rule.
- **That the far endpoint moved is the load-bearing detail.** It is what makes
  deriving patch exclusion from the excluded-wall set alone a SUPPORTED
  inference: `cross_section` depends on the wall's axis DIRECTION, so any
  gesture that excludes a wall by identity may deform every patch that wall
  belongs to, whatever kind of gesture it is. No gesture-kind knowledge is
  assumed.

#### Specification clarification: `excludedJoinIds` and the derived set are different sets

`SnapRequest::excluded_joins` and the patch-exclusion set are **not the same
set, do not have the same effect, and must not be required to match.**

- Verified against every consumption site: `SnapClass::JoinNode` is the only
  class carrying a join id, so the caller-supplied list means **join-NODE
  candidate suppression** and nothing else. That is independent gesture intent
  under R6.1.3 — "the moving endpoint of a chain segment excludes its own
  anchor point" — it is not derivable from the excluded-wall set, and it stays
  caller-supplied and unchanged.
- The derived set `{ j : members(j) ∩ excludedWalls ≠ ∅ }` governs **EDGE
  candidacy on patch-derived sub-spans** only. It is computed in core, where
  join membership is already known, and is NOT caller-supplied: a caller that
  omits it reproduces the patch counterexample above.
- **Neither implies the other, and both directions matter.** An explicit
  join-node exclusion must not suppress patch edges, because a join whose
  members are all static has a static patch boundary and is a legitimate
  target. The derived patch exclusion must not suppress the join NODE, because
  the node moves only if the gesture moves that member's endpoint AT THAT join,
  and a far-endpoint drag leaves the near-end node fixed. Core cannot tell
  which, so it does not guess — restricting the API on an inference the core
  cannot support is the failure this clause exists to prevent.

**Consequence for the implementation.** `SnapClass::Edge` carries
`entity: None`, which is why the query refuses outright with
`EdgeOwnershipUnavailable` instead of filtering. Once sub-span ownership
exists that refusal is removed and the excluded-wall set filters edges through
sub-span owners; the edge feature key gains a sub-span index, so hit-ordering
stability is decided once rather than twice.

- **Ownership.** Issue 0058 holds exclusive implementation ownership of
  `boolean.rs`, `walls.rs` and `SnapIndex` for Part 2. Issue 0030 owns the
  policy and the numeric sections only, and changed no core code.
- **What is NOT decided here.** No coordinate bound, dimension cap or quantum
  moves — ED-0100 stands unamended. No BEHAVIOR-SPEC semantics are edited: N-4
  and N-5 record that the implementation does not meet two existing rules, and
  neither rule is weakened to accommodate it. No format-observable validation
  change is applied; N-2's admission repair is specified and left to the format
  owner with its two regression cases.
- **Reversal gate.** N-4 and N-5 are obligations, so they reverse only by being
  discharged or by an explicit owner decision to weaken R5.3.2 or R5.1.9 —
  which would be a specification change with its own record, not a silent
  retune of a constant. Part 2 reverses if a cheaper ownership representation
  is found that still distinguishes the two counterexamples; any replacement
  must fail both of them before it is adopted.

#### ED-0103 amendment (a) — what N-2, N-4 and N-5 landed as, 2026-09-12

Implementation changed two of the three decisions. Recorded here rather than
by editing the text above, so the reasoning that turned out to be incomplete
stays legible.

- **N-2 is STRONGER than the clause it replaces.** The decision above kept
  admission as a DISTANCE and merely required `JOIN_COINCIDENCE_MM ≥
  √2 · WELD_MM` so it could not contradict identity. That is half a fix.
  Widening the ball removes the false REJECTION and makes the false ACCEPTANCE
  more common, because no metric ball aligns with a grid in both directions:
  two points either side of a cell boundary are different vertices however
  close they are, and no radius changes that. Aligning the two rules therefore
  means using the SAME PREDICATE, not two predicates ordered by a constant. As
  shipped, `Document::validate` decides R3.3.4 by topology-cell equality. The
  quantum does not move, so ED-0100's ratified value stands and no downstream
  consumer of the constant changes. `join_at` and the `domowik-edit` proximity
  queries keep a metric ball deliberately: "is there a join near this point" is
  a different question from "do these endpoints denote one node".

- **N-5 needed a THIRD answer, and the measurement is why.** The decision above
  anticipated that exact summation would let merely-touching regions cancel to
  zero, with a residual from constructed vertices that might need a derived
  floor. Measured on 1,000 pseudo-random touching pairs: 971 cancel to EXACTLY
  zero, and 29 leave a residue near 10⁻⁹ mm². The residue is not a distribution
  of rounding noise — it is ONE union vertex sitting an ulp from an input
  vertex it should have reproduced exactly, which a derived floor would have
  hidden rather than explained.

  So `positive_area` reports **Yes / No / Uncertain**. Touching returns No,
  exactly. The 29 return Uncertain: the arithmetic is certain and the geometry
  is not. `locality::link` maps Uncertain to a link, which is OVER-inclusion
  against R5.1.9's "EXACTLY" and is recorded as a deviation rather than
  described as conformance — the two errors are not symmetric, since
  recomputing a component that did not need it produces identical geometry
  while skipping one leaves the document stale. No tolerance floor exists;
  `MIN_OVERLAP_MM2` and `OVERLAP_RELATIVE` are both gone.

- **A defect in the boolean pipeline, which is NOT this issue's to fix.**
  `union` can emit a vertex one ulp from an input vertex when a cut parameter
  lands on it. Witness: rectangles `(0,0)–(2864.5859375, 13658.0546875)` and
  `(2864.5859375, 741.6640625)–(3364.5859375, 3591.8203125)`, which merely
  touch, produce a ring with seven exact input vertices and one
  `y = 0x40ac0fa3ffffffff` where the input is `0x40ac0fa400000000`. Fixing it
  removes the Uncertain band and the R5.1.9 deviation with it. `arrangement.rs`
  and `boolean.rs` belong to issue 0058.

- **N-4 landed as decided**, with the failure modes made explicit rather than
  bounded: non-finite input and `i64` overflow both return `None` instead of
  saturating or panicking, and the preconditions the shoelace cannot check —
  simple rings, contained holes, degenerate rings — are documented. The
  rounding is reached through a filtered entry point so the allocation is paid
  only by inputs near a tie.

- **No `Ord` on `ExactArea`**, against the orchestrator's shorthand and on the
  issue-0060 owner's argument: R5.1.3 already gives faces a normative total
  order by canonical ring, and a second area-based order over the same objects
  would have no authority. Nothing needs one; N-5 asks for the sign of a
  difference, not an ordering.

#### ED-0103 amendment (b) — the N-5 slack bound was unsound and is withdrawn, 2026-09-12

Amendment (a) described a derived slack of `64·n·u·M²` for N-5, justified by
counting the roundings in an intersection. **That justification does not hold,
and the count is why.** A rounded-operation count bounds each operation's
RELATIVE error and says nothing about a division by a near-zero denominator. A
cut is `lerp(a, b, t)` with `t = num/denom` and `denom = |r||s|·sin θ`, so the
displacement is bounded by `|r|·6u/|sin θ|` and grows without limit as two
segments approach parallel.

Issue 0058 derived that shape and measured it over 60,000 randomized pairs at
controlled crossing angle. Worst construction displacement: 1.05×10⁻⁹ mm at
`sin θ = 10⁻¹`, 1.41×10⁻⁷ at `10⁻³`, 1.18×10⁻⁵ at `10⁻⁵`, 1.10×10⁻³ at `10⁻⁷`
— the topology quantum — and 1.14×10⁻¹ mm at `10⁻⁹`. No constant band covers
that.

**A first witness for this, quoted in an earlier revision of this amendment,
was withdrawn by its author and is not the evidence here.** It reported a
0.0116 mm displacement; re-measurement showed that figure was the gap between
the geometry intended and the geometry actually STORED — the two segment
endpoints rounded asymmetrically — and that the construction error in it was
zero. The lesson is recorded because it generalises: construction error must
be measured against the exact crossing OF THE STORED COORDINATES, never
against an intended ideal. The conclusion did not change; the evidence for it
did.

The sign was not safe by argument either. "A crossing implies a real overlap,
so error perturbs a positive number rather than flipping a zero" fails in the
regime that matters: the near-parallel case with the LARGEST construction error
is the thin-sliver case with the SMALLEST true overlap. The two are correlated,
so the comfort is thinnest exactly where the error is biggest.

**The band is removed and no constant replaces it.** `positive_area` now
certifies only what it can prove without a bound: if every vertex of the union
is a vertex the caller supplied, nothing was constructed and the exact sign is
the true sign. Any vertex nobody supplied returns `Uncertain`. Touching
components construct nothing and return an exact `No`; genuinely overlapping
components construct their crossings and return `Uncertain`, which
`locality::link` resolves as a link — the correct outcome for an overlapping
pair, with the recorded R5.1.9 over-inclusion unchanged.

Also from the same review: the float display filter bounded only the per-ring
shoelace and omitted the halving, hole subtraction, accumulation and the
division by 10,000; those are now bounded separately as `(2h + 2)·u·T`. A
non-finite coordinate in a ring too short to enclose anything, or in a hole,
certified a value through the filtered entry point while the exact path refused
it — the finiteness test now precedes the short-ring shortcut. And this
module's claim that `Polygon::canonicalize` establishes simple rings and
contained holes was wrong: canonicalization is not validation, and both are
caller obligations.

Recorded because the reasoning that failed is more useful than the number that
replaced it: a bound must come from the conditioning of the computation, not
from how many operations it performs.

#### ED-0103 clarification (c) — R5.1.9 is decided on the welded regions, exactly, 2026-09-12

Root, under delegated numeric authority (R13, refined by R16 and R25). This
clarification supersedes the raw-piece proposal recorded against issue 0030 and
corrects three errors of mine, each confirmed by construction before it was
accepted.

**R5.1.9 is a topology and recomputation rule**, so it is evaluated on the
occupied regions produced under N-1/N-2 — the WELDED arrangement — and not on a
second geometry assembled from raw coordinates. Both the containment and the
positive-area relations must be decided EXACTLY for those supplied regions.
There is no weaker-containment waiver. Stored coordinates are untouched by any
of this, and none of it promises exact ideal constructions.

**The raw-piece proposal is withdrawn as the route.** It decided overlap on
wall rectangles and join patches, which is a different geometry from the one
the rest of the system uses. It also answered the stored-versus-welded question
the wrong way: a 0.4 µm overlap is topologically absent under N-1, so it must
NOT link, and a raw-piece test links it. Separating-axis over convex pieces
remains ADMISSIBLE where equivalence to the welded topology is certified, never
by blanket assumption.

**The quantum-boundary discriminator is derived through the actual N-2 cells,
not through extent.** My first formulation argued that sub-quantum features are
absent because they are small. That is wrong for exactly the reason N-2 already
records: two points 0.4 µm apart can straddle a cell boundary and stay
distinct, while two points 1.13 µm apart can share a cell. Whether two features
are one is `VertexKey` equality of the points the arrangement produced — a
predicate, not a magnitude. Reasoning from extent was the error this document
warns about in the admission predicate, repeated by its own author.

##### The two contracts, which are NOT the same test

They give OPPOSITE answers on F39's island and courtyard, which is the pin:

| | decided on | holes | F39 island |
|---|---|---|---|
| **Overlap** | interiors of the occupied SETS intersect | excluded from the set | no link |
| **Containment** | one region's RINGS enclose the other | ignored for enclosure | **links** |

R5.1.9 mandates the link — "a courtyard face containing an island loop's
walls" — so using point-in-region for containment would silently drop a link
the rule requires. `ring_contains_ring(outer, outer)` is the right shape for
containment and stays. Being inside a hole does not defeat enclosure; it is the
case the rule exists for.

##### The bounded overlap route, over existing vertices

No crossing coordinate is ever constructed. Exact symbolic witnesses are
permitted; rounded constructed coordinates are not.

1. **Proper crossing.** A boundary edge of A crossing one of B transversally
   implies the interiors meet, because two half-planes with distinct boundary
   lines share a positive-area sector. Four exact orientations decide
   EXISTENCE; the crossing point is never computed.
2. **Collinear overlap** of positive length with the interiors on the same
   side, decided by neighbour orientation.
3. **Partitioned edge-interior witnesses.** Partition a probe edge at the
   EXISTING opposing vertices lying on it, including collinear-overlap
   endpoints, sort exactly, and test the symbolic midpoint of EVERY open
   interval. A midpoint is `(a+b)/2`, a dyadic, exact.

**Why 3 needs every interval and not the first.** Vertex-on-edge contacts
partition an edge into inside and outside spans with no proper crossing
anywhere. Witness: probe edge `(0,0)–(10,0)` against
`(2,0),(5,3),(8,0),(8,-3),(2,-3)`, which touches it only at `(2,0)` and
`(8,0)`. Classification runs outside, INSIDE, outside; a first probe at `x=1`
or `x=9` certifies no overlap for regions that do overlap.

**Why a boundary witness suffices.** A point of ∂A strictly inside int(B) has a
neighbourhood meeting int(A) and contained in int(B), so the interiors meet.
The witness need not be interior to A.

**Why the earlier vertex-only form was incomplete.** Square `[0,2]²` against
the inscribed diamond `(1,0),(2,1),(1,2),(0,1)`: no proper crossing, no vertex
of either strictly inside the other — all four diamond vertices lie ON the
square's boundary — and no collinear overlap, yet the diamond has area 2 inside
the square. Every diamond edge midpoint is strictly inside, so the partitioned
witness finds it.

**Bounded.** Cuts are existing vertices, so the interval count per edge is at
most the opposing boundary's size plus one. Everything reduces to orientation
and point-in-ring, both already exact, with no division and therefore no
escalation whose depth depends on conditioning.

**Per component, not per region.** A witness from one polygon says nothing
about another, and containment must hold for every component.
### ED-0104: Issue 0062 takes Option A — the corpus generator is ported, and its two DEFLATE streams are pinned

- **Date.** 2026-09-12
- **Context.** `internal/corpusgen` was the last Go package in the active tree
  and the only reason `go.mod`, four Go verifier stages and a `setup-go` step in
  two CI jobs still existed. Issue 0062 left its disposition to the owner:
  port it, or freeze the 39 committed artifacts and drop the generator.
- **Decision, part one — PORT (Option A).** The corpus is not finished. Issues
  0028 and 0018 both name outstanding corpus work, and ED-0102's finish-all
  direction makes more fixtures likelier rather than less. Freezing would have
  given up deterministic artifact regeneration, `TestCoverageAudit`,
  `TestExpectationsManifest` and the `EXPECTATIONS.md` sha256 recomputation that
  runs on every check — a fail-closed gate issue 0028 owns — and the first new
  fixture would have had to revive the generator or be hand-built.
- **Decision, part two — the two DEFLATE streams are PINNED INPUTS, and the Go
  compressor is NOT ported.** Two hostile fixtures ship a raw DEFLATE stream
  produced by Go's `compress/flate` at `BestCompression`. `flate2`/`miniz_oxide`
  was measured at every level from 0 to 9 against both payloads and matched
  neither, differing in LENGTH as well as content: 369 bytes against Go's 372
  for the twin, 55 against 51 for the snapshot. Byte-identical artifacts and a
  Rust compressor are therefore incompatible. The streams are carried as
  generator constants with recorded provenance and sha256, and a gate inflates
  each and requires it to equal its payload.
- **Why not port Go's encoder.** Roughly 1,800 to 2,200 lines of translated
  `deflate.go`, `token.go`, `huffman_code.go` and `huffman_bit_writer.go` —
  a vendored compressor inside a repo-hygiene crate, carrying a third-party
  licence obligation, whose differential oracle disappears the moment Go is
  removed. Rejected as disproportionate to what two fixtures assert.
- **THE TRADE IS NAMED, NOT SOLD AS AN UPGRADE.** The worker's first write-up
  called the new gate "strictly stronger". It is not, and that wording is
  corrected wherever it appeared. Recompression asserted that the stored stream
  is REGENERABLE from its payload by a named compressor. Inflation asserts that
  the stored stream EXPANDS to the payload the package declares. Neither
  subsumes the other: payload-to-compressor regeneration is LOST for those two
  streams, and a semantic expansion check plus a pinned digest and provenance
  stands in its place. Accepted on those terms.
- **What is unconditional.** Every other issue-0028 gate is preserved and
  asserts what its Go original asserted; all 39 generated artifacts are
  byte-identical, proven against an oracle taken from the Go generator in memory
  before any Rust existed; no committed fixture byte, recorded digest or sidecar
  number changed; no new third-party crate enters the graph.
- **Sequencing.** The Rust generator lands and is reviewed on its own. The
  atomic Go removal — `go.mod`, `internal/corpusgen`, eight verifier stages,
  `GOTOOLCHAIN` and both `setup-go` steps — follows the CI and ops-P14
  integrations, and issue 0062's last acceptance criterion is proven by running
  `./scripts/verify.sh` with a PATH that contains no Go. No global toolchain
  uninstall.
- **Affected issues.** 0062, and 0028 by way of the gates it owns.
- **Reversal gate.** If a later fixture needs a stream that cannot be pinned —
  a generated family rather than a fixed pair — the pinning stops scaling and
  the decision reopens. The recorded cost is the test: reopen when
  payload-to-compressor regeneration is needed, not when pinning is merely
  inconvenient.
### ED-0105: Four format policy residuals of issue 0004, fixed by derivation rather than by choosing numbers

- **Date.** 2026-09-12
- **Authority.** Bounded, reversible decisions inside the product direction,
  taken under `INSTRUCTIONS.md` "Executive decisions" item 1 by the issue-0004
  owner; ED number allocated by the orchestrator. None of them changes what
  Domowik is for, and each names what would reverse it.
- **Scope discipline.** These are LOGICAL format decisions. The numeric
  parameters of FORMAT-SPEC §3.1 and the N1–N5 policy are not touched here;
  ED-0100 and ED-0103 own those.

#### 1. `lwwHorizon` at COMPLETE COVERAGE — **PROPOSED, not ratified**

**Status: Proposed, pending an explicit owner ruling (root direction,
2026-09-12).** `docs/LWWHORIZON-FIXATION-ANALYSIS.md` reserves this decision to
the product owner in terms, and an agent argument that the trade-off dissolves
does not dispose of that reservation, however good the argument is. What is
settled here is the SAFE MINIMUM — complete retention loses no data — and that
stands on its own. It is a PROPOSED CONTRACT rather than a description of the
shipped system: `Package.snapshot` is a bare `Document`, no `winnerPositions`
or `createdPositions` member exists anywhere in the wire, and S4's tombstones
are a bare `Vec<Uuid>` in the manifest rather than positions. Saying complete
retention is "currently implemented" was wrong, and root corrected it. What is NOT settled
is fixing it as v0.1 product policy. The clause is written into §5.1 marked
Proposed; the other three decisions in this record are unaffected.

- **Decision.** FORMAT-SPEC §5.1's retained competing-write horizon equals the
  snapshot's own S2 coverage. A snapshot retains a creation position for every
  id it carries live or tombstoned, a winner position for every mergeable field
  it carries, and a tombstone for every delete it absorbs, with no count, age
  or byte window inside that.
- **The correctness argument, which is the part that holds.** Any window
  smaller than coverage breaks §4.7's vector V5 for inputs outside it, and V5
  is a fixed conformance vector rather than a tunable. Tombstones cannot be
  aged out at all without §5.2 S6's drop precondition, which v0.1 supplies no
  protocol to attest, so a window over them is unsound at every value. This
  argument establishes that complete retention is SAFE and that no smaller
  value is available without a conformance change. It does not by itself
  establish that the horizon should be FIXED here rather than by the owner.
- **The cost argument does NOT hold, and the earlier draft of this record
  overstated it (root correction, 2026-09-12).** It claimed issue 0035's
  measurements put retention at "116 KB compressed … 0.7% of L23's 16 MiB",
  presented as measured headroom. Three things are wrong with that. The figure
  is Go-era Part A whole-snapshot COMPRESSED bytes, not S1/S3 position
  metadata. The Rust implementation retains no S1/S3 positions at all, so the
  metadata cost of this horizon has never been measured in the system that
  would pay it. And the comparison crosses byte classes that §9.2 says are
  separate quantities — compressed bytes against a cap whose class is not
  stated. **The honest position: the metadata cost of a complete horizon is
  UNMEASURED.** It needs a genuinely exclusive measurement slot against a Rust
  implementation that has S1/S3, which does not exist yet (issues 0011/0027).
- **A defect this exposed, in a row this issue owns.** §9.2 requires that
  "every limit row below names which quantity it binds". Row L23's
  "snapshot metadata section ≤ 16 MiB" names NO byte class — logical,
  physical, or decoded — so it cannot be compared against any measurement
  without first choosing one, and logical versus compressed differ by roughly
  an order of magnitude on this corpus. Recorded as an open residual of issue
  0004 and put to the owner alongside the horizon question, because the
  horizon's affordability cannot be assessed until the cap has a domain.
- **One economy, derived.** A mergeable field whose current value came from the
  `createEntity` that owns the id, and was never written since, needs no
  `winnerPositions` entry: S1 already retains exactly the position a late write
  must be compared against. Readers MUST fall back to it and MUST NOT read the
  absence as "no winner".
- **Compatibility.** It changes no byte of any package or fixture, because the
  members it governs are not yet emitted by anything. §5.1 gains one reader obligation
  so the decision stays reversible: retaining MORE position metadata than the
  horizon requires is never an error, and a reader must not infer anything from
  it. A future narrowing is then a writer-side change rather than a format
  break. It does not relax S3's separate referential exactness.
- **Reversal gate.** A measurement, on a Rust implementation that actually
  retains S1/S3 positions and against an L23 cap whose byte domain has been
  named, showing a realistic document approaching the cap.

#### 2. B7 defines no semantic caps, and the two deferred ones close as derived

- **Decision.** v0.1 defines NO semantic cap on batch contents beyond the §2.3
  schema constraints and the ratified L17/L18/L23 envelopes. No candidate cap
  carries semantic content: a batch of fifty thousand legitimate operations
  means what a batch of five means, and refusing it for being large is what
  L23 is already for.
- **`wallJoin` membership** is bounded by `w` for a document holding `w`
  walls: members are unique `{wallId, role}` pairs, and §2.3 prohibits both
  endpoints of one wall in the same join, so each wall contributes at most one
  member. L17 bounds the serialized array independently and whichever is
  smaller binds. The first draft of this decision said `2w`, reading the
  two-valued role and missing the endpoint prohibition that halves it — caught
  by self-attack after the advisory round failed, and recorded because the
  error was in the permissive direction, which is the direction that matters.
- **Frontier actor count** equals L18. An actor names a replica epoch, so the
  count is history rather than state, and a tighter cap rejects a legitimate
  long-lived document for having been edited on enough devices. Row L33's
  revisit trigger asked what issue 0004 fixes this to; the answer is L18, so
  **L33's derived maximum stands and that trigger is DISCHARGED** rather than
  left stale.
- **Reversal gate.** B2 rather than a number: if an implementation ever mints a
  fresh actor epoch per session, L18 becomes a ceiling reachable by opening a
  document often, and the minting is the defect to fix.

#### 3. Row L11 gains the counting contract it never had, and Rust gains the guard

- **The defect, measured rather than argued.** L11 ratified 128 and a boundary
  but never said what a level is, so two conforming readers could disagree
  about the same bytes. Worse, nothing in the Rust workspace enforced it: both
  untrusted parse sites relied on `serde_json`'s incidental recursion limit,
  and the pinned 1.0.151 accepts 127 levels and **rejects 128** — a document
  L11 declares conforming, refused at exactly the ratified boundary, with an
  unmapped error rather than `limit-nesting-depth`. That library limit can also
  be switched off by a build feature from anywhere in a dependency graph.
- **Decision.** §9.3 states the counting contract in full: a level is an
  enclosing array or object, the outermost at level 1; scalars and object keys
  are not levels; depth 128 parses and depth 129 rejects with
  `limit-nesting-depth`, before the offending container is entered. The rule is
  DERIVED from the corpus twins issue 0028 already authored — 128 nested arrays
  golden, 129 hostile — so the contract matches the fixtures rather than
  overriding them.
- **Implemented, not routed.** `domowik-container` counts depth over the bytes
  before any parse and reports the token; the CLI `apply` path, which does not
  go through the package reader, calls the same check. `serde_json`'s redundant
  limit is stood down for those two parses only, after depth is already bounded.
  Four mutants were run — guard removed, boundary made exclusive, string
  awareness removed, CLI call site deleted — and each was killed by a test.
- **Consequence for issue 0015.** Its "input size, operation count, depth, and
  numeric bounds are enforced" criterion had no enforcement behind the depth
  clause. It has one now. The criterion was not re-scoped and the issue was not
  quietly left Done on a routed-away gap.

#### 4. Two rejection classes are RESERVED — UNREACHABLE, and neither is removed

- **Decision.** `ambiguous-manifest` and row L29's duplicate-physical-blob
  tolerance are unreachable under the v0.1 profile, as issue 0005's rulings
  found, and **both are retained** with a normative disposition saying so and
  naming the diagnostic that fires first — `duplicate-logical-name` in both
  cases. Conformance coverage for such a class is discharged by a fixture
  asserting the PRECEDING token plus the recorded rationale, not by a gap
  record and not by a fixture expecting a token no conforming reader can emit.
- **Why retention rather than removal.** ED-0079's lesson: "this rule cannot
  fire" and "this rule should not exist" are different claims. The classes are
  unreachable because of the closed namespace and the injective blob mapping,
  not because the conditions are meaningless, and a later profile that opens
  either makes them reachable with no taxonomy change. Removal would also
  orphan a fixture, a coverage-set entry and production code.
- **This unblocks issue 0028's** `gaps/ambiguous-manifest-deterministic-trigger`,
  which has stood as blocked-pending-owner-decision since 2026-08-25.

- **Affected issues.** 0004 (owner), 0005, 0010, 0011, 0015, 0027, 0028, 0029,
  0035.
- **Also corrected in passing, both disclosed rather than absorbed.** §5.1's
  closing paragraph still said "pre-horizon winner KEYS" after ED-0037 renamed
  them positions, and §9.4's negative-zero sentence still deferred
  canonicalization to "issue 0004's schema … once ratified" when §3.2 had
  already ratified it. Neither is a new decision; both were stale text citing
  decisions already in force.

### ED-0106: The fallback placeholder box ignores scale and rotation

- **Date.** 2026-09-12
- **Status.** Root decision, refining BEHAVIOR-SPEC §9.6. Scoped to the
  no-dimensions fallback branch only.
- **Context.** FORMAT-SPEC §2.3 gives `assetInstance` a `transform` carrying
  `translationMm`, `scaleUniform` and `rotationDeg`. BEHAVIOR-SPEC §9.6 says a
  degraded instance with no descriptor dimensions renders as "a fixed 1000 mm
  box centred on the instance origin", and calls placeholder geometry an
  "axis-aligned box". Implementing R7.5.3 forced the question the two
  sentences leave open: does that box carry the instance's scale and rotation?
- **Decision.** It does not. The fallback is a 1000 mm square, AXIS-ALIGNED IN
  PLAN SPACE, centred at `translationMm`. `scaleUniform` and `rotationDeg` are
  ignored. Translation is applied, because "centred on the instance origin" is
  what `translationMm` means.
- **Why.** "Axis-aligned" and "rotated" are contradictory unless the text means
  the bounding box OF a rotated box, which it does not say and which would make
  the extent depend on the angle: a 1000 mm square at 45 degrees has a bounding
  box about 1414 mm across, so "fixed 1000 mm" would be false. `scaleUniform`
  contradicts "fixed" the same way. And a box scaled or rotated to match
  content asserts knowledge of content that could not be decoded, which is the
  one thing a degraded placeholder must not do.
- **An argument that was offered and REJECTED, recorded so it is not offered
  again.** Issue 0059 argued that R7.1.3's "exact min corner" forbids a rotated
  box, because rotation is constructed with sine and cosine and so yields a
  rounded corner. That reasoning is wrong: exact COMPARISON of stored values is
  a different property from exactness of CONSTRUCTION, and this repository
  already relies on the distinction. A wall's occupied rectangle is a rounded
  construction whose vertices are then compared exactly, which is why the
  `region` class carries an approximate projection and a total ordering at the
  same time. A rotated placeholder box would have been no worse. The decision
  stands on the textual and content arguments above, not on that one.
- **Scope, stated because it is narrow.** This settles the FALLBACK branch
  only. The descriptor-dimensions branch of §9.6 remains OWED to asset
  resolution and is not waived; whether declared dimensions scale with
  `scaleUniform` is a separate question and is still open.
- **Verification.** PH1 cannot discriminate the two readings, because its
  instance has an identity transform and they agree there. Fixture PH2 and
  `a_non_identity_transform_does_not_resize_or_rotate_the_fallback_box` supply
  an instance with `scaleUniform` 3 and `rotationDeg` 45 and assert the box is
  unchanged, so a future implementation that applies the transform fails.
- **Reversal gate.** Applying the transform later changes which entities a
  click and a marquee select, so it needs a specification change and new
  fixtures, not an implementation edit.
### ED-0107: A bounded Rust amendment to ED-0023's inspection seam

- **Date.** 2026-09-12
- **Status.** Root orchestrator engineering decision, taken under delegated
  bounded authority. **NOT owner direction**: the owner has not reviewed this
  code and no claim is made that they did. Amends
  [ED-0023](#ed-0023-authorize-a-parse-only-persistence-inspection-seam-for-issue-0027)
  for the Rust seam only; ED-0023 otherwise stands.
- **Affected issues.** 0027, and any future reader of `domowik ops`.
- **What was inspected before ruling.** The root orchestrator read
  `Package::log` and `OperationLog::deliver`. The latter validates framing and
  causality and builds a PRIVATE ordering buffer; it does not construct an
  entity projection.
- **Decision.** On the default `ops` path, materialising typed INERT
  operations and private framing/ordering metadata is ALLOWED. Document
  execution, entity projection, validation callbacks, shared or session
  mutation, and file writes remain FORBIDDEN there. This supersedes ED-0023's
  Go-symbol "call Deliver" wording for this seam and for nothing else; the
  no-execution requirement is untouched. One security parser is kept rather
  than forked.
- **Why the wording needed amending rather than the code.** ED-0023 was
  written against a Go `Deliver` that ordered AND applied. The Rust core
  splits those into `OperationLog::deliver` and `replay::project`, so the Go
  symbol name no longer identifies the thing the prohibition was protecting
  against. Reading it literally would have required a second package reader in
  `domowik-container` — the fork of security-critical parsing that ED-0023's
  own reason exists to prevent — while leaving the actual execution boundary
  no better defended.
- **The obligation that survives, and how it is discharged.** ED-0023's
  differential requirement stands: structurally valid, application-invalid
  data must INSPECT successfully while the replaying path REJECTS it.
  `application_invalid_data_inspects_but_replay_rejects_it` is that witness,
  and `default_path_never_applies_the_log` guards every default text and JSON
  output against carrying a replay verdict. Both fail when the application
  call is hoisted out of its `--replay` branch.
- **Found while ruling, and recorded because it was the trigger.** Adversarial
  review (Codex, `gpt-5.6-sol`, high effort) rated the unamended narrowing a
  HIGH finding against the literal text, and issue 0027 routed it here rather
  than deciding it. The narrowing was real; the remedy was a decision, not a
  patch.
- **Not authorised by this.** An operator's `--assert-local-only` assertion is
  NOT peer attestation and must never be described as one (§5.1 S6 supplies
  none in v0.1).
- **Owner escalation kept open.** This is a delegated engineering call on a
  seam ED-0023 drew in Go symbols. If the owner wants the literal boundary
  restored, nothing here is load-bearing beyond the two witnesses, which stand
  either way.
- **Reversal gate.** If a reader is ever shown to reach entity projection,
  validation callbacks or a write on the default path, this amendment is
  withdrawn and the seam returns to ED-0023's literal boundary.
### ED-0109: Issue 0025 depends on the `wgpu` crate directly, not on wgpu-native's C API

- **Date.** 2026-09-12
- **Status.** Delegated engineering choice, authorised by root. **Not personally
  reviewed by the owner**, and recorded as such so nobody later reads it as an
  owner ratification.
- **Context.** ADR 0003 names "wgpu-native", "consumed through its C API". That
  phrasing was written when the core was Go and the GUI plugin was not assumed
  to be Rust, the same assumption ADR 0003's "Flutter-to-Go" wording carried
  until ADR 0008 forced its re-assessment.
- **Two facts settle it.**
  - The crates.io package literally named `wgpu-native` is version 0.5.1,
    published 2020-07-10, licensed **MPL-2.0**. MPL-2.0 is not in
    `docs/third-party/licenses.json` `allowedLicenses`, so the project's own
    registry rejects it. Following the ADR by name pulls a six-year-old
    weak-copyleft dependency into the DISTRIBUTED graph.
  - The live project of that name, `gfx-rs/wgpu-native`, is Apache-2.0 and
    maintained, but it is a **C API wrapper around the `wgpu` crate**, and the
    reason it exists is to serve non-Rust callers.
- **Decision.** Issue 0025 depends on `wgpu` **30.0.1** directly. Root verified
  the pinned upstream `Cargo.toml` at that version: **MIT OR Apache-2.0**,
  `rust-version = 1.93`. Both licence arms are on the allowed list. The C API
  is not used, because our caller is Rust: the GUI plugin is already a
  cargo-built library, proved on macOS, Linux and Android by issue 0024, and
  routing Rust through a C wrapper back into Rust would add a second FFI
  boundary inside our own process for no benefit.
- **Conditions, all binding.** Minimal target-specific features. The lockfile
  is pinned and hashes recorded. Resolved transitive licences are audited
  **before linking**, and an unknown or incompatible licence stops the work
  rather than being waived. The runtime inventory and NOTICE remain issue
  0036's; this issue does not edit those generators.
- **What this does NOT do.** It does not discharge any gate. ADR 0003 stays
  **Proposed**: the surface, platform and performance gates are untouched by a
  choice about which package to link. It is also not a decision that wgpu is
  the renderer — the spike can still fail, and ADR 0003's failure clause still
  requires a new ADR rather than a hidden platform-specific divergence.
- **Reversal gate.** If the direct dependency proves unworkable — a backend
  that only the C wrapper exposes, or a licence in the tree that cannot be
  admitted — this decision is revisited here rather than quietly worked around.

### ED-0110: Derived topology is snap-rounded on a 2⁻¹⁰ mm grid

- **Date.** 2026-10-08
- **Status.** Owner direction ("development mode, don't worry about
  continuity; apply improvements"), issue 0096.
- **Context.** The union behind every occupied region and room found
  crossings on the original segments, then moved vertices onto weld
  representatives without re-noding, classified fragments against the
  original rings and traced walks that need not close. An independent review
  (Codex GPT-6-Astra) produced, and the owner's agent reproduced, three plain
  wall rectangles losing 60% of their union. ED-0103 N-2 fixed WHICH points
  are one vertex but not how edges route through identified vertices, which
  is the part that failed.
- **Decision.**
  1. The topology quantum is **2⁻¹⁰ mm** (≈ 0.977 µm), replacing 10⁻³ mm.
     Cells are half-open `[k − ½, k + ½)`; a point's cell is
     `floor(v · 1024 + ½)`. Scaling by a power of two is exact and every
     boundary is representable, so membership is decided without rounding.
     `JOIN_COINCIDENCE_MM` and `WELD_MM` both equal it, and R3.3.4 join
     agreement is cell equality on this grid.
  2. Derived topology is built by **snap rounding** (Hobby, Comput. Geom. 13
     (1999) 199–214): a cell is hot when it holds an endpoint or a crossing,
     and every segment is routed through the centres of all hot cells it
     meets. A crossing whose correctly rounded construction lands exactly on
     a cell boundary marks both neighbours.
  3. Faces carry winding numbers; occupied means positive winding. R5.1.9
     overlap is a bounded face covered by two components, containment is
     exact ring enclosure, both on the level's one labelled arrangement.
- **What changes for users.** Derived coordinates — occupied boundaries,
  room outlines, areas — are cell centres. Stored coordinates are untouched
  (N-1 stands). Input on the grid (every whole or binary-fraction millimetre)
  is reproduced exactly; other input moves at most half a cell per axis.
- **Supersedes.** ED-0103 N-2's decimal grid and N-3's statement that split
  points enter topology as the nearest binary64: they still persist that way,
  but topology now sees their cell. ED-0103 clarification (c)'s overlap route
  over existing vertices is replaced by the face-coverage test, which is what
  (c) required of it.
- **Known cost.** Routing work is the number of (segment, hot cell)
  incidences, which Hobby §2.3 warns can approach `N·(N + K)` for nearly
  collinear input. `benches/scale.rs` measures it (`fan-N`).
- **Reversal gate.** A measured plan where incidence count, not crossing
  count, dominates editor latency; or a fixture whose rounded topology is
  wrong rather than merely displaced.
