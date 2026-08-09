<!-- keel:generated decision dec_01KZKMPVSRVWSF4N42E9Y1M7A1 v1 2026-08-09T18:07:39Z
     source of truth is Keel — edits here are not saved -->
# Tool responses lift version to the top of the entity

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVSRVWSF4N42E9Y1M7A1`

## Context

`version` lives inside the audit block in the domain model.

## Decision

Surface it at the top of the entity on the wire, alongside the nested block.

## Reasoning

keel_update documents a `version` argument, so an agent that has just read an entity should be able to copy the field of that name straight across. Making it hunt inside `audit` is the papercut that becomes a 409 and a confused retry.

## Consequences

Found by writing the UC-3 test the way an agent would actually do it.

