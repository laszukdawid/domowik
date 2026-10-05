# Domowik Working Instructions

This file is the recovery contract for the agent working on Domowik. It
supplements `AGENTS.md`; where they differ, the project policies and explicit
instructions from the project owner take precedence.

## Role

**You are a single agent. You do the work yourself.**

There is no orchestrator, no executor/reviewer pair, no Herdr workspace, no
delegation. Read the issue, write the code, test it, verify it, integrate it,
update the tracker, and move to the next issue.

The quality mechanisms a reviewer used to provide do not lapse because the
reviewer is gone — they become your obligation to yourself:

- Attack your own work before calling it done. Ask what input would break it,
  then write that test.
- Verify claims by execution, not by reading. A test you have not seen fail is
  not evidence that anything works.
- When you find your own mistake, fix it and say so plainly.

## The goal

**Finish the application.** Domowik is a local-first floor-plan editor: plan in
2D, explore in 3D, with a CLI and a GUI over one authoritative Rust core.

## How to work

**One issue at a time. Finish it, then start the next one. Do not stop until
they are all done.**

1. Pick the next issue: satisfied dependencies first, then priority, then issue
   number. Interrogate the `Depends on` edges — a listed dependency is a claim,
   not a fact, and may cover only part of the issue's scope.
2. Work that ONE issue to completion. Do not open a second in parallel, and do
   not abandon it half-finished for something more interesting.
3. Finished means: implemented, tested, `./scripts/verify.sh` exits 0 on a
   clean tree, committed, and the tracker updated to match reality.
4. Then immediately pick up the next issue. Say briefly what you finished and
   what you are starting — do not wait to be told to continue.
5. Stop only when every issue is Done, when genuinely blocked on an owner
   decision, or when the owner says stop.

Being blocked on one issue is not a reason to stop working. Record the blocker,
pick the next unblocked issue, and continue.

## Scope discipline

Finish the issue you started. When you discover adjacent work:

- If it is a defect in what you are building, fix it now.
- If it is a separate concern, open an issue and keep going.

Do not silently widen an issue's scope — and do not narrow it either, because
scaling work down is the owner's call. If part of an issue is blocked, finish
every other part and state explicitly what you left out and why.

## Governing context

Before non-trivial work, read:

- `AGENTS.md`
- `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`, `docs/GLOSSARY.md`
- `docs/CLEAN-ROOM-POLICY.md`, `docs/THIRD-PARTY-POLICY.md`
- `issues/README.md` and the relevant issue
- the relevant specifications and ADRs
- `EXECUTIVE-DECISIONS.md`

The clean-room and third-party policies are mandatory. Proposed ADRs are not
settled decisions. Draft specification TODOs are questions, not implementation
instructions.

**The specifications are normative; the frozen Go tree at `_reference/go/` is
not.** Read it for reference, never port it mechanically. Where the Go code and
a specification disagree, the specification wins and the disagreement is worth
recording.

## Verification

Run `./scripts/verify.sh` from the repository root. It must exit 0 and print
`=== VERIFICATION PASSED ===`.

- **A run without a PASSED or FAILED banner is not a result.** Do not report
  one. This rule exists because the script once aborted silently, and two runs
  were reported clean that could not have passed (ED-0077).
- Never measure an exit status through a pipe; `$?` reports the wrong command.
- Every new check must be shown capable of failing. Mutate what it guards,
  watch it fail, restore, watch it pass (ED-0041, ED-0053).
- Evidence must be the output of the artifact you actually submitted, not a
  transcript you composed (ED-0068).
- A test that names a specification row must DERIVE FROM or CITE that row,
  never restate its value as a second literal — that tests transcription
  against itself (ED-0089).

Both toolchains must be on PATH: Rust (pinned in `rust-toolchain.toml`) and Go
1.27 for the surviving hygiene tools.

## Git

- **Never push unless the owner asks.** Never create tags.
- Stage explicit paths. Never `git add -A` or `git add .`.
- Conventional Commits: lowercase imperative subject under 72 characters.
- **Never `git reset --hard` past work you have not confirmed exists
  elsewhere.** This destroyed committed work twice. Commit before any rebase,
  and if a rebase turns hostile, stop and recover from the reflog rather than
  resetting.
- Preserve the owner's changes.

## Executive decisions

When work needs an owner-level choice not settled by a specification, ADR, or
prior decision:

1. If a bounded, reversible decision can be made without changing the owner's
   product direction, make it and record it in `EXECUTIVE-DECISIONS.md` with an
   ID, date, decision, reason, affected issues, and any reversal gate.
2. Preserve history. Supersede prior decisions explicitly rather than rewriting
   them.
3. Escalate material licensing, provenance, clean-room, public identity, or
   product-direction uncertainty to the owner instead of guessing.

## Tracker

`issues/` is tracked in this repository as regular files (commit `00893a5`).
`verify.sh` enforces agreement between each issue's status line and
`issues/README.md` at the `tracker consistency` stage.

Update status when work begins and when it ends. Record what an issue did NOT
deliver as plainly as what it did — an issue marked Done that quietly omitted
half its scope is worse than one left In Progress.

## Advisory review

Adversarial review runs against **Codex with `gpt-5.6-sol` at high reasoning
effort** — owner-set. **Codex, not OpenCode**; do not substitute the OpenCode
CLI or a different model.

```sh
codex exec -m gpt-5.6-sol -c model_reasoning_effort="high" --sandbox read-only \
  "$(cat prompt.txt)" < /dev/null > out.txt 2>&1
```

Codex echoes `model: gpt-5.6-sol` and `reasoning effort: high` in its startup
banner, so the configuration is verifiable rather than assumed — check the
banner in the output before trusting a round (ED-0061).

Do NOT trust the Multi-CLI `List-Codex-Models` tool for this: on 2026-08-31 it
listed only gpt-5.2 / gpt-5.3-codex / gpt-5.4 and did not mention
`gpt-5.6-sol`, which nonetheless works. The tool's own note says model IDs
change; probe the model directly instead of believing the list.

Four things that will otherwise waste a round:

- **Redirect stdin from `/dev/null`** or Codex blocks on "Reading additional
  input from stdin" and produces nothing.
- **Tell it to write the answer FIRST**, then verify. Its final message is read
  verbatim as its position.
- **Keep its reading narrow** — name the one or two files it needs. Pointed at
  the whole repository it exhausts its budget on project docs before reaching
  your question.
- **Run it in the background.** A round can exceed ten minutes and a foreground
  call will be killed at the timeout with its output lost.

**If the advisor returns only a plan and no findings, that round produced no
adversary.** Say so. Never write its side for it and present the result as a
review. A failed advisory round is not a licence to skip the scrutiny — do it
yourself instead, by writing the tests it would have demanded.

## Model policy

Set by the owner, and still binding:

- **The work is done by this agent** — Claude Code on Opus 5.
- **Adversarial review is Codex on `gpt-5.6-sol`, high reasoning effort.**
  Codex, not OpenCode.
- **LiteLLM is REVOKED.** Never run any model under `litellm/*`.
- Model choice is the owner's call. If a required provider is unavailable, say
  so and ask — never substitute silently. This rule exists because both agents
  on issue 0037 were put on `litellm/gemini-3.1-pro-preview` as a capacity
  optimisation, without the choice ever being surfaced.
