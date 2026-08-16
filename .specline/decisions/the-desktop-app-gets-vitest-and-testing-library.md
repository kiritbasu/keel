<!-- specline:generated decision dec_01KZNHQHCJ7D50QAY8738NNF3A v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-37 — The desktop app gets Vitest and Testing Library

**Status:** `accepted`  
**Id:** `dec_01KZNHQHCJ7D50QAY8738NNF3A`

## Context

The definition of done requires tests, including at least one failure case. The desktop app had no test runner at all — every test in the repository was Rust.

## Decision

**Vitest with jsdom and `@testing-library/react`, configured inside the existing `vite.config.ts`.** `npm test` in `apps/desktop`.

## Reasoning

Vitest reuses the Vite config the app already has, so the transform pipeline under test is the one that ships — a separate Jest setup would mean a second TypeScript and JSX configuration that can drift from the real one.

jsdom rather than a real browser: what needs testing here is routing, ranking and keyboard handling, none of which needs a compositor. The parts that do need one — layout, the light theme — are not things a unit test would have caught anyway, and were checked in a browser instead.

One patch to the environment, in `src/test-setup.ts`: jsdom has no `Element.prototype.scrollIntoView`. Stubbing it there keeps the guard out of product code, where it would have been test scaffolding shipped to users.

## Reversible?

Yes, though there is now a test suite that would have to move with it.

