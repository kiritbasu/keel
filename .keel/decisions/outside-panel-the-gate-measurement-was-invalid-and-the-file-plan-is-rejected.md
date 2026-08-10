<!-- keel:generated decision dec_01KZMGPPJ0MM4VSGAP4KF724DQ v1 2026-08-10T00:22:49Z
     source of truth is Keel — edits here are not saved -->
# Outside panel: the gate measurement was invalid and the file plan is rejected

**Status:** `proposed`  
**Id:** `dec_01KZMGPPJ0MM4VSGAP4KF724DQ`

Six-expert panel review with adversarial cross-examination, delivered as `product/WAY-FORWARD.md`, 2026-08-09.

**The headline finding, and it is correct.** The gate harness is headless single-turn — `claude -p ... </dev/null`, one prompt, one response, process exits. Five silent sessions ended with *"I'll hold off until you say go."* There was no "you" and no next turn. The write was not refused; it was scheduled for a turn the harness architecturally could not supply. I recorded that caveat for run 1 and then dropped it, and every strategic conclusion since — including "the premise may be dead" — was drawn as though 3/10 measured judgement.

**The statistical point is also right.** 9-of-10 at n=10 has a 95% CI of [0.555, 0.997]; two projects means effective n≈5. The gate cannot distinguish a 55% agent from a 100% agent. It was never a usable instrument.

**The cause I missed, sitting in the highest-traffic text in the system.** `keel_create`'s description ends with *"confirm with the human"* and *"worse than useless"*; `keel_projects` says *"ask the human before creating anything"*. Tool descriptions are the only text re-read every session in every environment — unlike a skill (never loaded, proven) or a hook preamble (weak directive force). The anti-write instruction is inside the write tool, and `requires_confirmation` is tool *output* arriving at decision time, which is the strongest channel that exists.

**My file-based plan is rejected, on grounds I should have seen.** `product/STATUS.md` is one spec artifact — the whole forty-row tracker. An agent adding a task row via the PostToolUse hook writes revision N+1 of that blob, creates zero task artifacts, and `generate --check` passes. That is bit-for-bit the incident that lost 16 of 28 questions, promoted to the default write path and executed on every edit. It also reduces surface coverage (chat and Cowork have no filesystem, no hook, no localhost daemon) and turns a silent non-write on those surfaces into silent data loss.

**Adopted from it:** collapse the four model-facing write tools into one `keel_record`; rewrite the description with "call this without asking" in the first three lines; teach reversibility through the write's own output; auto-create the first project for a directory; a deterministic Stop hook with no model call; and a `class` column making the prose-blob failure unrepresentable rather than merely detectable.

**Process rule adopted unanimously, and it indicts the build order:** no phase may be sequenced ahead of a phase that tests an assumption it depends on. 305 tests and a nine-relation typed graph for a store holding 29 links is the signature of ordering work by what was buildable rather than by what was uncertain.

Full document at `product/WAY-FORWARD.md`. It is not yet in Keel — it arrived as a repo file and should be imported.

