<!-- specline:generated decision dec_01M0D0KX0ZGSDTSQE1JPG5P8ZY v1 2026-08-19T12:41:04Z
     source of truth is Specline — edits here are not saved -->
# B-93 — Phases and releases get a screen each, and the roadmap groups by state

**Status:** `accepted`  
**Id:** `dec_01M0D0KX0ZGSDTSQE1JPG5P8ZY`

The Roadmap is phases. Releases is its own screen, sixth in the rail.

## Why

KB, looking at the roadmap after releases were added to it as a second section:

> the phases and releases are 2 orthogonal items, maybe they should be in 2 different tabs?

He is right, and the diagnosis is sharper than the one I had. A phase is a unit of **plan**: named ahead of time, holds tasks, has progress. A release is a unit of **record**: a version that went out on a date, holds nothing. They share the `milestones` table and nothing else. Putting them on one page implies a relationship neither has to the other, and stacking them in two sections does not fix that — two lists on one page still read as one page about one thing.

There was a second problem underneath it. A release has no tasks, so on a screen whose right-hand column is task progress it could only ever render as "not scoped". They had been given a phase's clothes.

Four directions were drawn against the real app and compared on a canvas: two tabs, two screens, a split view with releases as a dated rail, and leaving them adjacent but demoting the release rows to a table. KB picked two screens.

Two screens over tabs because a tab is a place things hide, and because each screen gets a title that says what it is. The cost is a tenth item in the rail, which shifted four keyboard shortcuts down by one — Library to 7, and What changed to 0. That was taken deliberately: the alternative was the only unnumbered row in a numbered rail, which is wrong every day rather than once.

## The roadmap groups by state now

`sort_order` gives the list the order somebody typed. It does not answer "where is this project now", which is what the screen is for — and fifteen phases in plan order buried the three that were moving in the middle of the twelve that were not. The groups are: in flight, finished-not-yet-declared, planned, shipped, set aside. The manual order still holds inside a group.

`complete` gets a heading of its own rather than being folded in with `shipped`, because the difference is the whole of B-57: every task closed is derivable, and "it shipped" is a declaration only a person can make. Three of this project's phases sat in that state unnoticed until the digest grew a section for it; now the screen says so too, with a line telling you what to do about it.

Anything whose state matches no group still renders, under "Everything else". A phase missing from the one screen whose job is to list them is the failure this screen cannot afford, and a new value in the enum would otherwise cause it silently.

## Two things reversed from the first attempt

**Descriptions are shown in full on every phase, finished ones included.** They were briefly clamped to one line to keep the page short. KB asked for them back whole, and he is right: the summary is the sentence saying what the phase was *for*, and a roadmap of fifteen bare names answers that only for whoever wrote them. Grouping had already done most of the work the clamp was trying to do.

**Releases are a table, not cards.** Ten versions of one product differ in their version and their date and almost nothing else, so the useful shape is a column of versions you can run your eye down. Newest first, which is the opposite of the roadmap and deliberate: a plan is read forwards from where you are, a changelog backwards from now.

## What this leaves

`product/STATUS.md` already rendered Phases and Released as separate tables, so the file and the app now agree without further work.

One ordering rule is stated in two places and must stay a mirror rather than a second opinion: an uncut version sorts *last* in the tracker's oldest-first table and *first* on the newest-first screen. Both have a test naming the other.

