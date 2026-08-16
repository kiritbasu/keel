<!-- keel:generated decision dec_01KZYC1V6NV3H9EVNPVAHEECRJ v1 2026-08-13T20:12:32Z
     source of truth is Keel — edits here are not saved -->
# B-62 — Spec decisions that outlive their reasoning are annotated, not rewritten

**Status:** `accepted`  
**Id:** `dec_01KZYC1V6NV3H9EVNPVAHEECRJ`

Resolves TQ-37, 2026-08-13. KB chose annotation, which was the recorded recommendation.

Six rows of SPEC §13 — D-1a, D-2, D-2b, D-4, D-5 and D-6 — argued from DuckDB and Lance, which left the tree in Phase 9. Every one of them still reaches the right conclusion; what expired was the reasoning underneath. Each row now keeps the rationale it was decided on, with the dead clause struck through and what replaced it named beside it, in the pattern D-1 already set.

Not rewritten, and the reason is D-4. It chose recursive CTEs over DuckPGQ because DuckPGQ could not run on DuckDB 1.5.x alongside Lance — a constraint that no longer exists in a tree containing neither. Then the Phase 9 survey ruled out Turso for not supporting recursive CTEs at all. The conclusion did not merely survive its rationale being replaced, it got stronger. Rewrite the row to argue from SQLite and that disappears; a reader learns what is true and not that the decision was load-bearing for a reason nobody anticipated.

D-6 is the one worth being blunt in. "Storage engines are Rust-native" is false — SQLite is a C amalgamation compiled into the binary and the embedding path reaches ONNX Runtime, which is C++. The property actually wanted was nothing to install and nothing running beside the binary, which §2 now argues directly rather than through the language. Saying that plainly is better than deleting the sentence and leaving the conclusion looking unexamined.

The standing note under the table changed with them. It used to say the rows were left alone pending KB's agreement, which stopped being true the moment the agreement arrived.

Rewriting the rationales was rejected for the reason KEEL-132 was told not to: it would make the spec read as though it had always said SQLite. Leaving them was rejected because a reader who reads only the table is misinformed, and the blockquote that was carrying the correction is easy to miss.

