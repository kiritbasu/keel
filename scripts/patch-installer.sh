#!/usr/bin/env bash
#
# Make the shell installer `dist` generates actually verify what it downloads.
#
# There are three places where the generated script reports success without
# having checked anything. Every one of them ends with an archive installed on
# somebody's machine on the strength of a line nobody reads.
#
# ## 1. A missing digest tool is treated as a successful verification
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
# ## 2. No checksum at all is treated as nothing to check
#
# Upstream's caller:
#
#     if [ -n "${_checksum_style:-}" ]; then
#         verify_checksum "$_file" "$_checksum_style" "$_checksum_value"
#     else
#         say "no checksums to verify" 1>&2
#     fi
#
# This is the one that shipped. Specline 0.1.2's installer embedded no checksums —
# `dist` fills them in from the per-target `dist-manifest.json` files it finds
# beside the archives, and this repository's hand-written release workflow never
# wrote any (KEEL-228). So every install took the second branch, printed one
# line, and carried on. The workflow now writes those manifests and
# `scripts/check-installer-checksums.sh` fails the release if the installer
# still has no checksums in it — but the installer itself should never have been
# willing to proceed, whatever the build did. An installer that cannot verify
# has not established anything about the bytes it is about to run.
#
# ## 3. An empty checksum value returns success before the switch
#
#     if [ -z "$_checksum_value" ]; then
#         return 0
#     fi
#
# The same hole one level down, reached if a style is set with no value. Closed
# for the same reason, and cheaply.
#
# ## Why this is a patch and not configuration
#
# There is no `dist` setting for any of it — the text lives in
# `templates/installer/installer.sh.j2` in the tool itself. The options were
# vendoring the whole installer template, or a targeted rewrite of the blocks
# with the upstream fix sent on. This is the second.
#
# ## Why it fails loudly
#
# A patch that silently does not apply is the same class of bug as the ones it
# is fixing: something that reports success while doing nothing. So every exit
# path is checked. If `dist` fixes these upstream, or moves the text, this
# script fails the release rather than leaving it unpatched, and whoever sees it
# deletes the patch that is no longer needed — which is the outcome we want.
#
# Idempotent: running it on an already-patched file is a no-op that exits 0.
#
# Usage: scripts/patch-installer.sh <installer.sh> [more.sh ...]

set -euo pipefail

if [ "$#" -eq 0 ]; then
    echo "usage: $0 <installer.sh> [...]" >&2
    exit 2
fi

# --- the three patches -------------------------------------------------------
#
# Each is a name, the exact block dist 0.32.0 emits, the block that replaces it,
# and a marker that says the replacement is already in place. The blocks are
# matched literally rather than by regex, including their indentation: a loose
# match that drifts with the template is how you get a patch that applies to
# something other than what it was written for.

names=()
broken=()
fixed=()
markers=()

# 1. `sha256sum` when it is there, `shasum -a 256` when it is not, and a refusal
#    when neither is — never a silent pass.
#
#    The refusal is a deliberate change from upstream's skip, not an oversight.
#    PHASE-10 §13 makes "the installer refuses a corrupted archive" an exit
#    criterion, and a verification that cannot run has not established anything
#    about the bytes it just downloaded. Both target platforms carry one of the
#    two commands, so this fires only on a machine that has neither, where
#    refusing to install something unverified is the right answer.
names+=("the sha256 digest tool")
read -r -d '' block <<'EOF' || true
        sha256)
            if ! check_cmd sha256sum; then
                say "skipping sha256 checksum verification (it requires the 'sha256sum' command)"
                return 0
            fi
            _calculated_checksum="$(sha256sum -b "$_file" | awk '{printf $1}')"
            ;;
EOF
broken+=("$block")
read -r -d '' block <<'EOF' || true
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
fixed+=("$block")
markers+=("shasum -a 256 -b ")

# 2. An installer with no checksum in it refuses, rather than announcing the
#    fact and installing anyway. This is the branch Specline 0.1.2 shipped in.
names+=("the no-checksum branch")
read -r -d '' block <<'EOF' || true
        if [ -n "${_checksum_style:-}" ]; then
            verify_checksum "$_file" "$_checksum_style" "$_checksum_value"
        else
            say "no checksums to verify" 1>&2
        fi
