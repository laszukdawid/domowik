# ADR 0011: Versioned core with namespaced feature extensions

- **Status:** Accepted (architecture direction; extension wire design pending)
- **Date:** 2026-10-08

## Context

The owner selected a defined core with namespaced extensions, rather than a
versioned closed schema alone or a document made entirely of generic feature
records. Feature scope is still developing and there are no external users
whose unpublished development files need a compatibility guarantee. The owner
wants a file-level version key so the document's high-level schema can evolve
within the same file type and package approach.

The existing manifest already contains `formatVersion: "0.1"`; the reader
dispatches by supported schema labels and refuses unsupported versions. The
current closed namespaces and unknown-kind rejection remain in force until a
reviewed extension schema and its conformance tests replace the relevant rules.

## Decision

1. **One overall schema identifier.** Keep `formatVersion` in `domowik.json`
   as the authoritative file-level schema identifier. Do not introduce a
   competing `version` key or infer a schema from content. Retain `.domo` and
   the ZIP-based package approach as the baseline for schema evolution.
2. **Defined core, extensible features.** Identity, units, references,
   authoritative versus derived data, atomic operations, integrity and save
   rules form the core. Feature-specific information uses explicitly
   namespaced, versioned extensions. This is a durable boundary, not a freeze
   of the core: incompatible core changes select another `formatVersion`.
3. **Explicit capability requirements.** The extension contract must distinguish
   optional information from capabilities needed to interpret the plan or edit
   it safely. Unknown optional data is preserved with identical meaning, or
   write-back is refused. Unsupported essential behavior cannot be silently
   ignored. A reader must restrict affected edits or refuse editing when it
   cannot prove that changing core data preserves unknown extension meaning.
4. **Development flexibility.** No migration/support promise is made for every
   unpublished development schema. Incompatible development revisions use a
   distinct `formatVersion` when introduced; a reader supports only the labels
   it actually implements. Compatible additions may retain their label when
   existing readers demonstrably handle them safely. Existing `"0.1"` files
   and emission are not relabelled by this decision.
5. **Version selects rules, not a conversion.** A schema change can keep the
   file extension and package container while changing its contents. Readers
   dispatch to the implementation for that version, convert explicitly when a
   tested conversion exists, or refuse. Editing a version string alone cannot
   transform incompatible contents. Released-schema obligations remain those
   of FORMAT-SPEC §8.2; post-v1 policy in ADR 0007 remains Proposed.
6. **Feature design remains incremental.** This decision does not invent
   electrical, curved-wall, catalog or external-integration payloads. ADR 0005's
   deferral of external-reference fields remains in force. Snapshot retention
   policy is a separate owner decision and is not ratified here.

## Wire-design and implementation gates

The logical contract must define extension identity/version syntax, placement at document
and entity scope, capability declarations, dependency/reference semantics,
operation/undo/replay rules and preservation during compaction. Exact field
names and JSON examples are not selected by this ADR.

The container contract must define package placement/inventory, how unsupported capabilities
are detected before effects, safe inspection/edit/write outcomes and how the
first extension-enabled schema is distinguished from readers that would ignore
its requirements. Required feature declarations cannot be smuggled into fields
an existing reader treats as optional. Arbitrary archive entries remain invalid;
an extension namespace needs explicit syntax and the existing resource/security
limits.

Implementation requires positive and negative fixtures for optional-data round trips, unsupported
required behavior, dependency-changing edits, version dispatch and invalid
extension declarations. These gates remain open; accepting the direction does
not establish reader/writer implementation.

## Compatibility assessment and provenance

This change records architecture and development policy. It changes no emitted
bytes, supported reader labels, entity schemas, dependencies or resource caps.
The existing `formatVersion` gate remains implemented. New extension wire rules
require tests and an explicit assessment of older readers before activation.

The development-version policy does not create a public predecessor release or
require migration of development artifacts.

Provenance: original Domowik product design and its existing specification and
source. No external application source, schema or data was
consulted; no third-party component was added.
