<!-- keel:generated decision dec_01KZKMPVTSVQB53R5AGXMB5WZ5 v1 2026-08-09T18:07:39Z
     source of truth is Keel — edits here are not saved -->
# The desktop app hand-writes its components

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVTSVQB53R5AGXMB5WZ5`

## Decision

No shadcn/ui generator.

## Reasoning

What a read-only, seven-screen app needs is a card, a badge, a status colour and an empty state. Running a generator to obtain those pulls in Radix primitives for a surface with no dialogs or focus traps to manage.

## Consequences

81 packages, a 227 KB bundle. Revisit the moment the app grows anything genuinely interactive.

