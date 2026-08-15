#!/bin/sh
#
# The only shell left, and it stays for one reason: a hook that *is* the binary
# cannot report the binary's absence.
#
# Between installing the plugin and running `/keel:setup` there is no `keel` on
# the machine, and the session should say so rather than starting silently
# unoriented. That sentence has to come from something that runs without Keel —
# so it comes from here, and everything that can change is on the other side of
# the `exec`.
#
# Deliberately dependency-free: `sh`, not `bash`; no `python3`, no `curl`, no
# `jq`. That is the whole point of KEEL-206 — the scripts this replaces needed
# python3 and curl, neither declared anywhere, and python3 is absent on a Mac
# until the Xcode command line tools arrive. Every failure path exited 0
# silently, so on a fresh machine the hooks did nothing and it looked exactly
# like Keel being broken.
#
# Nothing here holds logic worth testing. Everything that does is in
# `keel hook`, with `crates/keel/tests/hooks.rs` running it for real.
#
# Usage: keel-hook.sh session-start | stop

set -u

event="${1:-}"
[ -n "$event" ] || exit 0

# Where the installer puts it, then whatever is on PATH. `command -v` rather
# than a hardcoded path so a non-standard install still works.
keel="${KEEL_BIN:-}"
if [ -z "$keel" ]; then
    for candidate in "$HOME/.local/bin/keel" "$HOME/.cargo/bin/keel"; do
        [ -x "$candidate" ] && keel="$candidate" && break
    done
fi
[ -n "$keel" ] || keel="$(command -v keel 2>/dev/null || true)"

if [ -n "$keel" ] && [ -x "$keel" ]; then
    # Run rather than `exec`, so a binary that is present but too old to know
    # `hook` cannot speak for this script.
    #
    # That state is not hypothetical — it is exactly what an upgrade looks like
    # between updating the plugin and updating the binary. `exec` hands clap's
    # "unrecognized subcommand" straight to Claude Code, and for a Stop hook a
    # non-zero exit means *block, using stderr as the reason* — so a stale
    # binary would inject a usage message as a blocking instruction. Swallowing
    # it costs one buffered string and turns that into silence.
    output="$("$keel" hook "$event" 2>/dev/null)" || exit 0
    [ -n "$output" ] && printf '%s\n' "$output"
    exit 0
fi

# No binary. Say so once, at session start, and say nothing at all on stop —
# a session that is ending is the wrong moment to be told about installation,
# and `Stop` output would block it.
[ "$event" = "session-start" ] || exit 0

printf '%s\n' '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"The Keel plugin is installed but the `keel` binary is not on this machine yet, so this session has no project context and the keel_* tools will not answer. Run /keel:setup to install it, then restart Claude Code."}}'
