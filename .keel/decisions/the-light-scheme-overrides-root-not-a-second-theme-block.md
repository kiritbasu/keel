<!-- keel:generated decision dec_01KZNHQCRB7PYBKW0Q37P4VFVK v1 2026-08-10T18:53:23Z
     source of truth is Keel — edits here are not saved -->
# B-36 — The light scheme overrides :root, not a second @theme block

**Status:** `accepted`  
**Id:** `dec_01KZNHQCRB7PYBKW0Q37P4VFVK`

## Context

The light scheme was declared as a second `@theme` block nested inside `@media (prefers-color-scheme: light)`, and it overrode only the surfaces, the ink and the accent. `good`, `warn` and `bad` kept their dark-tuned values.

## Decision

**Declare the palette once in a top-level `@theme`, and override the custom properties on `:root` inside the media query — including the three status colours.**

## Reasoning

Tailwind v4 resolves every colour utility through `var(--color-…)`, so an override on `:root` reaches utilities that were generated once. A nested `@theme` is not the documented arrangement and depends on where the block lands in the cascade.

The status colours mattered more than the mechanism. A hue tuned to sit on a near-black surface at 0.74–0.80 lightness is close to invisible on a near-white one: in light mode "done" and "blocked" both read as pale smudges, and a colour system that cannot be told apart is not carrying information. Light values are 0.52–0.55 lightness at similar or higher chroma.

## Reversible?

Yes. One file.

