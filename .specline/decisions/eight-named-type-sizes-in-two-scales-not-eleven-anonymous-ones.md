<!-- specline:generated decision dec_01KZNHQ6BNEH54ZG8HQ7WRR2S5 v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-35 — Eight named type sizes in two scales, not eleven anonymous ones

**Status:** `accepted`  
**Id:** `dec_01KZNHQ6BNEH54ZG8HQ7WRR2S5`

## Context

The app used eleven ad hoc pixel sizes with no names — `text-[15px]`, `text-[12.5px]`, `text-[10px]` and so on — so no two screens agreed on what a label or a heading was.

## Decision

**Six named steps for the interface: `display`, `title`, `heading`, `body`, `small`, `micro`. Two more for rendered document bodies only: `doc-title` and `doc-section`.** All eight are Tailwind theme tokens in `styles.css`. No raw pixel size survives anywhere in `src/`.

## Reasoning

A name is what makes a size a decision rather than a guess: `text-micro` says "this is metadata", `text-[10px]` says nothing.

The two extra steps are an honest exception rather than a leak. A rendered spec has its own heading hierarchy and needs steps above `text-title`, which the interface never uses; forcing an article and a toolbar onto one scale would flatten the article. Naming them keeps the count truthful — eight sizes, every one a decision, in two clearly separated scales — instead of quietly reintroducing anonymous values inside the markdown renderer.

12px folded into 13 and 10px into 11. Nothing was lost that a reader can see.

## Reversible?

Yes. One file.

