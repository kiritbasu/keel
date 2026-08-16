#!/usr/bin/env bash
#
# Phase 10 §12, tier 1 — does a release install and work on a machine that has
# never had Specline?
#
# Tier 1 is the tier that runs on the build machine, which is also the machine
# least able to answer the question honestly: it has cargo, it has a `~/.specline`
# with months of rows in it, and it has a daemon listening on 7654. Every
# one of those makes a broken release look fine. So the whole script is built
# around one trick — `env -i HOME=<scratch> PATH=/usr/bin:/bin` — which takes
# all three away and leaves something close enough to a stranger's Mac to be
# worth believing.
#
#   scripts/verify-release-tier1.sh dist/keel-installer.sh
#   scripts/verify-release-tier1.sh target/distrib            # a whole artifact dir
#   scripts/verify-release-tier1.sh dist/keel-installer.sh /tmp/somewhere-else
#   KEEP=1 scripts/verify-release-tier1.sh dist/…             # leave the scratch for reading
#
# What it covers, and what it does not: §12 is explicit that tier 1 is blind to
# Linux, to a Mac without the Xcode command line tools, and to the cross-compile
# signature trap — the ad-hoc-signature failure from §2 that looks exactly like
# a corrupt download. A green run here is not a release; it is tier 1 of three.
#
# Two of these checks exist because of specific claims Phase 10 rests on, and
# both are claims that would be expensive to be wrong about:
#
#   - `curl` does not set `com.apple.quarantine`, which is the entire argument
#     for not paying Apple $99 a year. Asserted from an Info.plist argument,
#     proved here with `xattr -l` on a real download.
#   - The installer refuses a corrupted archive. The appendix records that the
#     generator's own script calls `sha256sum` — which stock macOS does not
#     have — then prints "skipping sha256 checksum verification" and returns
#     success. That is a verification step that reports success while doing
#     nothing, which this repository has been bitten by twice before, and it is
#     why the corruption is done on purpose rather than trusted to a code read.
#
# `set -uo pipefail`, deliberately without `-e`: the point is to run every
# check and report, not to stop at the first one. Every check tests its own
# exit codes.
#
# Fail closed. A check that could not be run counts as a failure, not as a
# skip — "we could not tell" and "it works" must never print the same line.

set -uo pipefail

# --- arguments --------------------------------------------------------------

usage() {
  sed -n '2,42p' "$0" | sed 's/^# \{0,1\}//'
  exit 2
}

[ $# -ge 1 ] || usage
case "$1" in -h|--help) usage ;; esac

TARGET="$1"
SCRATCH="${2:-/tmp/keel-tier1}"

# The port the scratch daemon binds. Not 7654, and this is not a preference.
# The build machine has a daemon on 7654 serving the real store, so a health
# probe against the default would pass by reading somebody else's healthy
# daemon, and `specline doctor` would report on the real store while claiming to
# describe a fresh install. Every command below that takes `--daemon` is
# pointed here explicitly for the same reason.
PORT="${PORT:-7699}"

# The quarantine claim is about `curl`, not about any particular file, but it
# has to be a real network transfer to prove anything — a `file://` fetch would
# demonstrate nothing about how a download arrives. GitHub because that is
# where releases actually come from.
QUARANTINE_URL="${QUARANTINE_URL:-https://github.com/robots.txt}"

# Resolve the target into an installer script and, where there is one, the
# directory of artifacts beside it. The corruption check needs the artifacts;
# everything else needs only the script.
if [ -d "$TARGET" ]; then
  ARTIFACT_DIR="$(cd "$TARGET" && pwd)"
  INSTALLER="$(find "$ARTIFACT_DIR" -maxdepth 1 -type f -name '*installer.sh' | sort | head -1)"
elif [ -f "$TARGET" ]; then
  INSTALLER="$(cd "$(dirname "$TARGET")" && pwd)/$(basename "$TARGET")"
  ARTIFACT_DIR="$(dirname "$INSTALLER")"
else
  echo "verify-release-tier1: no such file or directory: $TARGET" >&2
  exit 2
fi

if [ -z "$INSTALLER" ] || [ ! -f "$INSTALLER" ]; then
  echo "verify-release-tier1: found no installer script in $TARGET" >&2
  exit 2
