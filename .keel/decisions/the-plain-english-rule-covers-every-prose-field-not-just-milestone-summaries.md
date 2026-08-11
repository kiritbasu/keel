<!-- keel:generated decision dec_01KZRMFKQARKG1K6NEW1MD5222 v1 2026-08-11T15:07:34Z
     source of truth is Keel — edits here are not saved -->
# B-46 — The plain-English rule covers every prose field, not just milestone summaries

**Status:** `accepted`  
**Id:** `dec_01KZRMFKQARKG1K6NEW1MD5222`

## Decision

Everything Keel stores as prose — decision bodies, question bodies, specs, feedback, notes, task bodies and summaries, titles — must read as though a person wrote it. The rule from B-45 is not specific to milestones and is now applied wherever prose enters the store.

KB's call, 2026-08-11, extending B-45 on the same day it was taken.

## What is actually enforceable, stated honestly

This is the part worth being straight about, because the request is larger than any validator can satisfy.

**Structure can be checked. Voice mostly cannot.** A rule can see that a field is empty, that it restates its own title, or that it cites `TQ-15` with nothing beside it. It cannot see that a sentence is limp, over-hedged or arranged for cadence rather than meaning. Anyone claiming otherwise is describing a check that will be wrong in both directions.

**A false rejection is worse than a mediocre sentence.** When a model is refused for a reason it does not accept, its recovery is to satisfy the letter of the rule — swapping a banned word for a synonym and keeping the same shape. That is worse than the original, because the prose is now both bad and rule-compliant, and the check reports success.

So the enforcement is deliberately split three ways.

## Three layers, by how reliably each one works

**1. The tool descriptions, which are the real mechanism.** Every prose-bearing field says what good looks like, with a worked good/bad pair. A model reads this at the moment of writing, on every surface, whether or not any skill loaded. This is the layer that changes what gets written, and it is the one with no false-positive cost.

**2. Rejection, for what is objectively wrong.** Empty or whitespace. A body that only restates its title — reusing the containment rule from KEEL-65 rather than inventing a similarity measure. A bare `TQ-15`, `B-44`, `KEEL-96` or `REQ-7` with no gloss, using the parser `fsck` already has. A short list of phrases with essentially no legitimate use in a project tracker. Each rejection names the span and what to write instead.

**3. Warning, for what is a signal rather than a rule.** Softer tells are reported alongside a successful write rather than blocking it. The write lands, and the session is told what read as machine-written. This teaches without the write-around failure mode.

## Quoted material is exempt

Fenced code, inline code and block quotes are stripped before any check. A note quoting an error message, a spec quoting a vendor's documentation, or a decision quoting what someone actually said is carrying someone else's words, and refusing those would make the store unable to record the world as it is.

## Consequences

- One `style` module in `keel-core`, so the CLI, MCP and `keel import` cannot diverge on what is acceptable.
- The SessionStart hook states the house rule in one line, since that is the channel measured to reach the model — thirty gate sessions invoked the skill zero times.
- The existing rows are not rewritten. A machine inventing replacement prose would produce exactly the confident, plausible, wrong text this rule exists to prevent, which is the same reasoning that stops the mirror ever reading a file back. `keel lint` reports them.
- This does not settle TQ-34, which asks whether a task `summary` is required at all. This decides how prose is judged once written, not which fields must exist.

