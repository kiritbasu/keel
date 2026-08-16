<!-- specline:generated decision dec_01KZKMPVSRVWSF4N42E9Y1M7A1 v2 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-13 — Tool responses lift version to the top of the entity

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVSRVWSF4N42E9Y1M7A1`

## Context

`version` lives inside the audit block in the domain model.

## Decision

Tool responses lift version (and archived) to the top of the entity, alongside the nested audit block.

## Reasoning

`version` lives inside `audit` in the domain model, which is right there and wrong on the wire: `keel_update` documents a `version` argument, so an agent that has just read an entity should be able to copy the field of that name straight across. Making it hunt inside `audit` is the papercut that becomes a 409 and a confused retry. Found by writing the UC-3 handoff test as an agent would actually do it. The audit block is untouched — this adds a field rather than moving one.

## Consequences

Found by writing the UC-3 test the way an agent would actually do it.

## Reversible?

Yes.

