# Foundation Revision Policy

## Purpose

This document defines how frozen architecture-foundation contracts may change after approval.

## Revision Trigger

A downstream workstream must open an explicit foundation revision before implementation proceeds if it needs to change any of the following:

- stage ownership
- top-level shared contract family names or semantics
- metadata-only safety boundaries for explain or audit
- extension-point ownership rules
- evaluation data placement boundaries defined by the foundation

## Allowed Downstream Flexibility Without Revision

Downstream work may proceed without a foundation revision when it only:

- adds family-specific behavior inside the approved stage ownership model
- adds fields that are backward-compatible with the frozen contract intent
- adds implementation details that do not alter policy ownership or metadata-only guarantees
- adds tests, fixtures, or evidence that reuse the existing contract families
- consumes the exported `foundation_stage_boundaries()` map and `SanitizationEngine::trace_foundation_flow` proof surface without changing their semantics

## Required Revision Inputs

A foundation revision proposal must include:

- the contract family or stage boundary being changed
- the downstream feature that requires the change
- why the current contract is insufficient
- why a local downstream workaround was rejected
- how metadata-only safety and deterministic behavior remain protected
- which existing verification artifacts (`docs/verification/009-architecture-foundation.md` and foundation test suites) need updates

## Approval Consequence

Affected downstream implementation must not continue as if the new contract were already approved.
The revision must be accepted first so the roadmap remains consistent across workstreams.
