<!-- keel:generated decision dec_01KZKWMSYT6WTETRJ6DF82A42E v2 2026-08-10T20:25:03Z
     source of truth is Keel — edits here are not saved -->
# B-5 — unwrap/expect/panic/todo/unimplemented are workspace clippy lints, promoted to errors by CI

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKWMSYT6WTETRJ6DF82A42E`

## Decision

unwrap/expect/panic/todo/unimplemented are workspace clippy lints at warn, promoted to errors by CI's -D warnings.

## Reasoning

The definition of done forbids these in library code. Encoding it as a lint means CI catches it; leaving it to review discipline means it lands. Tests and binaries opt out locally with `#[allow]` where genuinely warranted.

## Reversible?

Yes.

