<!-- specline:generated decision dec_01KZKMPVQWSF1TN6TYEWQ3BJ61 v2 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-9 — ULIDs are minted from a monotonic generator

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVQWSF1TN6TYEWQ3BJ61`

## Context

`Ulid::new()` re-randomises its low 80 bits on every call, so two ids created in the same millisecond sort arbitrarily.

## Decision

All ULIDs are minted from a single process-wide *monotonic* generator, never Ulid::new().

## Reasoning

Found by a test, not by reading: `Ulid::new()` re-randomises its low 80 bits every call, so two ids created in the same millisecond sort arbitrarily. SPEC §3.4 rests on ULID order *being* chronological order so that "changed since T" is a range scan over `events.id` — and a burst of writes inside one millisecond is an agent's normal behaviour, not an edge case. Non-monotonic ids would make an event-cursor query silently skip or repeat rows, which is the same class of quiet-wrong-answer bug as an inverted graph traversal. Rejected: ordering every query by `(created_at, id)`, which pushes the problem to every call site instead of solving it once.

## Consequences

Found by a test, not by reading.

## Reversible?

Yes.