fi

# The scratch root is about to be deleted, twice. Refuse anything that could be
# a real directory: this runs unattended before a release, and the cost of
# getting it wrong is not symmetrical with the cost of being fussy.
case "$SCRATCH" in
  /|/Users|/Users/*/|"$HOME"|"$HOME"/) echo "verify-release-tier1: refusing to use $SCRATCH as scratch" >&2; exit 2 ;;
  /*) ;;
  *) echo "verify-release-tier1: scratch root must be an absolute path" >&2; exit 2 ;;
esac

CLEAN_HOME="$SCRATCH/home"      # the virgin HOME the release is installed into
DIRTY_HOME="$SCRATCH/home-bad"  # a second one, for the run that must be refused
MIRROR="$SCRATCH/mirror"        # artifacts, with every archive corrupted
CONTROL="$SCRATCH/mirror-ok"    # the same artifacts, untouched
LOGS="$SCRATCH/logs"
CLEAN_PATH="/usr/bin:/bin"

DAEMON_PID=""

# --- reporting --------------------------------------------------------------

pass=0; fail=0; warn=0

ok()      { printf '  \033[32mpass\033[0m  %-34s %s\n' "$1" "${2:-}"; pass=$((pass+1)); }
bad()     { printf '  \033[31mFAIL\033[0m  %-34s %s\n' "$1" "${2:-}"; fail=$((fail+1)); }
caution() { printf '  \033[33mwarn\033[0m  %-34s %s\n' "$1" "${2:-}"; warn=$((warn+1)); }
note()    { printf '        %s\n' "$1"; }

# Run something in an environment as close to a stranger's Mac as this machine
# can offer: no cargo, no rustup, no `~/.specline`, no `~/.cargo/env` sourced by a
# shell profile, nothing on PATH that the release did not put there.
clean_run() { env -i "HOME=$CLEAN_HOME" "PATH=$CLEAN_PATH" "$@"; }

cleanup() {
  # Kill the daemon before anything else. A stray daemon holding a store under
  # a directory that is about to be deleted is the one way this script could
  # leave the machine worse than it found it.
  if [ -n "$DAEMON_PID" ] && kill -0 "$DAEMON_PID" 2>/dev/null; then
    kill "$DAEMON_PID" 2>/dev/null
    wait "$DAEMON_PID" 2>/dev/null
  fi

  # Backstop, because the pid was wrong once and the consequence was a daemon
  # left serving a store this script had just deleted. Matched on the scratch
  # home, so it can only ever name a process this run started — the real daemon
  # on this machine serves `~/.specline` and cannot match.
  leaked="$(pgrep -f "specline-daemon .*$SCRATCH" 2>/dev/null || true)"
  if [ -n "$leaked" ]; then
    printf '  cleaning up a daemon that outlived its pid: %s\n' "$(echo "$leaked" | tr '\n' ' ')"
    # shellcheck disable=SC2086
    kill $leaked 2>/dev/null
    sleep 2
    still="$(pgrep -f "specline-daemon .*$SCRATCH" 2>/dev/null || true)"
    # shellcheck disable=SC2086
    [ -n "$still" ] && kill -9 $still 2>/dev/null
  fi
  if [ "${KEEP:-0}" = "1" ]; then
    printf '\n  scratch kept at %s\n\n' "$SCRATCH"
  else
    rm -rf "$SCRATCH"
  fi
}
trap cleanup EXIT

rm -rf "$SCRATCH"
mkdir -p "$CLEAN_HOME" "$DIRTY_HOME" "$LOGS"

# --- where the bytes come from ----------------------------------------------
#
# **Tier 1 has to be runnable before the tag, or it verifies nothing that
# matters.** The generated installer downloads from
# `github.com/<owner>/keel/releases/download/<version>`, so a run that only
# works once a release exists can only ever confirm what has already shipped —
# which is the wrong way round for a gate whose job is to stop a bad release.
#
# So when archives are sitting beside the installer, they are what gets
# installed, through the generator's own `INSTALLER_DOWNLOAD_URL` and a
# `file://` mirror. That is the mode this runs in before a release. With no
# archives it falls through to the published release, which is the mode worth
# running afterwards.
#
# The mode is printed, and it is named in the summary at the end, because "the
# installer completes" means two different things in the two modes and a green
# run must not be ambiguous about which one it established.
LOCAL_ARCHIVES="$(find "$ARTIFACT_DIR" -maxdepth 1 -type f \
  \( -name '*.tar.gz' -o -name '*.tar.xz' -o -name '*.tgz' -o -name '*.zip' \) 2>/dev/null | sort)"

if [ -n "$LOCAL_ARCHIVES" ]; then
  mkdir -p "$CONTROL"
  cp -R "$ARTIFACT_DIR"/. "$CONTROL"/ 2>/dev/null
  SOURCE_MODE="local artifacts"
  SOURCE_ENV="INSTALLER_DOWNLOAD_URL=file://$CONTROL"
else
  SOURCE_MODE="published release"
  # `env` takes no empty argument, so this carries a harmless assignment rather
  # than an empty string that would be parsed as a command name.
  SOURCE_ENV="SPECLINE_TIER1_SOURCE=release"
fi

printf '\n'
printf 'Phase 10 §12 tier 1 — clean-environment release verification\n'
printf 'installer  %s\n' "$INSTALLER"
printf 'source     %s\n' "$SOURCE_MODE"
printf 'scratch    %s   (HOME=%s, PATH=%s)\n' "$SCRATCH" "$CLEAN_HOME" "$CLEAN_PATH"
printf '\n'

# --- 0. the environment really is clean -------------------------------------
#
# A precondition, not a courtesy. If cargo is reachable, everything below still
# runs and still passes, and the run means nothing — the binary could have been
# built on the spot and "no toolchain present" would be a sentence this script
# printed rather than a fact it established. Checked first so a false green is
# impossible rather than unlikely.

if clean_run sh -c 'command -v cargo || command -v rustc || command -v rustup' >/dev/null 2>&1; then
  bad "stripped env has no toolchain" "cargo/rustc/rustup is reachable on $CLEAN_PATH"
  note "every result below is void: the machine could compile what it claims it downloaded"
else
  ok "stripped env has no toolchain" "no cargo, rustc or rustup on $CLEAN_PATH"
fi

if [ -e "$CLEAN_HOME/.specline" ]; then
  bad "scratch HOME has no store" "$CLEAN_HOME/.specline already exists"
else
  ok "scratch HOME has no store" "nothing at $CLEAN_HOME/.specline"
fi

# Nothing may be listening on the scratch port before we start. If something
# is, every daemon check below would be answered by whatever it is — most
# likely the real daemon on this machine — and would pass.
if curl -sf --max-time 3 "http://127.0.0.1:$PORT/api/health" >/dev/null 2>&1; then
  bad "scratch port is free" "something already answers on 127.0.0.1:$PORT"
  note "pass PORT=<n> for a free port; the daemon checks below cannot be trusted"
else
  ok "scratch port is free" "nothing on 127.0.0.1:$PORT"
fi

# --- 1. the installer runs, and puts binaries somewhere ---------------------

env -i "HOME=$CLEAN_HOME" "PATH=$CLEAN_PATH" "$SOURCE_ENV" \
  sh "$INSTALLER" >"$LOGS/install.log" 2>&1
install_status=$?
if [ $install_status -eq 0 ]; then
  ok "installer completes" "exit 0, from $SOURCE_MODE"
else
  bad "installer completes" "exit $install_status, from $SOURCE_MODE"
  note "see $LOGS/install.log"
fi

# `-type f` matters: `.keel` would match a name search, and a directory that
# happens to be called specline is not a binary. `-perm -u+x` because a file the
# installer unpacked without the execute bit is a failure that otherwise only
# shows up as a confusing "permission denied" three checks later.
SPECLINE_BIN="$(find "$CLEAN_HOME" -type f -perm -u+x -name specline 2>/dev/null | sort | head -1)"
DAEMON_BIN="$(find "$CLEAN_HOME" -type f -perm -u+x -name specline-daemon 2>/dev/null | sort | head -1)"

if [ -n "$SPECLINE_BIN" ] && [ -n "$DAEMON_BIN" ]; then
  ok "specline and specline-daemon installed" "${SPECLINE_BIN#"$CLEAN_HOME"/}, ${DAEMON_BIN#"$CLEAN_HOME"/}"
else
  bad "specline and specline-daemon installed" "specline=${SPECLINE_BIN:-missing} specline-daemon=${DAEMON_BIN:-missing}"
  note "searched $CLEAN_HOME for executable files by name"
fi

# --- 2. the binary runs with no toolchain -----------------------------------
#
# On Apple Silicon this is also the check that catches §2's cross-compile trap:
# an executable with no ad-hoc signature is killed at exec, which arrives as a
# kill signal and reads to a user like a corrupt download.

if [ -n "$SPECLINE_BIN" ]; then
  version_out="$(clean_run "$SPECLINE_BIN" --version 2>&1)"
  version_status=$?
  if [ $version_status -ne 0 ]; then
    bad "specline --version runs" "exit $version_status: $version_out"
    [ $version_status -ge 128 ] && note "killed by a signal — suspect a missing ad-hoc signature (§2), not a bad download"
  elif printf '%s' "$version_out" | grep -Eq '[0-9]+\.[0-9]+'; then
    ok "specline --version runs" "$version_out"
  else
    # Exit 0 with nothing version-shaped is ambiguous, so it fails.
    bad "specline --version runs" "exit 0 but no version in output: $version_out"
  fi
else
  bad "specline --version runs" "no specline binary to run"
fi

# --- 3. a store is created in the scratch HOME ------------------------------
#
# `fsck` rather than anything more elaborate because it opens the store, and
# opening a store that does not exist is what creates it — the same first-run
# path a real user takes. `--daemon` is pointed at the scratch port so that it
# falls back to a local read instead of asking the machine's real daemon about
# a store it has never heard of.

SPECLINE_STORE="$CLEAN_HOME/.specline"
if [ -n "$SPECLINE_BIN" ]; then
  clean_run "$SPECLINE_BIN" --home "$SPECLINE_STORE" fsck --daemon "http://127.0.0.1:$PORT" \
    >"$LOGS/fsck.log" 2>&1
  fsck_status=$?
  if [ $fsck_status -eq 0 ] && [ -f "$SPECLINE_STORE/keel.sqlite" ]; then
    ok "store created in scratch HOME" "$SPECLINE_STORE/keel.sqlite, fsck clean"
  elif [ ! -f "$SPECLINE_STORE/keel.sqlite" ]; then
    bad "store created in scratch HOME" "no keel.sqlite under $SPECLINE_STORE"
    note "see $LOGS/fsck.log"
  else
    bad "store created in scratch HOME" "fsck exit $fsck_status"
    note "see $LOGS/fsck.log"
  fi
else
  bad "store created in scratch HOME" "no specline binary to create it with"
fi

# --- 4. the daemon starts, answers, and stops -------------------------------

if [ -n "$DAEMON_BIN" ]; then
  # `env` directly, not the `clean_run` helper, and this is not style.
  #
  # Backgrounding a shell *function* forks a subshell, so `$!` is the
  # subshell's pid and not the daemon's. Killing it then kills the subshell and
  # orphans the daemon — which is exactly what happened on the first real run
  # of this script: the check reported "still alive or still answering", and a
  # daemon was left serving on the scratch port from a binary and a store the
  # cleanup had already deleted. `cleanup` below says a stray daemon is the one
  # way this script could leave the machine worse than it found it; it was
  # right, and this line was how.
  #
  # `env` execs its command rather than forking, so `$!` here is the daemon.
  env -i "HOME=$CLEAN_HOME" "PATH=$CLEAN_PATH" \
    "$DAEMON_BIN" --home "$SPECLINE_STORE" --bind "127.0.0.1:$PORT" \
    >"$LOGS/daemon.log" 2>&1 &
  DAEMON_PID=$!

  # Poll rather than sleep a fixed amount: first start migrates a new store,
  # and a timeout tuned to this machine on a good day is a flaky check on any
  # other. Fifteen seconds, then it has not started.
  health=""
  for _ in $(seq 1 15); do
    health="$(curl -sf --max-time 2 "http://127.0.0.1:$PORT/api/health" 2>/dev/null)"
    [ -n "$health" ] && break
    kill -0 "$DAEMON_PID" 2>/dev/null || break
    sleep 1
  done

  if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
    bad "daemon starts" "process exited before answering"
    note "see $LOGS/daemon.log"
    DAEMON_PID=""
  else
    ok "daemon starts" "pid $DAEMON_PID on 127.0.0.1:$PORT"
  fi

  # A 200 is not enough. The body has to be Specline's health body — anything else
  # answering on this port would satisfy `curl -sf` and prove nothing.
  if printf '%s' "$health" | grep -q '"status":"ok"' &&
     printf '%s' "$health" | grep -q '"schema"'; then
    ok "daemon answers /api/health" "$(printf '%s' "$health" | cut -c1-72)"
  else
    bad "daemon answers /api/health" "${health:-no response in 15s}"
  fi

  # `doctor` runs while the daemon is up on purpose: with one running it is
  # checking the store through its owner, which is the state a user is actually
  # in. It exits non-zero only for a real problem, so a degraded line — no
  # embeddings on a fresh store, no backup yet — correctly does not fail here.
  if [ -n "$SPECLINE_BIN" ]; then
    clean_run "$SPECLINE_BIN" --home "$SPECLINE_STORE" doctor --daemon "http://127.0.0.1:$PORT" \
      >"$LOGS/doctor.log" 2>&1
    doctor_status=$?
    if [ $doctor_status -eq 0 ]; then
      ok "specline doctor is happy" "exit 0 against the fresh store"
    else
      bad "specline doctor is happy" "exit $doctor_status"
      note "see $LOGS/doctor.log"
    fi
  else
    bad "specline doctor is happy" "no specline binary to run it with"
  fi

  # Stopping is part of the claim, not tidying up. A daemon that ignores TERM
  # is one a user cannot upgrade, and it would hold the store lock forever.
  if [ -n "$DAEMON_PID" ]; then
    kill "$DAEMON_PID" 2>/dev/null
    stopped=""
    for _ in $(seq 1 10); do
      kill -0 "$DAEMON_PID" 2>/dev/null || { stopped=yes; break; }
      sleep 1
    done
    wait "$DAEMON_PID" 2>/dev/null
    if [ -n "$stopped" ] && ! curl -sf --max-time 2 "http://127.0.0.1:$PORT/api/health" >/dev/null 2>&1; then
      ok "daemon stops on TERM" "port released"
    else
      bad "daemon stops on TERM" "still alive or still answering after 10s"
    fi
    DAEMON_PID=""
  else
    bad "daemon stops on TERM" "it never started"
  fi
else
  bad "daemon starts" "no specline-daemon binary"
  bad "daemon answers /api/health" "no specline-daemon binary"
  bad "specline doctor is happy" "no daemon to check through"
  bad "daemon stops on TERM" "no specline-daemon binary"
fi

# --- 5. the quarantine claim, proved rather than cited ----------------------
#
# §2 rests the entire "no Apple Developer Program" argument on this: quarantine
# is applied by the downloading application through a key in its Info.plist,
# and curl has no Info.plist. It is cheap to check and expensive to assume, and
# if Apple ever changes it the install story changes with it — so this asserts
# it against the network every release rather than quoting the appendix.

QFILE="$SCRATCH/quarantine-probe"
curl -sfL --proto '=https' --tlsv1.2 --max-time 20 -o "$QFILE" "$QUARANTINE_URL" 2>"$LOGS/quarantine.log"
curl_status=$?
if [ $curl_status -ne 0 ] || [ ! -s "$QFILE" ]; then
  # No download means the claim is unproven, and unproven fails.
  bad "curl download has no quarantine xattr" "download failed (curl exit $curl_status) from $QUARANTINE_URL"
  note "the claim is unproven, not disproven; set QUARANTINE_URL if that host is unreachable"
else
  xattrs="$(xattr -l "$QFILE" 2>&1)"
  if printf '%s' "$xattrs" | grep -q 'com.apple.quarantine'; then
    bad "curl download has no quarantine xattr" "com.apple.quarantine is present"
    note "§2's argument for skipping notarization does not hold; read $LOGS/quarantine.log and re-read §2"
  elif [ -z "$xattrs" ]; then
    ok "curl download has no quarantine xattr" "xattr -l printed nothing at all"
  else
    ok "curl download has no quarantine xattr" "other attributes present, none of them quarantine"
  fi
fi

# --- 6. the checksum refusal path -------------------------------------------
#
# The one check here that is about a known bug rather than a hypothetical. The
# generator's installer calls `sha256sum`; stock macOS ships `shasum` and not
# `sha256sum`, so the call fails, the script prints "skipping sha256 checksum
# verification" and returns success. That is the exact failure mode this repo
# deleted a hook over: a safety mechanism that reports success while doing
# nothing is worse than no mechanism, because it is relied upon.
#
# So it is checked twice. Once by reading the installer for the call, which is
# cheap and catches a regression the moment the generator is upgraded. Once by
# corrupting an archive and confirming the installer refuses it, which is the
# only evidence that means anything.

# Read by the script that knows what "verified" means, rather than by a grep for
# hopeful words. It pulls the hex out of the installer's own case statement,
# hashes the archive beside it, and compares — so it cannot be satisfied by
# wording, which is exactly how the version of this check that lived here missed
# 0.1.2 (KEEL-228).
#
# The old check asked whether checksum *files* existed beside the archives. They
# did, and the installer read none of them: `dist` embeds the digest in the
# script, and 0.1.2's had none embedded at all.
CHECKER="$(dirname "$0")/check-installer-checksums.sh"
archives="$(find "$ARTIFACT_DIR" -maxdepth 1 -type f \
  \( -name '*.tar.gz' -o -name '*.tar.xz' -o -name '*.tgz' -o -name '*.zip' \) 2>/dev/null | sort)"

if [ ! -x "$CHECKER" ]; then
  bad "installer carries real checksums" "cannot run $CHECKER"
elif [ -n "$archives" ]; then
  if "$CHECKER" "$INSTALLER" "$ARTIFACT_DIR" >"$LOGS/checksums.log" 2>&1; then
    ok "installer carries real checksums" "each archive's sha256 is embedded and matches the file"
  else
    bad "installer carries real checksums" "see $LOGS/checksums.log"
    sed -n 's/^  - /        /p' "$LOGS/checksums.log" | head -5
  fi
else
  # No archives to hash, so the digests can only be read, not confirmed. Said in
  # those words: this is a weaker claim than the branch above and must not read
  # like the same one.
  if "$CHECKER" --embedded-only "$INSTALLER" >"$LOGS/checksums.log" 2>&1; then
    caution "installer carries real checksums" "digests are embedded but nothing here can compare them"
    note "pass the artifact directory to check them against the archives"
  else
    bad "installer carries real checksums" "see $LOGS/checksums.log"
    sed -n 's/^  - /        /p' "$LOGS/checksums.log" | head -5
  fi
fi

if [ -z "$archives" ]; then
  bad "installer refuses a corrupt archive" "no archives beside the installer to corrupt"
  note "pass the artifact directory rather than the bare installer script"
else
  # One mirror to damage, one control left alone. The control is what separates
  # "the installer refused the corruption" from "the installer could not fetch
  # from a local mirror at all", which would otherwise look identical and would
  # be scored as a pass.
  #
  # The control may already exist — it is what the install above ran from when
  # there are local archives — so this tops it up rather than assuming.
  mkdir -p "$MIRROR" "$CONTROL"
  cp -R "$ARTIFACT_DIR"/. "$MIRROR"/ 2>/dev/null
  cp -R "$ARTIFACT_DIR"/. "$CONTROL"/ 2>/dev/null

  # Corrupt *every* archive, not a chosen one. The installer downloads only the
  # artifact for the host platform, so damaging one picked by sort order would
  # let a passing result mean "it never opened the file we broke".
  corrupted=0
  while IFS= read -r a; do
    [ -n "$a" ] || continue
    m="$MIRROR/$(basename "$a")"
    [ -f "$m" ] || continue
    size="$(stat -f %z "$m" 2>/dev/null || stat -c %s "$m" 2>/dev/null || echo 0)"
    [ "$size" -gt 256 ] || continue
    # In the middle and in place: appending would be caught by the archive
    # format as often as by the checksum, and a checksum check is what is on
    # trial here.
    dd if=/dev/urandom of="$m" bs=1 seek=$((size / 2)) count=64 conv=notrunc \
      >/dev/null 2>&1 && corrupted=$((corrupted + 1))
  done <<<"$archives"

  if [ "$corrupted" = "0" ]; then
    bad "installer refuses a corrupt archive" "could not corrupt any archive in $MIRROR"
  else
    # The control run first. `INSTALLER_DOWNLOAD_URL` is the generator's own
    # override and curl reads file:// URLs, but if this installer ignores it the
    # corrupted run would fail for the wrong reason and score a pass.
    env -i "HOME=$DIRTY_HOME" "PATH=$CLEAN_PATH" \
      "INSTALLER_DOWNLOAD_URL=file://$CONTROL" sh "$INSTALLER" \
      >"$LOGS/control.log" 2>&1
    control_status=$?
    rm -rf "${DIRTY_HOME:?}"/* "${DIRTY_HOME:?}"/.[!.]* 2>/dev/null

    if [ $control_status -ne 0 ]; then
      bad "installer refuses a corrupt archive" "the control install from an intact local mirror also failed"
      note "the installer does not honour INSTALLER_DOWNLOAD_URL, so nothing here proves a refusal"
      note "see $LOGS/control.log — this is inconclusive, which counts as a failure"
    else
      env -i "HOME=$DIRTY_HOME" "PATH=$CLEAN_PATH" \
        "INSTALLER_DOWNLOAD_URL=file://$MIRROR" sh "$INSTALLER" \
        >"$LOGS/corrupt.log" 2>&1
      corrupt_status=$?
      landed="$(find "$DIRTY_HOME" -type f -perm -u+x -name specline 2>/dev/null | head -1)"

      if [ $corrupt_status -eq 0 ] || [ -n "$landed" ]; then
        # The loud failure. Everything below the install path assumes a
        # tampered or truncated download cannot reach a user's machine.
        bad "installer refuses a corrupt archive" "IT ACCEPTED IT (exit $corrupt_status, binary=${landed:-none})"
        note "a corrupted archive installed. Checksum verification is absent or silently skipped."
        note "check $LOGS/corrupt.log for 'skipping sha256 checksum verification' — the sha256sum bug"
        note "this release must not ship"
      elif grep -Eq 'no checksums to verify|carries no checksum for' "$LOGS/corrupt.log"; then
        # **This branch is why 0.1.2 shipped.** The installer had no checksum at
        # all, downloaded the corrupted archive, said so, and then failed at
        # `tar` — a non-zero exit, and a log containing the word "checksum". The
        # grep below matched it and scored a pass. A refusal for the wrong
        # reason is not evidence of a working check, and "no checksums to
        # verify" is the absence of a check rather than one deciding anything.
        bad "installer refuses a corrupt archive" "it had no checksum to refuse it with"
        note "the installer said 'no checksums to verify' and then failed unpacking the damage"
        note "the exit code says nothing about integrity. See KEEL-228."
        note "this release must not ship"
      elif grep -qi 'checksum mismatch' "$LOGS/corrupt.log"; then
        ok "installer refuses a corrupt archive" "exit $corrupt_status, refused on the checksum"
      else
        # It failed, but nothing says it failed *because* the bytes were wrong.
        # A refusal for the wrong reason is not evidence of a working check.
        bad "installer refuses a corrupt archive" "exit $corrupt_status, but it never said 'checksum mismatch'"
        note "it failed for some other reason, so the refusal path is still unproven"
        note "a tar error on a damaged archive is not an integrity check"
        note "see $LOGS/corrupt.log"
      fi
    fi
  fi
fi

# --- summary ----------------------------------------------------------------

printf '\n'
printf '  %s passed, %s failed, %s warning(s)\n' "$pass" "$fail" "$warn"
if [ "$fail" -gt 0 ]; then
  printf '  tier 1 did not pass. Re-run with KEEP=1 to keep %s for reading.\n' "$LOGS"
else
  printf '  tier 1 passed. It is blind to Linux, to a Mac without the Xcode command\n'
  printf '  line tools, and to the cross-compile signature trap — tiers 2 and 3 cover those.\n'
fi
printf '\n'

[ "$fail" -eq 0 ]
