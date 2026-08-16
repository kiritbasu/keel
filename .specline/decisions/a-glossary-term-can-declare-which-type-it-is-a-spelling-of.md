<!-- specline:generated decision dec_01KZS6ZARDDED3P4GF3X8QF9E7 v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-50 — A glossary term can declare which type it is a spelling of

**Status:** `accepted`  
**Id:** `dec_01KZS6ZARDDED3P4GF3X8QF9E7`

KEEL-116 made `keel_create(type: "phase")` work by adding a fixed list of aliases in the source. That closed §8F's exit criterion and left the general problem open: every project's vocabulary had to be anticipated by whoever wrote the list, and a project saying "incident" for a task was out of luck until somebody shipped a binary.

`Term` gains a `means` column, holding one of the thirteen types. A term that declares one is consulted before the built-in list.

## Why a column and not the definition

The task this came from proposed having the alias table "consult the glossary", and the obvious reading is to parse the type out of a term's definition — the prose is right there. It does not survive contact with real definitions: "a phase is a milestone with a demo at the end" and "a phase is not a milestone" mention the same word and mean opposite things, and a rule that read either would resolve them identically.

A declaration cannot be misread. And because it is an `EntityType` rather than a string, the type system enforces the thing that actually matters here.

## The rule this lives under

**A term declares a spelling, never a concept.** This is the feature most able to break the thirteen-type ceiling, because for the first time a *stored row* can introduce vocabulary — and a row is written by whoever is using Keel rather than by whoever reviews a pull request. Making `means` an `EntityType` makes a fourteenth type unrepresentable, which is the same move TQ-31's alias table made and for the same reason. A test asserts it across every word the glossary knows.

## The resolution order, and why each step is where it is

1. **The canonical name.** Nothing can shadow it — a project defining a term called "task" that means a decision gets a task, because `keel_create(type: "task")` has to mean the same thing in every project forever. Tested.
2. **This project's glossary, then the global one.** Project-first is Q-4's existing rule for terms, and it applies here for the same reason. Another project's term never applies: a word meaning one thing here and another there is precisely what project scoping is for.
3. **The project's own `milestone_noun`.** Not redundant with the glossary even though setting the noun seeds a term, because the noun is what the *interface* says: a project whose board reads "Phase 8" should accept "phase" on input whether or not anybody kept the term in step.
4. **Keel's built-in list**, which is where KEEL-116 stopped.

## The display noun

`milestone_noun` on projects is a label and never changes what is stored. The tracker now writes "Active phase", "## Phases" and a "Phase" column header; the board's filter says "Phase" and "Any phase"; the digest's first paragraph says "active phase: Phase 8".

A noun that is another type's name is refused at the point somebody sets it. A project calling milestones "tasks" would make every `keel_create(type: "task")` ambiguous, and the resolution order *hides* that rather than surfacing it — the canonical name wins, so the noun would silently do nothing.

## Narrating why, not just what

KEEL-116 established that resolution is narrated: a session told "you said 'sprint' — in Keel that is a milestone" learns the vocabulary in one round trip. This adds the reason. "Because this project's glossary says so" tells a session where the vocabulary lives; "because Keel recognises that word" tells it the word is universal. The difference is actionable.

