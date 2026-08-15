<!-- keel:generated decision dec_01M01F8R621R79SSKGCV4D4G34 v1 2026-08-15T01:17:34Z
     source of truth is Keel — edits here are not saved -->
# B-72 — The repository stays private and macOS builds run on a self-hosted runner

**Status:** `accepted`  
**Id:** `dec_01M01F8R621R79SSKGCV4D4G34`

## Decision

The repository stays private. macOS builds — both targets — run on a self-hosted runner on KB's own Mac. Linux stays on GitHub's hosted `ubuntu-22.04`. This supersedes the answer given to the visibility question earlier the same day, which was to go public.

## What overturned it

Going public was never wanted for its own sake. It was wanted because §2 requires macOS runners for the ad-hoc signature and those are free only on public repositories. The cost of getting them was never priced properly.

It was priced when the change was made, and it is higher than it looked:

- **Making a repository public emits a `PublicEvent` to every follower.** There is no setting that suppresses it — GitHub's controls are over what an account receives, not what it broadcasts. The account has sixteen followers and the event is recorded at `2026-08-14T14:20:47Z`. Reverting to private does not retract it, and the public event stream is mirrored off GitHub.
- **There is no unlisted tier.** Private or public, with `internal` for organisations only. Public means the profile listing, GitHub search, code search and crawlers.
- **Nothing written into Keel can be taken back out of the mirror.** Found while trying to remove a machine path before publishing: retracting a note works, but editing a task body reprints the old value in the changelog, because events are immutable and the changelog derives from them. That is KEEL-215, and until it is fixed "publish the mirror" and "redact anything" cannot both be true.

## Why a self-hosted runner answers it rather than working around it

The requirement in §2 is Apple's linker, not GitHub's hardware. An Apple Silicon Mac satisfies it for both targets: `--target=x86_64-apple-darwin` on an arm64 host still links with `cc` and still gets the ad-hoc signature. §10 had already worked this out for the August 2027 Intel-runner retirement — the plan for then is the plan for now, arrived at early.

Self-hosted runners are free and unmetered. Linux stays hosted because it is billed at the base rate and because a second machine testing Linux is worth more than a saved minute — CI has never once tested Linux on anything but a hosted runner.

And the usual objection does not apply. A self-hosted runner on a *public* repository is dangerous, because a pull request from a stranger executes on the machine. On a private repository it is the ordinary supported pattern. Private is what makes this safe, so the two halves of this decision hold each other up rather than trading off.

## What it costs, plainly

**The Mac has to be awake for a release, and for the macOS half of CI.** A job with no runner queues rather than failing, so the symptom of a sleeping laptop is a run that sits there — not an error. Worth knowing before it happens.

**The build machine is now the development machine.** This is the real cost and it lands on exactly the thing §12 tier 1 exists to protect: tier 1 runs the installer under `env -i HOME=<scratch> PATH=/usr/bin:/bin` precisely because the build machine has cargo, a real store and a running daemon, all of which make a broken release look fine. Building on that machine does not weaken the trick — the stripped environment is still stripped — but it removes the last accidental independence there was.

So tier 2, the Linux VM, matters more under this decision than it did before, not less. It is now the only verification that happens anywhere other than one Mac.

## Rejected

Staying private and paying for hosted macOS minutes at ten times the Linux rate, at a cadence of "every few days". It is the option that changes nothing and bills for it.

