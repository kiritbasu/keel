#!/usr/bin/env bash
#
# Prove that a built installer carries the checksum of every archive it can
# download, and that each one is the checksum of the file actually being
# published.
#
# ## Why this exists
#
# Keel 0.1.2 shipped an installer that verified nothing. `dist` emits the
# checksum into the installer from the per-target `dist-manifest.json` files it
# finds beside the archives; this repository's hand-written release workflow
# never wrote any, so `_checksum_style` came out empty, every install took the
# "no checksums to verify" branch, and the binaries went on somebody's machine
# unverified (KEEL-228).
#
# Nothing noticed. `scripts/patch-installer.sh` ran and passed, because it was
# checking a different hole in the same file. Both release-verification tiers
# ran their "installer refuses a corrupt archive" check and *passed*, because
# the corrupted archive did fail — at `tar`, not at any checksum — and their
# grep for checksum language matched the words "no checksums to verify". Three
# green checks over a false property.
#
# So this one is not written in terms of what the installer says. It reads the
# hex out of the script, hashes the archive on disk, and compares. There is no
# wording it can be satisfied by.
#
# Usage:
#
#   scripts/check-installer-checksums.sh <installer.sh> <artifact-dir>
#   scripts/check-installer-checksums.sh --embedded-only <installer.sh>
#
# Exits non-zero, and says which archive and why, if:
#
#   - the installer offers an archive with no checksum embedded
#   - an embedded checksum is not a sha256
#   - an archive the installer offers is not in the artifact directory
#   - an embedded checksum is not the digest of that archive
#   - the installer still contains the text of a silent-skip branch
#   - the installer offers no archives at all, which would mean this script
#     parsed nothing and passed
#
# `--embedded-only` drops the two checks that need the archives, for the case
# where the installer is all there is — `scripts/verify-release-tier1.sh` run
# against an already-published release, say. It establishes strictly less, and
# says which line it is printing, because "we could not tell" and "it is fine"
# must never read the same.

set -euo pipefail

mode="full"
if [ "${1:-}" = "--embedded-only" ]; then
    mode="embedded-only"
    shift
fi

if { [ "$mode" = "full" ] && [ "$#" -ne 2 ]; } || { [ "$mode" = "embedded-only" ] && [ "$#" -ne 1 ]; }; then
    echo "usage: $0 <installer.sh> <artifact-dir>" >&2
    echo "       $0 --embedded-only <installer.sh>" >&2
    exit 2
fi

installer="$1"
artifacts="${2:-}"

if [ ! -f "$installer" ]; then
    echo "check-installer-checksums: no such installer: $installer" >&2
    exit 2
fi
if [ "$mode" = "full" ] && [ ! -d "$artifacts" ]; then
    echo "check-installer-checksums: no such directory: $artifacts" >&2
    exit 2
fi

# `python3` rather than shell text handling: this parses a case statement and
# hashes files, and getting either subtly wrong here would produce exactly the
# false green this script exists to make impossible.
python3 - "$installer" "$artifacts" "$mode" <<'PY'
import hashlib
import re
import sys
from pathlib import Path

installer_path = Path(sys.argv[1])
artifact_dir = Path(sys.argv[2])
embedded_only = sys.argv[3] == "embedded-only"
text = installer_path.read_text(encoding="utf-8")

problems = []

# The generated installer destructures the selected archive in a case statement:
#
#     "keel-aarch64-apple-darwin.tar.xz")
#         _arch="aarch64-apple-darwin"
#         _zip_ext=".tar.xz"
#         _checksum_style="sha256"
#         _checksum_value="4f3c..."
#         ...
#         ;;
#
# One arm per archive, and the checksum lines are *absent* rather than empty
# when dist had nothing to fill them in with — which is why this looks for arms
# first and then asks each one whether it has a checksum, rather than looking
# for checksums and counting them.
ARM = re.compile(
    r'^\s*"(?P<name>[^"]+\.(?:tar\.[a-z0-9]+|tgz|zip))"\)\s*$(?P<body>.*?)^\s*;;\s*$',
    re.MULTILINE | re.DOTALL,
)

arms = list(ARM.finditer(text))
if not arms:
    problems.append(
        "found no archive arms in the installer's case statement. Either the "
        "installer offers nothing to download, or dist changed the template and "
        "this check is now parsing nothing and passing."
    )

for arm in arms:
    name = arm.group("name")
    body = arm.group("body")

    style = re.search(r'_checksum_style="([^"]*)"', body)
    value = re.search(r'_checksum_value="([^"]*)"', body)

    if style is None or value is None:
        problems.append(
            f"{name}: the installer embeds no checksum for it, so it would "
            f"download this archive with nothing to check it against. dist fills "
            f"these in from the per-target dist-manifest.json files beside the "
            f"archives — if there are none in {artifact_dir}, that is why."
        )
        continue

    if style.group(1) != "sha256":
        problems.append(
            f"{name}: checksum style is {style.group(1)!r}, expected 'sha256' "
            f"(dist-workspace.toml sets checksum = \"sha256\")"
        )
        continue

    embedded = value.group(1).strip().lower()
    if not re.fullmatch(r"[0-9a-f]{64}", embedded):
        problems.append(f"{name}: {embedded!r} is not a sha256 digest")
        continue

    if embedded_only:
        print(f"check-installer-checksums: {name} sha256 {embedded} — embedded, not compared")
        continue

    archive = artifact_dir / name
    if not archive.is_file():
        problems.append(
            f"{name}: the installer offers it but it is not in {artifact_dir}, "
            f"so the embedded checksum describes a file this release is not "
            f"publishing"
        )
        continue

    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if digest != embedded:
        problems.append(
            f"{name}: the installer would check for {embedded}, but the archive "
            f"about to be published hashes to {digest}. Somebody rebuilt or "
            f"rewrote one of the two after the other was made."
        )
        continue

    print(f"check-installer-checksums: {name} sha256 {digest} — matches the archive")

# The wording checks are secondary, and deliberately so: they catch a
# regression in scripts/patch-installer.sh rather than establishing anything
# about the bytes. Kept because the phrase is the one users saw on 0.1.2 and it
# must never appear in a shipped installer again.
for phrase in ("no checksums to verify", "skipping sha256 checksum verification"):
    if phrase in text:
        problems.append(
            f'the installer still contains "{phrase}" — a branch that installs '
            f"without checking. scripts/patch-installer.sh should have removed it."
        )

if problems:
    print("", file=sys.stderr)
    print("check-installer-checksums: this installer must not ship.", file=sys.stderr)
    print("", file=sys.stderr)
    for problem in problems:
        print(f"  - {problem}", file=sys.stderr)
    print("", file=sys.stderr)
    sys.exit(1)

if embedded_only:
    print(
        f"check-installer-checksums: {len(arms)} archive(s), every one with a "
        f"sha256 embedded. NOT compared against any file — pass an artifact "
        f"directory for that."
    )
else:
    print(
        f"check-installer-checksums: {len(arms)} archive(s), every one verified "
        f"against the file being published"
    )
PY
