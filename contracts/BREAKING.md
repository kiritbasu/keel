# Breaking changes, acknowledged

Every breaking difference the classifier finds has to appear below the marker in
this file before a release can merge. CI fails otherwise, and it fails in both
directions: a breaking change with no entry, and an entry describing nothing
that changed.

This is the mechanism. The version number is decoration — on 0.x, additive and
breaking both mean a minor bump, so a gate that checked the number would be
satisfied by every release forever while looking like a guard.

## How to add one

Run the classifier against the previous release tag:

```
CONTRACTS_BASELINE=<last-tag> cargo test -p keel --test classify -- --nocapture
```

Copy each `BREAKING` line **exactly** as printed, and write the two fields
underneath it:

```markdown
## tool `keel_note` was removed
- migration: none — callers move to `keel_update` with a `notes` field
- tells the user: `keel_note` is gone. Notes are a field on `keel_update` now.
```

The heading has to match the classifier's sentence word for word. That is
deliberate: if the description of what breaks has changed, whoever signed it off
should read it again, rather than have a stable key quietly carry an old
agreement forward.

`migration: none` is a real answer when nothing needs migrating. Blank is not —
an entry with an empty field is somebody acknowledging that a problem exists
rather than the problem.

## Why the marker

Everything above `<!-- acknowledgements -->` is instructions, and its headings
are not entries — including the example directly above, which is a real-looking
entry that must never be counted as one.

Without the marker the parser read `## How to add one` as an acknowledgement of
a difference by that name, which then failed as stale and blocked a release that
was otherwise fine. The instructions live in the same file on purpose, because
they are what somebody needs at the moment they are writing an entry, so the
boundary has to be explicit rather than inferred from heading levels.

A file with no marker acknowledges nothing. That is the failing side on purpose:
renaming the marker must gate everything rather than wave everything through.

The **Breaking** section of the release notes is generated from the entries
below, which is the point of writing them here rather than in a commit message.
Notes assembled by hand from a week of commits are how a breaking change reaches
users unannounced; notes built from the thing that already refused to let it
merge cannot forget one.

<!-- acknowledgements -->

*Nothing outstanding. Nothing has been released yet, so there is no baseline to
compare against — the first tag creates one.*
