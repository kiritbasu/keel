<!-- keel:generated decision dec_01M04DBTX99VPTD5X477XWEM9F v1 2026-08-16T04:32:08Z
     source of truth is Keel — edits here are not saved -->
# B-78 — Hard constraint 7 is rewritten: the interface writes what a person does, and Claude keeps the reasoning

**Status:** `accepted`  
**Id:** `dec_01M04DBTX99VPTD5X477XWEM9F`

## Decision

Hard constraint 7 stops saying "the desktop app is read-only" and says what is actually true and where this is going:

> **The interface may write what a person does; Claude keeps what a person reasons.** Creating a task, commenting on one, archiving or closing a row, moving a status or a priority — these are a person's own actions and the interface performs them, through `keel-core`'s write path, attributed `actor: human`, `surface: ui`. Authoring is what it does not do yet: the body of a spec, a decision or a question is written by Claude, because the reasoning in it is the product rather than a field on a form.
>
> Every write from the interface carries the daemon's token (B-78's sibling, KEEL-238), which is what makes "a person clicked it" distinguishable from "a page did it".

## Why the old sentence had to go rather than be amended again

It has been amended twice — B-75 for applying a staged update from the interface, B-77 for the CLI's half — and KEEL-240 would have been the third. KB, asked how far it should move: *"we do need to rethink those constraints, over time we will need to be author inside the ui."*

A constraint with three exceptions and a stated intention to go further is not doing a constraint's job. Its job is to stop somebody building the wrong thing by accident, and that requires a reader who believes it. Nobody believes the fourth exception.

## What is preserved, and it is the important half

The original was never about forms being distasteful. It was that **the reasoning is the product**. Keel exists because the thinking behind a project — why this, why not that, what was tried — is the part that normally evaporates, and the bet is that an agent in the conversation where that thinking happens is the only thing that will ever write it down. A person typing into fields produces a tracker with an AI feature attached, which is the thing Keel is trying not to be.

So the line is not "no writes". It is **capture versus authoring**. Archiving a stale row is capture. Writing the paragraph that says why a decision went the way it did is authoring, and putting that behind a textarea would quietly change what the product is.

## Why this is a line and not a permanent one

KB has said authoring reaches the interface eventually, and this decision does not pretend otherwise. What it refuses is writing "never authors" into the contract as a principle when it is already known to be temporary — that is how the previous sentence became something people had to read three exceptions past.

When authoring does arrive, the question to answer first is not "can we build a form" but "what stops the reasoning becoming a field somebody fills in because the form asked". That is the argument this constraint exists to force, and it should survive the rewrite.

## The test that keeps it honest

An endpoint that accepts a document revision is on the wrong side of the line. That is checkable, and it is what to look for when reviewing a change that claims to be within this.

