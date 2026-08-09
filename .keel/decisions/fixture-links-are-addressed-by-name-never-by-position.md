<!-- keel:generated decision dec_01KZKMPVT876SD8CJJPGY9ZVXY v1 2026-08-09T18:07:39Z
     source of truth is Keel — edits here are not saved -->
# Fixture links are addressed by name, never by position

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVT876SD8CJJPGY9ZVXY`

## Context

Two Harbour feedback items ended up linked to a Keel spec.

## Decision

Look artifacts up by label; error if the label is missing.

## Reasoning

The link section used positional indices, and appending rows near the top of each list shifted every index below. The edges silently rewired themselves and nothing complained, because a link to the wrong artifact is still a valid link.

## Consequences

A renamed artifact now breaks the fixture loudly rather than quietly dropping an edge.

