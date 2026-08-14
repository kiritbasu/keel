#!/usr/bin/env bash
#
# Phase 10 §12, tier 1 — does a release install and work on a machine that has
# never had Keel?
#
# Tier 1 is the tier that runs on the build machine, which is also the machine
# least able to answer the question honestly: it has cargo, it has a `~/.keel`
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
# daemon, and `keel doctor` would report on the real store while claiming to
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
# can offer: no cargo, no rustup, no `~/.keel`, no `~/.cargo/env` sourced by a
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
  if [ "${KEEP:-0}" = "1" ]; then
    printf '\n  scratch kept at %s\n\n' "$SCRATCH"
  else
    rm -rf "$SCRATCH"
  fi
}
trap cleanup EXIT

rm -rf "$SCRATCH"
mkdir -p "$CLEAN_HOME" "$DIRTY_HOME" "$LOGS"

printf '\n'
printf 'Phase 10 §12 tier 1 — clean-environment release verification\n'
printf 'installer  %s\n' "$INSTALLER"
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

if [ -e "$CLEAN_HOME/.keel" ]; then
  bad "scratch HOME has no store" "$CLEAN_HOME/.keel already exists"
else
  ok "scratch HOME has no store" "nothing at $CLEAN_HOME/.keel"
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

clean_run sh "$INSTALLER" >"$LOGS/install.log" 2>&1
install_status=$?
if [ $install_status -eq 0 ]; then
  ok "installer completes" "exit 0"
else
  bad "installer completes" "exit $install_status"
  note "see $LOGS/install.log"
fi

# `-type f` matters: `.keel` would match a name search, and a directory that
# happens to be called keel is not a binary. `-perm -u+x` because a file the
# installer unpacked without the execute bit is a failure that otherwise only
# shows up as a confusing "permission denied" three checks later.
KEEL_BIN="$(find "$CLEAN_HOME" -type f -perm -u+x -name keel 2>/dev/null | sort | head -1)"
DAEMON_BIN="$(find "$CLEAN_HOME" -type f -perm -u+x -name keel-daemon 2>/dev/null | sort | head -1)"

if [ -n "$KEEL_BIN" ] && [ -n "$DAEMON_BIN" ]; then
  ok "keel and keel-daemon installed" "${KEEL_BIN#"$CLEAN_HOME"/}, ${DAEMON_BIN#"$CLEAN_HOME"/}"
else
  bad "keel and keel-daemon installed" "keel=${KEEL_BIN:-missing} keel-daemon=${DAEMON_BIN:-missing}"
  note "searched $CLEAN_HOME for executable files by name"
fi

# --- 2. the binary runs with no toolchain -----------------------------------
#
# On Apple Silicon this is also the check that catches §2's cross-compile trap:
# an executable with no ad-hoc signature is killed at exec, which arrives as a
# kill signal and reads to a user like a corrupt download.

if [ -n "$KEEL_BIN" ]; then
  version_out="$(clean_run "$KEEL_BIN" --version 2>&1)"
  version_status=$?
  if [ $version_status -ne 0 ]; then
    bad "keel --version runs" "exit $version_status: $version_out"
    [ $version_status -ge 128 ] && note "killed by a signal — suspect a missing ad-hoc signature (§2), not a bad download"
  elif printf '%s' "$version_out" | grep -Eq '[0-9]+\.[0-9]+'; then
    ok "keel --version runs" "$version_out"
  else
    # Exit 0 with nothing version-shaped is ambiguous, so it fails.
    bad "keel --version runs" "exit 0 but no version in output: $version_out"
  fi
else
  bad "keel --version runs" "no keel binary to run"
fi

# --- 3. a store is created in the scratch HOME ------------------------------
#
# `fsck` rather than anything more elaborate because it opens the store, and
# opening a store that does not exist is what creates it — the same first-run
# path a real user takes. `--daemon` is pointed at the scratch port so that it
# falls back to a local read instead of asking the machine's real daemon about
# a store it has never heard of.

KEEL_STORE="$CLEAN_HOME/.keel"
if [ -n "$KEEL_BIN" ]; then
  clean_run "$KEEL_BIN" --home "$KEEL_STORE" fsck --daemon "http://127.0.0.1:$PORT" \
    >"$LOGS/fsck.log" 2>&1
  fsck_status=$?
  if [ $fsck_status -eq 0 ] && [ -f "$KEEL_STORE/keel.sqlite" ]; then
    ok "store created in scratch HOME" "$KEEL_STORE/keel.sqlite, fsck clean"
  elif [ ! -f "$KEEL_STORE/keel.sqlite" ]; then
    bad "store created in scratch HOME" "no keel.sqlite under $KEEL_STORE"
    note "see $LOGS/fsck.log"
  else
    bad "store created in scratch HOME" "fsck exit $fsck_status"
    note "see $LOGS/fsck.log"
  fi
