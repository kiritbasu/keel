// The desktop shell is suspended, deliberately and reversibly.
//
// The read surface people are actually looking at right now is the web build:
// the same React bundle, served by Vite on :1420, talking to the daemon over
// the `/api` proxy. Nothing in `apps/desktop/src` imports a Tauri API, so the
// shell adds a webview dependency tree and about 1.2 GB of build output for a
// surface nobody has open. This guard is here because that cost is invisible
// until the disk is full — which is how it was found.
//
// To build it anyway: `KEEL_DESKTOP=1 cargo build` (or `npm run tauri`, once
// the CLI is back). Delete this block when the shell comes off the shelf.
fn main() {
    if std::env::var_os("KEEL_DESKTOP").is_none() {
        eprintln!(
            "keel-desktop: the Tauri shell is suspended — work is on the web build \
             (`npm run dev` in apps/desktop, daemon on :7654).\n\
             Set KEEL_DESKTOP=1 to build it anyway, or remove the guard in \
             apps/desktop/src-tauri/build.rs to un-suspend it for good."
        );
        std::process::exit(1);
    }

    tauri_build::build()
}
