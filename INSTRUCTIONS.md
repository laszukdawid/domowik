# Working instructions

Follow [AGENTS.md](AGENTS.md) and [Contributing](CONTRIBUTING.md).

1. Understand the requested scope, current architecture, relevant specifications
   and acceptance criteria before changing behavior.
2. Preserve unrelated work. Keep implementation, tests and current documentation
   together; do not include personal plans, credentials or internal session logs.
3. Challenge the implementation with concrete failing inputs. Tests should
   demonstrate observable behavior and fail for the defect they guard.
4. Run relevant checks while iterating, then `./scripts/verify.sh`. Report the
   actual banner, failed stage and skipped platform checks. A partial run is not
   a full pass.
5. Keep Accepted and Proposed architecture decisions distinct. Ask the owner
   about material product, public-identity, licensing or provenance uncertainty.
6. Document the current contract and remaining limitations, not the history of
   agent assignments, model choices or issue-status changes.
7. Never push, publish, rewrite history or discard existing changes without
   explicit authorization.