else
  bad "store created in scratch HOME" "no keel binary to create it with"
fi

# --- 4. the daemon starts, answers, and stops -------------------------------

if [ -n "$DAEMON_BIN" ]; then
  clean_run "$DAEMON_BIN" --home "$KEEL_STORE" --bind "127.0.0.1:$PORT" \
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

  # A 200 is not enough. The body has to be Keel's health body — anything else
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
  if [ -n "$KEEL_BIN" ]; then
    clean_run "$KEEL_BIN" --home "$KEEL_STORE" doctor --daemon "http://127.0.0.1:$PORT" \
      >"$LOGS/doctor.log" 2>&1
    doctor_status=$?
    if [ $doctor_status -eq 0 ]; then
      ok "keel doctor is happy" "exit 0 against the fresh store"
    else
      bad "keel doctor is happy" "exit $doctor_status"
      note "see $LOGS/doctor.log"
    fi
  else
    bad "keel doctor is happy" "no keel binary to run it with"
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
  bad "daemon starts" "no keel-daemon binary"
  bad "daemon answers /api/health" "no keel-daemon binary"
  bad "keel doctor is happy" "no daemon to check through"
  bad "daemon stops on TERM" "no keel-daemon binary"
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

if grep -Eq '(^|[^[:alnum:]_./-])sha256sum([^[:alnum:]_-]|$)' "$INSTALLER"; then
  caution "installer avoids bare sha256sum" "the installer text calls sha256sum"
  note "stock macOS has shasum, not sha256sum — this is the bug that silently skips verification"
  note "the corruption check below is what decides whether it actually matters"
else
  ok "installer avoids bare sha256sum" "no bare sha256sum call in the installer text"
fi

# There must be something to verify against before a refusal can mean anything:
# either checksum files shipped beside the archives, or hashes embedded in the
# installer itself. Neither is a release problem in its own right.
checksum_files="$(find "$ARTIFACT_DIR" -maxdepth 1 -type f \
  \( -name '*.sha256' -o -name '*sha256*' -o -name '*checksum*' \) 2>/dev/null | wc -l | tr -d ' ')"
embedded_hashes=0
grep -Eq '[0-9a-f]{64}' "$INSTALLER" && embedded_hashes=1

archives="$(find "$ARTIFACT_DIR" -maxdepth 1 -type f \
  \( -name '*.tar.gz' -o -name '*.tar.xz' -o -name '*.tgz' -o -name '*.zip' \) 2>/dev/null | sort)"

if [ -z "$archives" ]; then
  bad "installer refuses a corrupt archive" "no archives beside the installer to corrupt"
  note "pass the artifact directory rather than the bare installer script"
elif [ "$checksum_files" = "0" ] && [ "$embedded_hashes" = "0" ]; then
  bad "installer refuses a corrupt archive" "no checksum files and no embedded hashes"
  note "there is nothing for the installer to verify against, so it cannot refuse anything"
else
  mkdir -p "$MIRROR" "$CONTROL"
  # Copy twice: one mirror to damage, one control left alone. The control is
  # what separates "the installer refused the corruption" from "the installer
  # could not fetch from a local mirror at all", which would otherwise look
  # identical and would be scored as a pass.
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
      landed="$(find "$DIRTY_HOME" -type f -perm -u+x -name keel 2>/dev/null | head -1)"

      if [ $corrupt_status -eq 0 ] || [ -n "$landed" ]; then
        # The loud failure. Everything below the install path assumes a
        # tampered or truncated download cannot reach a user's machine.
        bad "installer refuses a corrupt archive" "IT ACCEPTED IT (exit $corrupt_status, binary=${landed:-none})"
        note "a corrupted archive installed. Checksum verification is absent or silently skipped."
        note "check $LOGS/corrupt.log for 'skipping sha256 checksum verification' — the sha256sum bug"
        note "this release must not ship"
      elif grep -Eqi 'checksum|sha256|sha-256|verif|mismatch|corrupt' "$LOGS/corrupt.log"; then
        ok "installer refuses a corrupt archive" "exit $corrupt_status, refused on the checksum"
      else
        # It failed, but nothing says it failed *because* the bytes were wrong.
        # A refusal for the wrong reason is not evidence of a working check.
        bad "installer refuses a corrupt archive" "exit $corrupt_status, but no checksum language in the output"
        note "it failed for some other reason, so the refusal path is still unproven"
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
