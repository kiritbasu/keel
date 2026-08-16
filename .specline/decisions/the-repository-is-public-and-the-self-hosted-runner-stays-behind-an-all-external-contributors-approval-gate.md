<!-- specline:generated decision dec_01M031B11Y5XFDD2QGMA3CP85V v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-74 — The repository is public, and the self-hosted runner stays behind an all-external-contributors approval gate

**Status:** `accepted`  
**Id:** `dec_01M031B11Y5XFDD2QGMA3CP85V`

## Decision

`kiritbasu/keel` is public as of 2026-08-15. This supersedes B-72, which chose private the previous day.

The self-hosted macOS runner on KB's Mac **stays registered and stays in both workflows**. The exposure it creates is closed by setting the repository's fork-PR approval policy to `all_external_contributors`, rather than by moving to hosted runners.

## What changed since B-72

B-72 gave three reasons for private. One has gone:

- Its third reason was that nothing written into Keel could be taken back out of the mirror, so "publish the mirror" and "redact anything" could not both be true. That is KEEL-215, and it closed `done` about half an hour after B-72 was written.

The other two stand and were accepted rather than answered: the `PublicEvent` at 2026-08-14T14:20:47Z cannot be retracted, and there is no unlisted tier.

Before flipping, the tracked tree was scanned for the things that cannot be taken back — machine paths, the account name, real email addresses, and key material. It was clean, which is KEEL-215's fix holding rather than luck.

## The runner, and why this is the weaker of the two options

B-72 was right that a self-hosted runner on a public repository is dangerous: `ci.yml` triggers on `pull_request`, and for a fork PR the workflow definition comes from the contributor's branch — so a stranger can name `runs-on: [self-hosted, macOS, ARM64]` regardless of what our workflow files say. The runner was online at the moment of the flip.

KB chose to keep it and gate it. The gate is real: `all_external_contributors` means no outside contributor's workflow runs without approval, not merely first-timers. It is also weaker than the alternative, and worth being honest about why: it relies on a settings value staying set, where de-registering the runner would have made the machine unreachable. **B-72 explicitly rejected relying on a gate.** This overrides that judgement knowingly, in exchange for keeping the warm `target/` cache that makes macOS CI fast.

Two consequences to hold onto. GitHub will not let the policy be set at all while a repository is private, so it could not be closed in advance of the flip — there was a window, and it was measured: the policy was set immediately after, and the fork count was 0 throughout. And if the policy is ever reset to a default, the exposure returns silently, which is the failure shape this project keeps meeting.

## What it buys

- **Attestation resumes with no code change.** `release.yml`'s attest step is conditioned on the repository being private; the next release carries provenance, and the "no build provenance" note stops being added to release notes. Verifying it is not built — that is the open half of B-73.
- **Installs need no account.** Release assets are served from `releases/latest/download/…` unauthenticated, confirmed by a tokenless request returning 200. This is what makes the updater a plain HTTPS GET instead of `gh` plus an asset-id lookup, and it is the difference between something KB can install on his other Mac and something another person can install at all.
- **Hosted macOS minutes are now free**, which removes the cost half of B-72's argument even though the runner is staying.

## Rejected

Moving the macOS legs to `macos-latest` and de-registering the runner. It closes the exposure completely and is the shape B-72 had already planned for the 2027 Intel-runner retirement; it was declined for build speed.

