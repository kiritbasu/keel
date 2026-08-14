#!/usr/bin/env bash
#
# Make the shell installer `dist` generates actually verify what it downloads
# on macOS.
#
# ## What is wrong with the generated script
#
# `dist` 0.32.0 verifies a sha256 like this:
#
#     if ! check_cmd sha256sum; then
#         say "skipping sha256 checksum verification (it requires the 'sha256sum' command)"
#         return 0
#     fi
#
# `return 0` is the whole problem. A missing tool is reported as a *successful*
# verification, so the caller installs the archive either way. It prints a line
# saying so, which is better than nothing and is not what anyone reads while
# watching an install scroll past.
#
# PHASE-10 recorded this as "stock macOS does not have `sha256sum`". Measured on
# 2026-08-14 that is now half true, and the correction is worth having: this
# machine (Darwin 25.5) *does* ship `/sbin/sha256sum`, an Apple binary dated
# June 2026. So on a current macOS with the default `PATH` — which includes
# `/sbin` — the check runs.
#
# It skips on every other macOS path into this code:
#
#   - Older macOS, which has no `sha256sum` anywhere.
#   - Any restricted `PATH`. `scripts/verify-release-tier1.sh` runs the
#     installer under `env -i PATH=/usr/bin:/bin`, exactly to prove a machine
#     with no toolchain can install — and `/sbin` is not on it. So the tier the
#     release gate leans on is a tier where integrity checking does nothing.
#
# `/usr/bin/shasum` is present in all of those cases. It is Perl, it has been in
# the base system for as long as anyone cares about, and it computes the same
# digest. Hence the fallback.
#
# ## Why this is a patch and not configuration
#
# There is no `dist` setting for it — the text lives in
# `templates/installer/installer.sh.j2` in the tool itself. The options were
# vendoring the whole installer template, or a targeted rewrite of the block
# with the upstream fix sent on. This is the second.
#
# ## Why it fails loudly
#
# A patch that silently does not apply is the same class of bug as the one it is
# fixing: something that reports success while doing nothing. So every exit path
# is checked. If `dist` fixes this upstream, or moves the text, this script
# fails the release rather than leaving it unpatched, and whoever sees it
# deletes the script — which is the outcome we want.
#
# Idempotent: running it on an already-patched file is a no-op that exits 0.
#
# Usage: scripts/patch-installer.sh <installer.sh> [more.sh ...]

set -euo pipefail

if [ "$#" -eq 0 ]; then
    echo "usage: $0 <installer.sh> [...]" >&2
    exit 2
fi

# The exact block dist 0.32.0 emits. Matched literally rather than by regex: a
# loose match that drifts with the template is how you get a patch that applies
# to something other than what it was written for.
read -r -d '' BROKEN <<'EOF' || true
        sha256)
            if ! check_cmd sha256sum; then
                say "skipping sha256 checksum verification (it requires the 'sha256sum' command)"
                return 0
            fi
            _calculated_checksum="$(sha256sum -b "$_file" | awk '{printf $1}')"
            ;;
EOF

# `sha256sum` when it is there, `shasum -a 256` when it is not, and a refusal
# when neither is — never a silent pass.
#
# The refusal is a deliberate change from upstream's skip, not an oversight.
# PHASE-10 §13 makes "the installer refuses a corrupted archive" an exit
# criterion, and a verification that cannot run has not established anything
# about the bytes it just downloaded. Both target platforms carry one of the two
# commands, so this fires only on a machine that has neither, where refusing to
# install something unverified is the right answer.
read -r -d '' FIXED <<'EOF' || true
        sha256)
            if check_cmd sha256sum; then
                _calculated_checksum="$(sha256sum -b "$_file" | awk '{printf $1}')"
            elif check_cmd shasum; then
                _calculated_checksum="$(shasum -a 256 -b "$_file" | awk '{printf $1}')"
            else
                err "cannot verify the sha256 checksum of $_file: neither 'sha256sum' nor 'shasum' is on PATH. Refusing to install bytes that have not been checked. Install either one and run this again."
            fi
            ;;
EOF

status=0

for file in "$@"; do
    if [ ! -f "$file" ]; then
        echo "patch-installer: $file does not exist" >&2
        status=1
        continue
    fi

    contents="$(cat "$file")"

    if [[ "$contents" == *"shasum -a 256 -b "* ]]; then
        echo "patch-installer: $file is already patched"
        continue
    fi

    if [[ "$contents" != *"$BROKEN"* ]]; then
        {
            echo "patch-installer: $file does not contain the block this patch replaces."
            echo
            echo "  That means one of three things, and all of them need a person:"
            echo "    - dist fixed the sha256sum fallback upstream, and this script should be deleted"
            echo "    - dist changed the template, and the block below needs updating to match"
            echo "    - this is not a dist shell installer"
            echo
            echo "  Refusing to publish an installer whose checksum path has not been checked."
        } >&2
        status=1
        continue
    fi

    # `python3` rather than `sed`: the replacement is multi-line and contains
    # slashes, backslashes and quotes, and every shell-level escaping scheme for
    # that is a way to corrupt an installer subtly.
    BROKEN="$BROKEN" FIXED="$FIXED" python3 - "$file" <<'PY'
import os, sys

path = sys.argv[1]
broken = os.environ["BROKEN"]
fixed = os.environ["FIXED"]

with open(path, encoding="utf-8") as handle:
    text = handle.read()

if text.count(broken) != 1:
    sys.exit(f"patch-installer: expected exactly one sha256 block in {path}, found {text.count(broken)}")

with open(path, "w", encoding="utf-8") as handle:
    handle.write(text.replace(broken, fixed, 1))
PY

    # Belt and braces: read it back and prove the thing we came here to remove
    # is gone. The whole point of this script is that a check which reports
    # success without running is worse than no check, so it does not get to
    # claim success on the strength of having written a file.
    after="$(cat "$file")"
    if [[ "$after" == *"skipping sha256 checksum verification"* ]]; then
        echo "patch-installer: $file still skips sha256 verification after patching" >&2
        status=1
        continue
    fi
    if [[ "$after" != *"shasum -a 256 -b "* ]]; then
        echo "patch-installer: $file has no shasum fallback after patching" >&2
        status=1
        continue
    fi

    echo "patch-installer: $file now falls back to 'shasum -a 256'"
done

exit "$status"
