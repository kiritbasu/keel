<!-- keel:generated decision dec_01KZR4KZQ8BXFXA1PRFTXEPE07 v2 2026-08-11T10:22:41Z
     source of truth is Keel — edits here are not saved -->
# B-45 — Every milestone carries a plain-English explainer, required at creation

**Status:** `accepted`  
**Id:** `dec_01KZR4KZQ8BXFXA1PRFTXEPE07`

## Decision

A milestone cannot be created without a short plain-English summary of what the phase covers. `keel_create(type: "milestone")` requires it and refuses a create without one, naming what was missing and what would be valid.

KB's call, 2026-08-11, on seeing Phase 8 appear on the roadmap with no description.

## What "plain English" means here

KB set the standard in the same conversation, and it is the harder half of this decision. The summary must read like a person wrote it:

- **One or two sentences.** The existing summaries are 8 to 15 words — "Deployable daemon, auth, mobile client." A paragraph is too long.
- **Say what the phase does for the reader**, in the words they would use. Not the section numbers, not the internal names.
- **No AI register.** No em-dash asides, no "genuinely" or "deliberately" or "rather than", no rule-of-three lists, no "not X but Y", no sentence that exists to sound considered.

The first attempt at Phase 8 failed this: five clauses, six section references, and the phrase "constitute doing work rather than describing it". It was replaced with "Make the everyday loop work: file a bug in seconds, see what's ready to start, and read the board without opening every card."

This is a house style rule, not only a milestone rule. It applies to any prose the product puts in front of a human.

## Context

The trigger was a silent drop, not just an omission. `keel_create` takes a `body` argument for every type. `build_entity` (crates/keel-mcp/src/dispatch.rs:1070) routes it into `description` for a project and `body` for a task, and for a milestone it calls `Milestone::new(project, name)` and ignores `body` completely. `Milestone.summary` exists (crates/keel-core/src/types.rs:307) and nothing on the create path writes it.

So a session that supplies a description gets a success, no warning, and no description. An input that is accepted and thrown away is worse than one that is refused, because the caller has no way to find out.

The data shows it. Phases 0 to 5 were created by `keel bootstrap`, which builds the struct directly and sets the summary; all six have one. Phase 6 and Phase 7 were created through `keel_create` over MCP, and both are empty.

## Reasoning

The roadmap answers "what is this project doing, and in what order". A phase whose row is a bare name answers that only for whoever wrote it. There are eight milestones and ninety-nine tasks, so the milestone is the unit a human actually reads and an unreadable one costs more per row.

Requiring it at the tool boundary follows what this project has already measured. Skills do not fire unprompted — thirty gate sessions, zero invocations. A rejection at the tool boundary was recovered from in the same turn by both sessions that hit one. A required property is confronted on every call, on every surface, whether or not anything else loaded.

## Consequences

- `Milestone::new` takes the summary as a required argument, so the compiler finds every call site.
- `keel_create` declares it required for milestones and refuses a missing or empty one.
- Phase 6 and Phase 7 get summaries written from what they shipped.
- The style rule above goes in the tool description with a good and a bad example, the way §8G proposes for task summaries. A length ceiling is enforceable; the register is not, so the description has to carry it.
- Narrower than TQ-34, which asks the same of tasks and is still open. Eight milestones is a small, unambiguous set. Deciding milestones does not decide tasks.

