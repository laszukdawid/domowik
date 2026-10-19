# Clean-Room and Provenance Policy

Status: **Active project policy.** It is a risk-control and provenance process,
not legal advice or a guarantee of non-infringement. Seek qualified counsel for
material licensing or interoperability questions.

## Purpose

Domowik is Apache-2.0 and shares a problem domain with GPL-licensed
applications, including Sweet Home 3D and DobryDom3D. GPL software may be used,
sold, and hosted under its terms; Domowik nevertheless chooses an independent
implementation and permissive distribution model. We therefore avoid deriving
Domowik implementation from those source trees.

## Default rule: black-box study only

- Study externally visible behavior by running released applications, reading
  public user documentation, and conducting original usability experiments.
- Do not consult GPL source code, internal tests, class/file layouts, or parser
  implementations for Domowik specification or implementation work.
- Do not copy or translate identifiers, source structure, test vectors,
  artwork, catalogs, data schemas, or serialization formats.
- The `.domo` format is independently designed. A future interoperability
  importer requires its own ADR, source/provenance plan, and legal review.

If source inspection is ever considered necessary, stop work and define a
conventional clean-room process with separated specification and
implementation participants plus legal review. A time delay or fresh AI chat
alone is not sufficient separation.

## From behavior to implementation

1. **Observe or decide.** Record black-box behavior or an original product
   choice without implementation details.
2. **Specify.** Convert it into testable, implementation-independent rules in
   `BEHAVIOR-SPEC.md`. Retain necessary external-source and test-data provenance
   in [PROVENANCE.md](PROVENANCE.md), without internal session or issue history.
3. **Review provenance.** Confirm that the rule can be implemented from the
   specification and cited public material alone.
4. **Implement.** Use only the reviewed Domowik specification and approved
   public/permissive references.
5. **Review similarity.** Investigate suspicious matching names, structures,
   constants, or fixtures before accepting code.

AI systems cannot reliably prove that training or prior conversational context
contains no similar material. They must follow the same source restrictions,
cite concrete references used, and avoid claims of guaranteed legal
independence.

## Algorithms and permissive source

Mathematical ideas and source implementations have different provenance and
licensing questions. For every non-trivial geometry or compression algorithm,
record:

- publication title, author, version/date, and URL;
- exact source artifact consulted, if any;
- that artifact's license and notice obligations;
- whether code is independently implemented, adapted, or translated;
- tests/reference vectors and their provenance.

Do not say an abstract algorithm is "MIT" or "Boost licensed". A particular
implementation or document may carry that license. Patent risk is a separate
question and should be reviewed for material codecs or techniques.

## Evidence and repository contents

Current behavior summaries and necessary provenance may be committed as
original text; internal review/session archives stay outside public source. Do not commit
third-party screenshots, recordings, code, test fixtures, data files, or
catalog assets without a rights and license review. User documentation may be
linked and summarized; avoid unnecessary quotation.

If material of uncertain provenance is added, quarantine it, stop derivative
work, record what happened, and obtain review before deciding whether removal
is sufficient.
