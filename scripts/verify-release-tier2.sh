#!/usr/bin/env bash
#
# Phase 10 §12 tier 2 — the Linux checks, run inside a Linux VM.
#
# **This is the highest-value check in the phase**, and the reason is not that
# Linux is important in itself. It is that Linux is the only platform whose
# behaviour nothing on the development machine can predict, and until CI ran for
# the first time on 2026-08-14 it had never been tested at all. Tier 1 says so
# itself: it is blind to Linux, to a Mac without the Xcode tools, and to the
# cross-compile signature trap.
#
#   scripts/verify-release-tier2.sh <installer.sh | artifact-dir>
#   scripts/verify-release-tier2.sh /vagrant/distrib /tmp/keel-tier2
#   KEEP=1 scripts/verify-release-tier2.sh …        # leave the scratch for reading
#
# ## Where this runs
#
# Inside the VM, not on the host. Copy the artifact directory in — a shared
# folder, `scp`, whatever the VM offers — and run this there. It refuses to run
# on macOS rather than degrading into a worse copy of tier 1.
#
# The VM is snapshot-restored between runs. That is what makes "a machine that
# has never had Rust" true on the second release as well as the first, and it is
# the whole reason this is a VM rather than a container: `systemd` checks below
# need PID 1, and the point of the glibc check is a real distribution userland.
#
# ## What this adds over tier 1
#
# - The Linux binary actually executing. Nothing else in the project does this.
# - **The glibc floor.** A binary built on a newer image refuses to start on an
#   older one, with a message about `GLIBC_2.xx` that reads like a corrupt
#   download to anyone who has not met it before. `release.yml` builds on
#   `ubuntu-22.04` to keep that floor low; this is what checks the floor is
#   where it was meant to be.
# - The shell installer on a machine that is not a Mac.
# - `systemd`: the unit installs, the daemon comes back after a kill.
# - "Nothing compiles", on a machine that has genuinely never had a toolchain
#   rather than one where it has merely been taken off the path.
#
# ## What it deliberately does not do
#
# It does not install a service into the user's real session on a machine
# anybody works on. Everything runs under a scratch `HOME`, and the systemd unit
# is a *user* unit in that scratch, so a failed run leaves nothing enabled.
#
# Fail closed, like tier 1: a check that could not be run counts as a failure,
# never as a skip. "We could not tell" and "it works" must not print the same
# line.

set -uo pipefail

usage() {
  sed -n '2,46p' "$0" | sed 's/^# \{0,1\}//'
  exit 2
}

[ $# -ge 1 ] || usage
case "$1" in -h|--help) usage ;; esac

TARGET="$1"
SCRATCH="${2:-/tmp/keel-tier2}"
PORT="${PORT:-7699}"

# The oldest glibc the release is meant to run on. `release.yml` builds Linux on
# ubuntu-22.04, which is glibc 2.35, so a binary requiring more than this means
# the build image moved and the floor moved with it — silently, and only for
# people on older distributions.
GLIBC_FLOOR="${GLIBC_FLOOR:-2.35}"

# --- refuse to run anywhere that would make the result a lie ----------------

if [ "$(uname -s)" != "Linux" ]; then
  echo "verify-release-tier2: this is the Linux tier and this is $(uname -s)." >&2
  echo "  Run it inside the VM. On the Mac, scripts/verify-release-tier1.sh is the one you want." >&2
  exit 2
fi

if [ -d "$TARGET" ]; then
  ARTIFACT_DIR="$(cd "$TARGET" && pwd)"
  INSTALLER="$(find "$ARTIFACT_DIR" -maxdepth 1 -type f -name '*installer.sh' | sort | head -1)"
elif [ -f "$TARGET" ]; then
  INSTALLER="$(cd "$(dirname "$TARGET")" && pwd)/$(basename "$TARGET")"
  ARTIFACT_DIR="$(dirname "$INSTALLER")"
else
  echo "verify-release-tier2: no such file or directory: $TARGET" >&2
  exit 2
fi

if [ -z "$INSTALLER" ] || [ ! -f "$INSTALLER" ]; then
  echo "verify-release-tier2: found no installer script in $TARGET" >&2
  exit 2
