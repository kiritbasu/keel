<!-- specline:generated decision dec_01M0AYBAJEBJA2Z3Y9D0BZHBDH v1 2026-08-18T17:25:53Z
     source of truth is Specline — edits here are not saved -->
# B-86 — The label box creates labels, and normalises what it creates

**Status:** `accepted`  
**Id:** `dec_01M0AYBAJEBJA2Z3Y9D0BZHBDH`

#### Decision

The New Task dialog's label box creates a label that does not exist yet. What it creates is **normalised** — trimmed, lowercased, whitespace runs folded to a single hyphen, repeated hyphens collapsed, leading and trailing hyphens dropped — and a candidate that normalises onto a label already in use is not offered as new; the existing one is offered instead.

The normalisation lives **only in the picker**. `specline-core`, MCP and the CLI still take a label exactly as given.

#### What this reverses

KEEL-246 shipped the picker with no create affordance, deliberately, and said so at length in the component's own doc comment:

> A free-text label box is how a set becomes `ui`, `UI` and `ui ` inside a month, and nothing downstream can tell those apart — the board's facets, the filters and `specline_next` all treat them as three labels.

That reasoning was right about the failure and wrong about the remedy. Refusing sends a person out of the dialog and into a conversation to obtain a one-word tag, which costs more than the fragmentation it prevents. KB, filing KEEL-304: *"it should automatically add the label to the main list of labels so that it can be autocompleted the next time"*.

Normalising handles the failure directly instead of by abstinence. `Data Safety`, `DATA-SAFETY` and `data safety ` all land on `data-safety`, and because the same fold is applied to the existing set before comparing, typing any of them finds the label that is already there rather than offering to make a fourth.

#### Why the rule is this rule and not a stricter one

All 75 labels in use are already lowercase and hyphenated. The rule codifies the set rather than imposing on it, so nothing needed migrating and nothing already filed changed meaning. Punctuation is deliberately left alone: the rule exists to stop case and spacing splitting one label into three, and stripping anything else would be inventing policy the label set never asked for.

#### Why the store is not normalised too

Two other places could have carried the rule, and both were rejected.

Normalising in `specline-core` means a caller asking for `Phase10` gets `phase10` back with no explanation. That is the silent-correction shape this codebase keeps having to undo — B-56 chose to tell the caller what is true over quietly adjusting, and the same argument applies here.

Rejecting a non-normalised label in `specline-core` avoids the silence but is a wider change than the problem justifies, and it can break an MCP call that works today. Claude can see the existing label set on every read and matches it; the box is for the person who cannot.

So the fold is a property of the typing surface, not of the store. If labels ever fragment from the MCP side, that is the point to revisit this — and the evidence will be visible in the label facet rather than inferred.

#### The part that needed no code

"So it can be autocompleted the next time" needs nothing. There is no label registry: the picker's `available` list is derived from the labels the loaded tasks carry, and the dialog already reloads the board on create. A label exists exactly as long as something is tagged with it, which is also why an unused label disappears on its own.

#### What this leaves open

Labels still cannot be changed once a task exists — the task screen renders them read-only, so the picker is reachable only during creation. Filed as KEEL-307.

