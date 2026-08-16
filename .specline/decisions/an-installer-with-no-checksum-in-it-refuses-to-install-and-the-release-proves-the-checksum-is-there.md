<!-- specline:generated decision dec_01M03H4VBXDT31B3FX5TQ9653D v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-76 — An installer with no checksum in it refuses to install, and the release proves the checksum is there

**Status:** `accepted`  
**Id:** `dec_01M03H4VBXDT31B3FX5TQ9653D`

## Decision

Two changes, and the second is the one that matters.

1. **`scripts/patch-installer.sh` now rewrites three blocks, not one.** B-71 fixed the missing digest tool. The other two are the same shape: the caller's `else say "no checksums to verify"` when no digest is embedded, and `verify_checksum`'s `return 0` on an empty value. Both are now `err`. An installer that cannot check has established nothing, whatever the reason, so it does not install.

2. **`scripts/check-installer-checksums.sh` runs in the release job and fails it** if the installer does not carry the sha256 of every archive being published. It reads the hex out of the installer's own case statement, hashes the file about to be uploaded, and compares. It is deliberately not written in terms of what the installer *says*.

The build job also now writes `target/distrib/<target>-dist-manifest.json`, which is the actual root cause fix — see below.

## Why

Keel 0.1.2 shipped an installer that verified nothing. KB ran it and saw:

```
downloading keel 0.1.2 aarch64-apple-darwin
no checksums to verify
installing to /Users/h8hcn/.cargo/bin
```

`dist` fills a digest into the installer from the per-target `dist-manifest.json` files it finds in the dist directory — `load_manifests` reads every `*dist-manifest.json` there, `merge_artifact` merges the checksums, and `fill_in_checksums_from_manifest` puts them in the template. Only `dist host` writes such a file, and this repository's hand-written workflow does not call it. The workflow header already recorded that cost — "at the cost of not merging the per-target manifests" — without anyone connecting it to the installer's integrity check.

Verified against `dist` 0.32.0 rather than reasoned about: planting a manifest carrying an archive's digest in `target/distrib` and running `dist build --artifacts=global` produced an installer with `_checksum_style="sha256"` and the digest in it. Removing the manifest and rebuilding produced 0.1.2's installer again.

## Three green checks over a false property

This is the part worth keeping.

- `patch-installer.sh` passed. It was fixing the digest-tool hole in the same file, and had nothing to say about a digest that was never there.
- `verify-release-tier1.sh` and `verify-release-tier2.sh` both passed their "installer refuses a corrupt archive" check. Reproduced on 2026-08-15 with the published 0.1.2 installer and a deliberately damaged archive: it printed "no checksums to verify", then failed at `tar`. Non-zero exit, and a log matching their `grep -Eqi 'checksum|sha256|verif|mismatch|corrupt'` — because "no checksums to verify" contains the word "checksum". Scored a pass.

Both tiers now fail on that wording explicitly, and the only output they will accept as evidence of a working check is `checksum mismatch`. A `tar` error on a damaged archive is not an integrity check and no longer reads as one.

## Why the installer refuses rather than warns

The installer is the last thing standing between a user and unverified bytes, and it is the piece that runs on their machine rather than in a job somebody can inspect. It should not have to depend on the release having been built correctly to be safe. On 0.1.2 it was not, and the only thing between that and an unverified install was a line of output.

## What this does not fix

0.1.2's published installer is still the one that verifies nothing, and `.../releases/latest/download/keel-installer.sh` still serves it. Anyone installing before the next release gets the unverified path. Re-cutting or re-uploading is KB's call.