fi

case "$SCRATCH" in
  /|/home|/home/*/|"$HOME"|"$HOME"/|/root|/root/)
    echo "verify-release-tier2: refusing to use $SCRATCH as scratch" >&2; exit 2 ;;
  /*) ;;
  *) echo "verify-release-tier2: scratch root must be an absolute path" >&2; exit 2 ;;
esac

CLEAN_HOME="$SCRATCH/home"
CONTROL="$SCRATCH/mirror-ok"
LOGS="$SCRATCH/logs"
CLEAN_PATH="/usr/bin:/bin"
DAEMON_PID=""

pass=0; fail=0; warn=0
ok()      { printf '  \033[32mpass\033[0m  %-34s %s\n' "$1" "${2:-}"; pass=$((pass+1)); }
bad()     { printf '  \033[31mFAIL\033[0m  %-34s %s\n' "$1" "${2:-}"; fail=$((fail+1)); }
caution() { printf '  \033[33mwarn\033[0m  %-34s %s\n' "$1" "${2:-}"; warn=$((warn+1)); }
note()    { printf '        %s\n' "$1"; }

clean_run() { env -i "HOME=$CLEAN_HOME" "PATH=$CLEAN_PATH" "$@"; }

cleanup() {
  if [ -n "$DAEMON_PID" ] && kill -0 "$DAEMON_PID" 2>/dev/null; then
    kill "$DAEMON_PID" 2>/dev/null
    wait "$DAEMON_PID" 2>/dev/null
  fi
  # The same backstop tier 1 needed, for the same reason: a pid that turns out
  # to name a subshell leaves a daemon serving a store that is about to be
  # deleted. Matched on the scratch path, so it can only name this run's own.
  leaked="$(pgrep -f "keel-daemon .*$SCRATCH" 2>/dev/null || true)"
  if [ -n "$leaked" ]; then
    printf '  cleaning up a daemon that outlived its pid: %s\n' "$(echo "$leaked" | tr '\n' ' ')"
    # shellcheck disable=SC2086
    kill $leaked 2>/dev/null; sleep 2
    still="$(pgrep -f "keel-daemon .*$SCRATCH" 2>/dev/null || true)"
    # shellcheck disable=SC2086
    [ -n "$still" ] && kill -9 $still 2>/dev/null
  fi
  # A user unit left enabled would survive the snapshot restore in the one case
  # that matters — somebody running this outside a VM.
  if [ -n "${UNIT_INSTALLED:-}" ]; then
    systemctl --user stop keel-tier2.service >/dev/null 2>&1
    systemctl --user disable keel-tier2.service >/dev/null 2>&1
    rm -f "$HOME/.config/systemd/user/keel-tier2.service"
    systemctl --user daemon-reload >/dev/null 2>&1
  fi
  if [ "${KEEP:-0}" = "1" ]; then
    printf '\n  scratch kept at %s\n\n' "$SCRATCH"
  else
    rm -rf "$SCRATCH"
  fi
}
trap cleanup EXIT

rm -rf "$SCRATCH"
mkdir -p "$CLEAN_HOME" "$LOGS"

LOCAL_ARCHIVES="$(find "$ARTIFACT_DIR" -maxdepth 1 -type f \
  \( -name '*.tar.gz' -o -name '*.tar.xz' -o -name '*.tgz' \) 2>/dev/null | sort)"
if [ -n "$LOCAL_ARCHIVES" ]; then
  mkdir -p "$CONTROL"
  cp -R "$ARTIFACT_DIR"/. "$CONTROL"/ 2>/dev/null
  SOURCE_MODE="local artifacts"
  SOURCE_ENV="INSTALLER_DOWNLOAD_URL=file://$CONTROL"
else
  SOURCE_MODE="published release"
  SOURCE_ENV="KEEL_TIER2_SOURCE=release"
fi

printf '\n'
printf 'Phase 10 §12 tier 2 — Linux release verification\n'
printf 'host       %s %s (%s)\n' "$(uname -s)" "$(uname -r)" "$(uname -m)"
printf 'installer  %s\n' "$INSTALLER"
printf 'source     %s\n' "$SOURCE_MODE"
printf 'scratch    %s\n' "$SCRATCH"
printf '\n'

# --- 0. this machine has genuinely never had Rust ---------------------------
#
# Stronger than tier 1's version, which only proves the toolchain is off one
# PATH. Here the claim is about the machine, so it looks in the places a
# toolchain actually installs rather than trusting a stripped environment.

toolchain_found=""
for candidate in /usr/bin/cargo /usr/local/bin/cargo /usr/bin/rustc "$HOME/.cargo/bin/cargo" "$HOME/.rustup"; do
  [ -e "$candidate" ] && toolchain_found="$toolchain_found $candidate"
done
if [ -n "$toolchain_found" ]; then
  bad "this machine has no toolchain" "found:$toolchain_found"
  note "tier 2 is meant to run on a VM that has never had Rust; restore the snapshot"
else
  ok "this machine has no toolchain" "no cargo, rustc or rustup anywhere it would install"
fi

# --- 1. the installer runs on a machine that is not a Mac -------------------

env -i "HOME=$CLEAN_HOME" "PATH=$CLEAN_PATH" "$SOURCE_ENV" \
  sh "$INSTALLER" >"$LOGS/install.log" 2>&1
install_status=$?
if [ $install_status -eq 0 ]; then
  ok "installer completes" "exit 0, from $SOURCE_MODE"
else
  bad "installer completes" "exit $install_status, from $SOURCE_MODE"
  note "see $LOGS/install.log"
fi

KEEL_BIN="$(find "$CLEAN_HOME" -type f -executable -name keel 2>/dev/null | sort | head -1)"
DAEMON_BIN="$(find "$CLEAN_HOME" -type f -executable -name keel-daemon 2>/dev/null | sort | head -1)"

if [ -n "$KEEL_BIN" ] && [ -n "$DAEMON_BIN" ]; then
  ok "keel and keel-daemon installed" "${KEEL_BIN#"$CLEAN_HOME"/}, ${DAEMON_BIN#"$CLEAN_HOME"/}"
else
  bad "keel and keel-daemon installed" "keel=${KEEL_BIN:-missing} keel-daemon=${DAEMON_BIN:-missing}"
fi

# --- 2. the binary runs, having compiled nothing ----------------------------

if [ -n "$KEEL_BIN" ]; then
  version="$(clean_run "$KEEL_BIN" --version 2>&1)"
  if [ $? -eq 0 ] && [ -n "$version" ]; then
    ok "keel --version runs" "$version"
  else
    bad "keel --version runs" "$version"
  fi
else
  bad "keel --version runs" "no keel binary"
fi

# --- 3. the glibc floor -----------------------------------------------------
#
# The check that only a real distribution can answer. A binary linked against a
# newer glibc than the target distribution has does not degrade — it refuses to
# start, with a `GLIBC_2.xx not found` message that reads to a user like a
# corrupt download. Reading the required versions out of the ELF is better than
# waiting for someone on Debian stable to report it.

if [ -n "$KEEL_BIN" ] && command -v objdump >/dev/null 2>&1; then
  needed="$(objdump -T "$KEEL_BIN" 2>/dev/null | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1)"
  needed="${needed#GLIBC_}"
  if [ -z "$needed" ]; then
    caution "glibc floor is $GLIBC_FLOOR or lower" "no versioned glibc symbols found"
    note "statically linked, or objdump could not read it — check by hand"
  elif [ "$(printf '%s\n%s\n' "$needed" "$GLIBC_FLOOR" | sort -V | tail -1)" = "$GLIBC_FLOOR" ]; then
    ok "glibc floor is $GLIBC_FLOOR or lower" "needs at most GLIBC_$needed"
  else
    bad "glibc floor is $GLIBC_FLOOR or lower" "needs GLIBC_$needed"
    note "the build image moved. Anyone on an older distribution gets a binary that will not start,"
    note "with a message that reads like a corrupt download. Build Linux on the oldest supported image."
  fi
else
  bad "glibc floor is $GLIBC_FLOOR or lower" "no binary, or objdump is not installed"
  note "install binutils in the VM image; this is the check tier 2 exists for"
fi

# --- 4. store, daemon, and the API ------------------------------------------

KEEL_STORE="$CLEAN_HOME/.keel"

if [ -n "$KEEL_BIN" ]; then
  clean_run "$KEEL_BIN" --home "$KEEL_STORE" fsck >"$LOGS/fsck.log" 2>&1
  if [ $? -eq 0 ] && [ -f "$KEEL_STORE/keel.sqlite" ]; then
    ok "store created in scratch HOME" "$KEEL_STORE/keel.sqlite, fsck clean"
  else
    bad "store created in scratch HOME" "see $LOGS/fsck.log"
  fi
fi

if [ -n "$DAEMON_BIN" ]; then
  # `env` directly, never a shell function: backgrounding a function makes `$!`
  # the subshell's pid, and killing that orphans the daemon. Tier 1 shipped that
  # bug and left a daemon serving a deleted store.
  env -i "HOME=$CLEAN_HOME" "PATH=$CLEAN_PATH" \
    "$DAEMON_BIN" --home "$KEEL_STORE" --bind "127.0.0.1:$PORT" \
    >"$LOGS/daemon.log" 2>&1 &
  DAEMON_PID=$!

  health=""
  for _ in $(seq 1 15); do
    health="$(curl -sf --max-time 2 "http://127.0.0.1:$PORT/api/health" 2>/dev/null)"
    [ -n "$health" ] && break
    kill -0 "$DAEMON_PID" 2>/dev/null || break
    sleep 1
  done

  if [ -n "$health" ]; then
    ok "daemon answers /api/health" "$(echo "$health" | cut -c1-70)"
  else
    bad "daemon answers /api/health" "no answer in 15s"
    note "see $LOGS/daemon.log"
  fi

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
  bad "daemon answers /api/health" "no keel-daemon binary"
  bad "daemon stops on TERM" "no keel-daemon binary"
fi

# --- 5. systemd: it installs, and it comes back after a kill ----------------
#
# A *user* unit in the scratch, not a system one. The claim being tested is that
# the daemon survives being killed, which is what `Restart=` is for and what a
# user actually depends on; testing it as a system unit would need root and
# would leave something behind on a machine somebody works on.

if [ -z "$DAEMON_BIN" ]; then
  bad "systemd unit restarts the daemon" "no keel-daemon binary"
elif ! command -v systemctl >/dev/null 2>&1 || ! systemctl --user show-environment >/dev/null 2>&1; then
  bad "systemd unit restarts the daemon" "no user systemd session in this VM"
  note "tier 2 is meant to cover the service story; a VM without a user bus cannot answer it"
  note "log in properly rather than over a bare ssh exec, or enable lingering for this user"
else
  mkdir -p "$HOME/.config/systemd/user"
  cat > "$HOME/.config/systemd/user/keel-tier2.service" <<UNIT
[Unit]
Description=Keel daemon (tier 2 verification)

[Service]
Environment=HOME=$CLEAN_HOME
ExecStart=$DAEMON_BIN --home $KEEL_STORE --bind 127.0.0.1:$PORT
Restart=always
RestartSec=1

[Install]
WantedBy=default.target
UNIT
  UNIT_INSTALLED=1
  systemctl --user daemon-reload >/dev/null 2>&1

  if systemctl --user start keel-tier2.service >/dev/null 2>&1; then
    up=""
    for _ in $(seq 1 15); do
      curl -sf --max-time 2 "http://127.0.0.1:$PORT/api/health" >/dev/null 2>&1 && { up=yes; break; }
      sleep 1
    done
    if [ -z "$up" ]; then
      bad "systemd unit restarts the daemon" "the unit started but nothing answered"
      systemctl --user status keel-tier2.service >"$LOGS/systemd.log" 2>&1
      note "see $LOGS/systemd.log"
    else
      first_pid="$(systemctl --user show -p MainPID --value keel-tier2.service 2>/dev/null)"
      # SIGKILL, not TERM: the question is whether it comes back from a crash,
      # and a graceful stop is the case that already works.
      kill -9 "$first_pid" 2>/dev/null
      back=""
      for _ in $(seq 1 20); do
        sleep 1
        second_pid="$(systemctl --user show -p MainPID --value keel-tier2.service 2>/dev/null)"
        [ -n "$second_pid" ] && [ "$second_pid" != "0" ] && [ "$second_pid" != "$first_pid" ] \
          && curl -sf --max-time 2 "http://127.0.0.1:$PORT/api/health" >/dev/null 2>&1 \
          && { back=yes; break; }
      done
      if [ -n "$back" ]; then
        ok "systemd unit restarts the daemon" "killed $first_pid, back as $second_pid"
      else
        bad "systemd unit restarts the daemon" "killed $first_pid and it did not come back in 20s"
        systemctl --user status keel-tier2.service >"$LOGS/systemd.log" 2>&1
        note "see $LOGS/systemd.log"
      fi
    fi
  else
    bad "systemd unit restarts the daemon" "the unit would not start"
  fi
fi

# --- 6. the checksum refusal path, on Linux ---------------------------------
#
# Worth repeating here rather than trusting tier 1, because the tools differ:
# Linux has `sha256sum` from coreutils and often has no `shasum` at all, so this
# exercises the other branch of the patched installer entirely.

MIRROR="$SCRATCH/mirror"
archives="$LOCAL_ARCHIVES"
if [ -z "$archives" ]; then
  bad "installer refuses a corrupt archive" "no archives beside the installer to corrupt"
  note "pass the artifact directory rather than the bare installer script"
else
  mkdir -p "$MIRROR"
  cp -R "$ARTIFACT_DIR"/. "$MIRROR"/ 2>/dev/null
  corrupted=0
  while IFS= read -r a; do
    [ -n "$a" ] || continue
    m="$MIRROR/$(basename "$a")"
    [ -f "$m" ] || continue
    size="$(stat -c %s "$m" 2>/dev/null || echo 0)"
    [ "$size" -gt 256 ] || continue
    dd if=/dev/urandom of="$m" bs=1 seek=$((size / 2)) count=64 conv=notrunc \
      >/dev/null 2>&1 && corrupted=$((corrupted + 1))
  done <<<"$archives"

  DIRTY_HOME="$SCRATCH/home-bad"
  mkdir -p "$DIRTY_HOME"
  if [ "$corrupted" = "0" ]; then
    bad "installer refuses a corrupt archive" "could not corrupt any archive"
  else
    env -i "HOME=$DIRTY_HOME" "PATH=$CLEAN_PATH" \
      "INSTALLER_DOWNLOAD_URL=file://$MIRROR" sh "$INSTALLER" \
      >"$LOGS/corrupt.log" 2>&1
    corrupt_status=$?
    landed="$(find "$DIRTY_HOME" -type f -executable -name keel 2>/dev/null | head -1)"
    if [ $corrupt_status -eq 0 ] || [ -n "$landed" ]; then
      bad "installer refuses a corrupt archive" "IT ACCEPTED IT (exit $corrupt_status)"
      note "a corrupted archive installed. This release must not ship."
    elif grep -Eqi 'checksum|sha256|verif|mismatch|corrupt' "$LOGS/corrupt.log"; then
      ok "installer refuses a corrupt archive" "exit $corrupt_status, refused on the checksum"
    else
      bad "installer refuses a corrupt archive" "exit $corrupt_status, but no checksum language"
      note "it failed for some other reason, so the refusal path is unproven"
    fi
  fi
fi

# --- summary ----------------------------------------------------------------

printf '\n  %d passed, %d failed, %d warning(s)\n' "$pass" "$fail" "$warn"
if [ "$fail" -eq 0 ]; then
  printf '  tier 2 passed. With tier 1 green on the Mac, both shipped platforms\n'
  printf '  have now run the release. The cross-compile signature trap is still\n'
  printf '  only covered by building macOS on macOS, which tier 3 would confirm.\n\n'
  exit 0
else
  printf '  tier 2 did not pass. Re-run with KEEP=1 to keep %s/logs for reading.\n\n' "$SCRATCH"
  exit 1
fi
