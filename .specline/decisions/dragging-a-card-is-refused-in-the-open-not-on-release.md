<!-- specline:generated decision dec_01M0B35ABXXTCYT8MS8TQRJ2EA v1 2026-08-18T18:47:54Z
     source of truth is Specline — edits here are not saved -->
# B-88 — Dragging a card is refused in the open, not on release

**Status:** `accepted`  
**Id:** `dec_01M0B35ABXXTCYT8MS8TQRJ2EA`

#### Decision

A card can be dragged between board columns when the board is grouped by status. Three of the six columns do not simply take it, and **each says so while the card is still in the air** rather than on release:

- **`done` and `wont_do`** open the Close form on drop.
- **`in_progress`** and **`blocked`** are not drop targets at all, and each prints its reason in the column for as long as the drag lasts.

`dropOnStatus` in `lib/tasks.ts` is the one place that decides. The board asks it what to *show*; the drop handler asks it again to decide what to *write*.

#### Why the refusals are shown rather than discovered

A drop target that quietly does nothing is indistinguishable from a broken app. That is the whole reason these rules were worth having in the first place — the point of refusing `in_progress` is that a claim records who, and a refusal that does not say so teaches nobody anything and just looks like a bug.

So the reason appears in the column at `dragstart`, in every column that would refuse, and disappears at `dragend`. It costs two lines of text on screen for the second or two a drag lasts.

#### Why `blocked` is refused too

It is not a status. The column is derived — something links to this task with `blocks` — so there is nothing a drop could set (TQ-25). It is the same argument that kept `blocked` out of `TaskStatus`, and it would have been easy to miss because the column looks exactly like the others.

#### The case that needed a sentence rather than a rule

A card that *is* blocked can still be dragged out of the blocked column onto `todo` or `review`, and the write succeeds — its status really does change. But the card does not move, because the blocked column is derived and comes first. Nothing appears to have happened.

Refusing the drag would have been the tidier rule and the wrong one: the status change is legitimate and sometimes wanted. So the board does it and then says what it did — *"Moved to review. It stays under Blocked while something blocks it."* This is the only place the board explains an outcome rather than showing it, and that is because it is the only place where showing it is impossible.

#### Plain HTML5 drag and drop, no library

One gesture on one screen. The scale rule in the contract asks for a measurement before a dependency, and there is none to offer.

The cost is real and worth stating: **dragging is a pointer gesture only.** There is no keyboard equivalent, and the accessible route to a status is the select on the task screen that B-87 added. The cards here stay ordinary focusable links and nothing on the board steals a key, so the board is no less usable from a keyboard than it was — it is simply not *more* usable, and a keyboard reordering affordance is still unbuilt.

#### Grouped by anything else, cards do not drag

Grouped by label a card legitimately sits in three columns at once, so a drop has no meaning. Grouped by phase it would have one, and would be a useful thing to add; it is not what KEEL-308 asked for and is not smuggled in.

