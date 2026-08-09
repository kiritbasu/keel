<!-- keel:generated decision dec_01KZKMPVQWSF1TN6TYEWQ3BJ61 v1 2026-08-09T18:07:39Z
     source of truth is Keel — edits here are not saved -->
# ULIDs are minted from a monotonic generator

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVQWSF1TN6TYEWQ3BJ61`

## Context

`Ulid::new()` re-randomises its low 80 bits on every call, so two ids created in the same millisecond sort arbitrarily.

## Decision

One process-wide monotonic generator.

## Reasoning

SPEC §3.4 rests on ULID order *being* chronological order, so that "what changed since T" is a range scan. A burst of writes inside one millisecond is an agent's normal behaviour. Non-monotonic ids would make an event cursor silently skip or repeat rows — the same class of quiet wrong answer as an inverted graph traversal.

## Consequences

Found by a test, not by reading.

