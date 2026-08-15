#!/usr/bin/env bash
#
# Register this Mac as a self-hosted GitHub Actions runner for the repository.
#
# Why this exists (B-72): the repository is private, hosted macOS minutes are
# billed at ten times the Linux rate, and the alternative — making the
# repository public to get them free — publishes an event to every follower and
# has no undo. What §2 actually requires is Apple's linker, not GitHub's
# hardware: an Apple Silicon Mac links with Apple's `cc`, which carries the
# ad-hoc signature that stops the kernel killing the process at exec.
#
# Linux stays on GitHub's hosted runners. This only replaces the macOS half.
#
#   scripts/install-runner.sh              # install and start as a service
#   scripts/install-runner.sh --no-service # install, run in the foreground
#   scripts/install-runner.sh --remove     # unregister and delete
#
# ## What this does to your machine
#
# Creates `~/.keel-runner`, downloads GitHub's runner release into it, registers
# it against this repository, and — unless `--no-service` — installs a launchd
# agent so it starts with your login. `--remove` reverses all of it.
#
# ## The registration token
#
# Minted at run time through `gh` and passed straight to `config.sh`. It is
# never written to a file and it expires in an hour. That is deliberate: a
# runner token in a script or an environment file is a credential that lets
# anyone register a machine to execute this repository's workflows.
#
# ## Why this is safe here and would not be on a public repository
#
# A self-hosted runner executes whatever a workflow says. On a public
# repository a pull request from a stranger is a workflow, so it is remote code
# execution on your Mac by design. On a private repository only people with
# access can trigger it. **If this repository is ever made public, remove this
# runner first.**

set -euo pipefail

REPO="${KEEL_RUNNER_REPO:-kiritbasu/keel}"
RUNNER_DIR="${KEEL_RUNNER_DIR:-$HOME/.keel-runner}"
SERVICE=1

for arg in "$@"; do
    case "$arg" in
        --no-service) SERVICE=0 ;;
        --remove)     REMOVE=1 ;;
        -h|--help)    sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *)            echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

die() { echo "install-runner: $*" >&2; exit 1; }
say() { printf '  %s\n' "$*"; }

command -v gh >/dev/null || die "the GitHub CLI is not installed"
gh auth status >/dev/null 2>&1 || die "gh is not authenticated — run 'gh auth login'"

# --- removal ----------------------------------------------------------------

if [ "${REMOVE:-0}" = 1 ]; then
    [ -d "$RUNNER_DIR" ] || die "$RUNNER_DIR does not exist; nothing to remove"
    echo "Removing the runner at $RUNNER_DIR"
    if [ -f "$RUNNER_DIR/svc.sh" ]; then
        (cd "$RUNNER_DIR" && ./svc.sh stop  >/dev/null 2>&1) || true
        (cd "$RUNNER_DIR" && ./svc.sh uninstall >/dev/null 2>&1) || true
        say "service stopped and uninstalled"
    fi
    # A removal token, not a registration token. Same handling: never stored.
    token="$(gh api -X POST "repos/$REPO/actions/runners/remove-token" --jq .token)" \
        || die "could not mint a removal token for $REPO"
    (cd "$RUNNER_DIR" && ./config.sh remove --token "$token") || true
    rm -rf "$RUNNER_DIR"
    say "unregistered and deleted"
    exit 0
fi

# --- preconditions ----------------------------------------------------------

[ "$(uname -s)" = "Darwin" ] || die "this registers a *macOS* runner; run it on the Mac"

arch="$(uname -m)"
case "$arch" in
    arm64) pkg_arch="osx-arm64" ;;
    # An Intel Mac would register with the label x64, and the workflow asks for
    # ARM64. Refusing here is better than a job that queues forever against a
    # label nothing carries.
    x86_64) die "the release workflow asks for an ARM64 runner; this Mac reports x86_64" ;;
    *) die "unexpected architecture: $arch" ;;
esac

# The toolchain has to be on the machine, not installed per job: the workflow
# adds a target with rustup and expects cargo to exist.
command -v rustup >/dev/null || die "rustup is not installed — the runner needs the Rust toolchain"
say "rustup $(rustup --version 2>/dev/null | head -1 | awk '{print $2}')"

# Only the arm64 target. Intel macOS was dropped from the release when 0.1.0
# was cut: `ort-sys` has no prebuilt ONNX Runtime for it, so the binary cannot
# link at all. Adding the target here would install a std nothing builds
# against and imply a platform that does not ship.
rustup target add aarch64-apple-darwin >/dev/null
say "target aarch64-apple-darwin present"

if [ -d "$RUNNER_DIR" ]; then
    die "$RUNNER_DIR already exists — run '$0 --remove' first"
fi

# --- fetch ------------------------------------------------------------------

echo "Installing a self-hosted runner for $REPO"

version="$(gh api repos/actions/runner/releases/latest --jq .tag_name | sed 's/^v//')" \
    || die "could not find the latest runner release"
say "runner version $version"

mkdir -p "$RUNNER_DIR"
tarball="actions-runner-$pkg_arch-$version.tar.gz"
url="https://github.com/actions/runner/releases/download/v$version/$tarball"

# The checksum is published in the release body as a table. Rather than parse
# prose, verify against the digest GitHub serves for the asset itself.
say "downloading $tarball"
curl --proto '=https' --tlsv1.2 -fsSL "$url" -o "$RUNNER_DIR/$tarball" \
    || die "could not download $url"

expected="$(gh api "repos/actions/runner/releases/tags/v$version" \
    --jq ".assets[] | select(.name == \"$tarball\") | .digest" 2>/dev/null || true)"
if [ -n "$expected" ] && [ "$expected" != "null" ]; then
    actual="sha256:$(shasum -a 256 "$RUNNER_DIR/$tarball" | awk '{print $1}')"
    [ "$actual" = "$expected" ] || die "checksum mismatch for $tarball
  want: $expected
  got:  $actual"
    say "checksum verified"
else
    # Loud, not silent. This repository has been bitten twice by a verification
    # step that reported success while doing nothing.
    say "WARNING: GitHub published no digest for this asset; the download is unverified"
fi

tar -xzf "$RUNNER_DIR/$tarball" -C "$RUNNER_DIR"
rm -f "$RUNNER_DIR/$tarball"

# --- register ---------------------------------------------------------------
#
# Labels are left at the defaults, which for this machine are exactly
# `self-hosted`, `macOS` and `ARM64` — the three the workflows ask for. Nothing
# to remember to type, and nothing to get wrong.

token="$(gh api -X POST "repos/$REPO/actions/runners/registration-token" --jq .token)" \
    || die "could not mint a registration token for $REPO — do you have admin on it?"

(cd "$RUNNER_DIR" && ./config.sh \
    --url "https://github.com/$REPO" \
    --token "$token" \
    --name "$(scutil --get ComputerName 2>/dev/null || hostname)" \
    --work _work \
    --unattended \
    --replace)

say "registered against $REPO"

if [ "$SERVICE" = 1 ]; then
    (cd "$RUNNER_DIR" && ./svc.sh install >/dev/null && ./svc.sh start >/dev/null)
    say "installed as a launchd service; it starts at login"
else
    echo
    echo "Not installed as a service. Start it in the foreground with:"
    echo "  (cd $RUNNER_DIR && ./run.sh)"
fi

echo
echo "Done. Check it is listed:"
echo "  gh api repos/$REPO/actions/runners --jq '.runners[] | {name, status, labels: [.labels[].name]}'"
echo
echo "The Mac must be awake for a macOS job to start. A job with no runner"
echo "queues rather than failing, so a run stuck at 'waiting' means this is down."
