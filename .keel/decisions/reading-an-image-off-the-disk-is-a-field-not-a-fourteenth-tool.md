<!-- keel:generated decision dec_01KZS2VXYGZ35YVV56QZ4AYNC0 v1 2026-08-11T18:56:07Z
     source of truth is Keel — edits here are not saved -->
# B-49 — Reading an image off the disk is a field, not a fourteenth tool

**Status:** `accepted`  
**Id:** `dec_01KZS2VXYGZ35YVV56QZ4AYNC0`

TQ-33 approved the capability by name: `keel_attach(id, path)`, so the daemon can read a file that is already on the same machine. TQ-31, settled hours earlier the same day, set thirteen tools as the ceiling and said a fourteenth needs an argument at least as good as the one that earned the thirteenth.

Both are KB's. This resolves them in favour of the capability without spending the slot.

## What was built

`image_path` on `keel_create`, beside the `image` field that already takes base64, and `image_path` in `keel_update`'s `changes` for attaching to something that already exists. Absolute paths only, up to 10 MB, sniffed from the magic bytes.

## Why this is the same decision rather than a different one

The substance of TQ-33 is that the daemon may read a local file. The form — a tool called `keel_attach` or an argument called `image_path` — is naming and internal structure, which the standing rules say to decide and record rather than ask about.

And `product/CLAUDE.md` names the alternative as an anti-pattern in almost these words: reaching for a new type when the modelling is awkward, where it is almost always a field. A second tool for "the same thing, from a path" would have been a second door onto one capability, with the base64 form on `keel_create` and the file form somewhere else — so a model deciding how to attach an image would first have to decide which tool attaches images.

The one thing a tool would have bought is a shorter call for the attach-to-existing case, which is `keel_update(id, version, changes: {image_path})`. That is not worth a permanent slot on a surface where every extra tool makes selection worse.

## The boundary that has to hold

A local path and a URL look similar and are not. One touches the machine Keel is already running on; the other gives a model the ability to make the daemon talk to the internet, which TQ-6 declined and TQ-33 confirmed. So anything URL-shaped is refused explicitly, with the reason in the message, and a test asserts it for `https:`, `http:` and `file:`. TQ-33 predicted the failure mode exactly: if the path argument ever quietly accepts something URL-shaped, that is this decision being reversed by accident.

Relative paths are refused too, for a duller reason: the daemon's working directory is its own, so a relative path resolves against something the caller cannot see.

## What the base64 description used to promise

1 MB, which no session can reach. Base64 inflates by a third and the *model* emits every character, so 1 MB is 350,000 to 450,000 output tokens and the useful ceiling is nearer 100 KB. The description now says the reachable number, says why, and points at the path with no such cost. Verified against a live daemon: a 683 KB PNG went from disk to the store and back out of `/api/blob/{id}` intact, which through base64 would have cost roughly 240,000 output tokens.

