<!-- keel:generated decision dec_01KZXMFZH4V5TDJGPN96B1WBJ2 v1 2026-08-13T13:22:02Z
     source of truth is Keel — edits here are not saved -->
# B-59 — A changed model is an ordinary re-embed, because search refuses to mix models at all

**Status:** `accepted`  
**Id:** `dec_01KZXMFZH4V5TDJGPN96B1WBJ2`

Resolves TQ-3, 2026-08-13, which asked whether re-embedding after a model change should be a background full pass or lazy on access.

Neither, and the question turned out to be resting on a bug.

**The bug first.** The only guard on the vector scan was `length(embedding) = ?`, which catches a model that changed *dimension*. It cannot catch one that did not. Two 384-wide models produce vectors in unrelated spaces; the cosine between one model's document vector and another's query vector is a perfectly well-formed number that sorts into a plausible ranking and means nothing. Swapping `bge-small-en-v1.5` for any other 384-dimension model would have done this silently, and no strategy for *when* to re-embed would have helped, because the corpus is mixed for the whole duration of any strategy.

So the semantic query filters on `embedding_model` as well as width, and the embedder is now required even when the query vector was computed elsewhere — it is what names the model, and without the name there is no way to know which stored vectors this one may be compared against. No embedder means no semantic results rather than a guess.

**Then the strategy, which mostly dissolves.** "Missing" is redefined as *has no passages from the model now configured*. Changing the model makes every live document missing, and `keel reembed --missing` — the command that already exists, for the case that already existed — rebuilds them. One definition, one command.

A **background full pass** was rejected for the reasons scale discipline gives: it is a background worker, it writes on a schedule nobody asked for, it competes for the single write path, and it makes the first start after an upgrade slow and surprising. There is one user and a few hundred documents; the pass takes 29 seconds and a person can run it.

**Lazy on access** was rejected because it writes during a read, and because it leaves the corpus permanently half in one space and half in another — with the model filter now in place, that means recall silently depends on what happened to be searched recently, which is the least predictable failure of the three.

What makes the explicit choice safe is that nothing is silent. `passages_from_mixed_models` in `fsck` and `passage_index` in `doctor` both report a split corpus with the remedy, and the model filter means the stragglers are *absent* from results rather than wrong in them. The failure mode is missing rows that something is complaining about, not present rows nobody can question.

One consequence worth stating: between changing the model and finishing the pass, semantic search returns only what the new model has embedded, and the keyword half carries the rest. That is a visible, temporary degradation with a command that ends it, which is the trade this project makes every time.

