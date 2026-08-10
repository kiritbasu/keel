<!-- keel:generated decision dec_01KZKMPVTSVQB53R5AGXMB5WZ5 v2 2026-08-10T18:53:24Z
     source of truth is Keel — edits here are not saved -->
# B-14 — The desktop app hand-writes its components

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVTSVQB53R5AGXMB5WZ5`

## Decision

The desktop app hand-writes its components rather than using shadcn/ui's generator.

## Reasoning

SPEC §10 names shadcn/ui. What a read-only, seven-screen app actually needs from it is a card, a badge, a status colour and an empty state — four small components. Running the generator to obtain those pulls in Radix primitives and a registry dependency for a surface with no dialogs, popovers or focus traps to manage. The conventions are kept: one component per concern, styling through class names, no theme provider, Tailwind 4 tokens in one place. Total frontend dependency footprint is 81 packages and a 227 KB bundle. Revisit the moment the app grows anything genuinely interactive.

## Consequences

81 packages, a 227 KB bundle. Revisit the moment the app grows anything genuinely interactive.

## Reversible?

Yes.

