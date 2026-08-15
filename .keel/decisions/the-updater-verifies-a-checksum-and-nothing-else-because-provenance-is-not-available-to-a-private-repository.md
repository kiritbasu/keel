<!-- keel:generated decision dec_01M02ZT12E0A8RJZ050SJPKMB3 v1 2026-08-15T15:24:05Z
     source of truth is Keel — edits here are not saved -->
# B-73 — The updater verifies a checksum and nothing else, because provenance is not available to a private repository

**Status:** `accepted`  
**Id:** `dec_01M02ZT12E0A8RJZ050SJPKMB3`

## Decision

The auto-updater verifies the SHA-256 of what it downloads against the checksum in the published release manifest, and does not require a build attestation. It fetches through `gh release download` rather than opening a socket itself.

## Why the bar moved

KEEL-203 was written saying the updater "verifies checksum and build attestation" and that verification "is not optional and never degrades". Those two sentences cannot both hold today.

B-72 kept the repository private. GitHub does not issue artifact attestations for user-owned private repositories — `release.yml` already skips the attest step on `if: ${{ !github.event.repository.private }}`, and every release says so in its own notes. So **no release that exists carries provenance**. An updater holding the original bar refuses every update there is; one that quietly proceeds is the unverified fallback the task forbade. The bar was set before the repository's visibility was decided, and the decision moved underneath it.

Checksum-only is a real guarantee and worth naming precisely: it detects a corrupt or truncated download and a substituted asset, given that the manifest itself arrived intact. It does **not** independently establish that GitHub built these bytes from this commit. Provenance is the thing that is absent, and it is absent because of B-72 rather than because it was judged unnecessary.

## Why `gh` rather than an HTTP client in the daemon

KEEL-221 established by testing that a private repository's `releases/download/…` URL returns 404 with a valid Bearer token as readily as without one, and that only `api.github.com/repos/OWNER/REPO/releases/assets/{id}` with `Accept: application/octet-stream` serves the bytes — after an asset-id lookup by name. `setup.sh` already goes through `gh release download` for exactly this reason.

Reusing that route keeps credential handling out of the daemon, keeps `reqwest` a dev-dependency, and means the updater walks the same path the installer has been verified on. The cost is a hard dependency on the `gh` CLI being present and authenticated — acceptable because every install that exists today already needed it to get the bytes in the first place.

## What this obliges

The manifest becomes the trust root, so it travels the same authenticated path as the artifact and its absence is a hard failure rather than a reason to skip the check.

If the repository ever goes public, attestation starts working with no change to `release.yml`, and this decision should be revisited rather than inherited — the reason for it disappears the same day.

## Rejected

Requiring attestation and shipping the updater switched off until the repository goes public. It keeps the bar honest at the cost of building the auto half and never running it, against a visibility decision made deliberately eight days ago.

