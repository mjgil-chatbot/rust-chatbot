# Spool And Session Contract Versioning

**Status**: Current contract versioning policy
**Last updated**: 2026-05-21

This document defines the versioning and compatibility policy for the durable
worker spool and session-reconciliation surfaces owned or consumed by
`rust-chatbot`.

## Purpose

The worker spool and session-reconcile logic already behave like contracts. The
goal here is to treat them as versioned contracts rather than as undocumented
file shapes plus best-effort interpretation.

## Contract Surfaces

The current contract surfaces are:

- `request.json`
- `state.json`
- `events.jsonl`
- `output`
- `result.json`
- transcript-gated session replacement rules
- terminal reconcile behavior for streamed versus final assistant content

## Current Baseline Version

The current baseline is now explicit in-code:

```text
spool/session contract version = v1
```

`v1` means the current repo behavior described in:

- `docs/architecture/worker-spool.md`
- `docs/architecture/session-worker-lifecycle-spec.md`
- `docs/verification.md`

For the durable JSON artifacts, `v1` is represented by:

- `schema_version = 1` on `request.json`
- `schema_version = 1` on `state.json`
- `schema_version = 1` on `result.json`

Current readers accept missing `schema_version` fields as legacy `v1` inputs so
older on-disk artifacts remain readable. Unknown future versions are rejected.

## Compatibility Classes

Changes to the spool or session-reconcile contract should be classified as one
of the following:

### `Compatible additive`

Examples:

- new optional fields in `request.json`, `state.json`, or `result.json`
- new optional event variants that older readers can ignore safely
- new diagnostic metadata that does not change terminal interpretation

Required actions:

- update the owning doc
- add or update regression coverage
- verify sibling-repo consumers still accept the new shape

### `Behavioral compatible tightening`

Examples:

- stronger pruning rules for stale request-only directories
- stricter transcript cleanup that preserves user-visible behavior
- narrower acceptance of malformed provider labels in prompt-debug assertions

Required actions:

- update docs plus verification mapping
- add a regression that demonstrates the tightened rule

### `Breaking contract`

Examples:

- changing required fields in `state.json` or `result.json`
- changing event interpretation so existing consumers would misread terminal
  truth
- changing reconcile rules so older session data would be reinterpreted

Required actions:

1. bump the documented contract version
2. update both the owning repo docs and the sibling boundary docs
3. add migration or fallback behavior if old artifacts may still exist on disk
4. run cross-repo verification before accepting the change

## Change Checklist

Any spool/session contract change must update all applicable items:

1. `docs/architecture/worker-spool.md`
2. `docs/architecture/session-worker-lifecycle-spec.md`
3. `docs/verification.md`
4. repo-local regressions or assertion tests
5. sibling verification docs when the worker boundary changes

## Required Acceptance Questions

Before merging a contract change, answer all of the following:

1. Can an older UI instance or sibling worker reader still interpret the new
   artifact safely?
2. If not, where is the version bump recorded?
3. Does terminal truth still depend on the same durable artifact path?
4. Does the change affect restart, resume, or stale-session recovery?
5. Which regression or replay fixture demonstrates the new behavior?

## Current High-Risk Areas

Changes in these areas should be treated as contract-sensitive by default:

- terminal `result.json` interpretation
- request-only startup-gap pruning
- transcript reconstruction from `events.jsonl`
- replacement of stale provider session ids with fresh terminal ids
- transcript-gated current-session replacement

## Current Native Enforcement

The current repo-local enforcement surface for this contract is:

- `scripts/verify_spool_session_contract.py`
- `scripts/verify-spool-session-regressions.sh`

The verifier keeps the versioning policy, worker-spool contract, verification
mapping, and focused release regression bundle aligned. The regression script
runs the exact release-mode checks that currently back schema-version
rejection, terminal save ordering, think-only terminal preservation,
request-only startup-gap pruning, stale provider-session replacement, and spool
inspection metadata.

## Related Docs

- `docs/architecture/worker-spool.md`
- `docs/architecture/session-worker-lifecycle-spec.md`
- `docs/verification.md`
