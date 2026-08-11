Read `PHASE-8.md` in the repository root. It is the specification for the next phase of work, written in plain English. Read it in full before doing anything.

Then follow this repository's standing session ritual in `product/CLAUDE.md` — read `product/STATUS.md`, check `git log --oneline -15`, and say in one line what you are picking up.

## First, get the phase into Keel

Everything in this repository is meant to live in Keel and be generated back out. So before any code:

1. `keel import PHASE-8.md --project keel` — the spec becomes a versioned document rather than a loose file. Do the same for `PHASE-9.md` and `PHASE-10.md` so all three are in the store; they are not being worked on yet, but they are the roadmap and they belong in it.
2. Create the **Phase 8 — The working loop** milestone.
3. Create a task row for every unit of work in the spec, each `implements` the imported spec, each assigned to the Phase 8 milestone, each with a real priority, and each with the blocking links that the spec's ordering implies. Where the spec names sub-parts of one piece of work — §8C has eight — use the parent/child relationship rather than eight unrelated rows.
4. `keel generate keel`, then commit. Nothing has been built yet and the tracker should already show the whole phase.

Do not start coding until the board reads as a plan you could hand to someone else.

## Then work it in this order

**8C first** — the app made legible. C1 navigation, C2 the Library split, C3 time, C7 the milestone on every row are the four that change KB's day most; C0 the design pass underpins them. The design is not a description: `keel-design-system.html`, `keel-screens.html` and `keel-tokens.css` are attached to the spec, and the two font files go in `apps/desktop/public/fonts/`. Start by dropping in the token layer and the fonts, because everything else in 8C reads against them.

**Then 8A** — the three verbs and the triage status. **Then 8G** — required summaries. **Then 8B** — intake and attachments. **Then 8F** — the project's own words. **Then 8E** — the small things.

## Five things need KB's decision before they are built

They are listed at the end of the spec. Three of them gate work in 8A and 8G, so raise them early — write each as a question artifact in Keel with the options and your recommendation, tell KB in one line, and get on with something that is not blocked. Do not decide them yourself:

1. Twelve MCP tools instead of ten.
2. A `triage` status — a seventh value on the task status enum.
3. Whether the Activity screen is reworked or deleted.
4. Whether `keel_attach` may fetch a URL at all.
5. A required, validated `summary` field on tasks.

## Hold to the repository's own rules

- **Claim before you start.** Move a task to `in_progress` before working it, not after. Across 66 tasks there were once zero transitions into that state; do not add to that.
- **Record what you find as a note on the task**, not as a line in a markdown table. A status without the finding behind it is a colour, not information.
- **The definition of done in `product/CLAUDE.md` applies to every row** — clippy clean with `-D warnings`, formatted, tests including at least one failure case, no `unwrap`/`expect`/`panic` in library code, the task row updated in Keel, `keel generate keel` run, and committed.
- **Ask before touching** storage format, the MCP tool surface, phase order, or anything in SPEC §13. Decide yourself on anything reversible and record it as a decision.
- **Tell KB early if something in the spec is wrong or unbuildable.** He wants to know at the point it is cheap. Parts of this were written from outside the codebase and some of it will not survive contact.

Close with a two-line summary: what landed, and what is next.