EOF
broken+=("$block")
read -r -d '' block <<'EOF' || true
        if [ -n "${_checksum_style:-}" ]; then
            verify_checksum "$_file" "$_checksum_style" "$_checksum_value"
        else
            err "this installer carries no checksum for $_artifact_name, so there is nothing to check the download against. Refusing to install unverified bytes. That is a broken release rather than something to work around: please report it at https://github.com/kiritbasu/specline/issues"
        fi
EOF
fixed+=("$block")
markers+=("carries no checksum for")

# 3. The same hole one level down: a style with no value behind it.
names+=("the empty checksum value")
read -r -d '' block <<'EOF' || true
    if [ -z "$_checksum_value" ]; then
        return 0
    fi
EOF
broken+=("$block")
read -r -d '' block <<'EOF' || true
    if [ -z "$_checksum_value" ]; then
        err "no checksum was recorded for $_file, so nothing has been established about the bytes just downloaded. Refusing to install them."
    fi
EOF
fixed+=("$block")
markers+=("no checksum was recorded for")

# --- applying them -----------------------------------------------------------

status=0

for file in "$@"; do
    if [ ! -f "$file" ]; then
        echo "patch-installer: $file does not exist" >&2
        status=1
        continue
    fi

    file_status=0

    for i in "${!names[@]}"; do
        contents="$(cat "$file")"

        if [[ "$contents" == *"${markers[$i]}"* ]]; then
            echo "patch-installer: $file already has ${names[$i]} patched"
            continue
        fi

        if [[ "$contents" != *"${broken[$i]}"* ]]; then
            {
                echo "patch-installer: $file does not contain the block this patch replaces (${names[$i]})."
                echo
                echo "  That means one of three things, and all of them need a person:"
                echo "    - dist fixed this upstream, and this part of the script should be deleted"
                echo "    - dist changed the template, and the block needs updating to match"
                echo "    - this is not a dist shell installer"
                echo
                echo "  Refusing to publish an installer whose checksum path has not been checked."
            } >&2
            file_status=1
            continue
        fi

        # `python3` rather than `sed`: the replacements are multi-line and
        # contain slashes, backslashes and quotes, and every shell-level
        # escaping scheme for that is a way to corrupt an installer subtly.
        BROKEN="${broken[$i]}" FIXED="${fixed[$i]}" python3 - "$file" <<'PY'
import os, sys

path = sys.argv[1]
broken = os.environ["BROKEN"]
fixed = os.environ["FIXED"]

with open(path, encoding="utf-8") as handle:
    text = handle.read()

if text.count(broken) != 1:
    sys.exit(f"patch-installer: expected exactly one occurrence in {path}, found {text.count(broken)}")

with open(path, "w", encoding="utf-8") as handle:
    handle.write(text.replace(broken, fixed, 1))
PY

        echo "patch-installer: $file — patched ${names[$i]}"
    done

    if [ "$file_status" -ne 0 ]; then
        status=1
        continue
    fi

    # Belt and braces: read it back and prove the things we came here to remove
    # are gone. The whole point of this script is that a check which reports
    # success without running is worse than no check, so it does not get to
    # claim success on the strength of having written a file.
    after="$(cat "$file")"
    for phrase in \
        "skipping sha256 checksum verification" \
        "no checksums to verify"
    do
        if [[ "$after" == *"$phrase"* ]]; then
            echo "patch-installer: $file still contains \"$phrase\" after patching" >&2
            file_status=1
        fi
    done
    for i in "${!names[@]}"; do
        if [[ "$after" != *"${markers[$i]}"* ]]; then
            echo "patch-installer: $file has no ${names[$i]} fix after patching" >&2
            file_status=1
        fi
    done

    if [ "$file_status" -ne 0 ]; then
        status=1
        continue
    fi

    echo "patch-installer: $file verifies what it downloads, or refuses to install it"
done

exit "$status"
