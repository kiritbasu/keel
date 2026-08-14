<!-- keel:generated decision dec_01M010PZ4GM1Q2NS41KPJJEZAS v1 2026-08-14T20:52:32Z
     source of truth is Keel — edits here are not saved -->
# B-71 — The installer refuses a download it cannot verify, rather than skipping the check

**Status:** `accepted`  
**Id:** `dec_01M010PZ4GM1Q2NS41KPJJEZAS`

## Decision

`scripts/patch-installer.sh` rewrites the sha256 block in the installer `dist` generates. It tries `sha256sum`, falls back to `shasum -a 256`, and **errors** when neither is on the path. Upstream returns success in that last case.

The release workflow runs it after building the installer and before attesting, so the provenance statement covers the bytes people download.

## The correction PHASE-10 needs

§10 says stock macOS has no `sha256sum`. Measured on 2026-08-14 that is half right, and the half that is wrong changes where the bug bites.

This machine ships `/sbin/sha256sum` — an Apple binary, universal with an arm64e slice, dated June 2026. So on a current macOS with the default path the check does run.

It skips everywhere else on macOS:

- Older macOS, which has it nowhere.
- Any restricted path. `scripts/verify-release-tier1.sh` runs the installer under `env -i PATH=/usr/bin:/bin` precisely to prove a machine with no toolchain can install — and `/sbin` is not on it.

So the tier the release gate leans on is the tier where integrity checking does nothing. Demonstrated directly: the unpatched installer, on that path, accepted a file whose contents had been changed and exited 0.

`/usr/bin/shasum` is present in all of those cases.

## Why error rather than skip

PHASE-10 §13 makes "the installer refuses a corrupted archive" an exit criterion. A check that could not run has established nothing about the bytes, so reporting success is a claim it has no basis for. Both target platforms carry one of the two commands, so the refusal only fires on a machine that has neither — where declining to install something unverified is the right answer.

## Why a patch script and not configuration

The text is in `dist`'s own installer template. The choice was vendoring the whole template or a targeted rewrite; this is the second, with the fix to be sent upstream.

It fails loudly on text it does not recognise, and that is the part worth defending. A patch that silently does not apply is the same failure as the bug it fixes. If `dist` fixes this upstream, the release fails and somebody deletes the script — which is the outcome we want, arrived at by being told rather than by noticing.

`crates/keel/tests/installer_checksum.rs` covers it, including one test that pins the *unpatched* behaviour so the patch can be retired with evidence.

