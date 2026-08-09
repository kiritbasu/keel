<!-- keel:generated decision dec_01KZKWMSYT6WTETRJ6DF82A42E v1 2026-08-09T18:32:09Z
     source of truth is Keel — edits here are not saved -->
# unwrap/expect/panic/todo/unimplemented are workspace clippy lints at warn, promoted to…

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKWMSYT6WTETRJ6DF82A42E`

`B-5` · 2026-08-09

**Decision.** **`unwrap`/`expect`/`panic`/`todo`/`unimplemented` are workspace clippy lints at `warn`, promoted to errors by CI's `-D warnings`.**

**Reasoning.** The definition of done forbids these in library code. Encoding it as a lint means CI catches it; leaving it to review discipline means it lands. Tests and binaries opt out locally with `#[allow]` where genuinely warranted.

**Reversible?** yes

