// The desktop shell is suspended, deliberately and reversibly.
//
// The read surface is the same React bundle, and since KEEL-207 the daemon
// serves it directly — compiled in with `rust-embed`, opened by `keel ui`. It
// was Vite on :1420 when this guard was written; that is now the development
// loop rather than the product.
//
// Nothing in `apps/desktop/src` imports a Tauri API, so the shell adds a webview
// dependency tree and about 1.2 GB of build output for a surface nobody has
// open. This guard is here because that cost is invisible until the disk is
// full — which is how it was found.
//
// The case against reviving it got stronger rather than weaker: a `.dmg`
// downloaded through a browser is quarantined, and clearing that needs Developer
// ID signing plus notarization at $99 a year, with no Control-click bypass since
// Sequoia. A local page costs a dock icon and native menus instead.
//
// To build it anyway: `KEEL_DESKTOP=1 cargo build`. Delete this block when the
// shell comes off the shelf.
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
