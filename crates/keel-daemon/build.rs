//! Make sure there is something for `rust_embed` to embed, and rebuild when it
//! changes.
//!
//! Two jobs, both of which exist because the site is built by a different tool
//! than the one building this crate.
//!
//! **The directory has to exist at compile time.** `rust_embed`'s `folder`
//! attribute is resolved by a proc macro, so a missing `apps/desktop/dist` is a
//! compile error in a file nobody edited, with a message about a path rather
//! than about Node. That would break `cargo test --workspace` for anyone who
//! has not run `npm run build`, and it would break CI, which tests the
//! workspace without going near the site. So a placeholder is written when the
//! real build is absent.
//!
//! The placeholder says what happened, in the page itself. A blank page or a
//! 404 would be indistinguishable from a broken daemon, and this project has
//! been bitten twice by things that fail in a way that looks like something
//! else. Anyone who sees it gets the command that fixes it.
//!
//! **Cargo cannot see the site.** Nothing in the Rust dependency graph mentions
//! `apps/desktop`, so rebuilding the site would not rebuild this crate and the
//! daemon would keep serving the previous bundle — the kind of staleness that
//! costs an hour before anyone suspects the build rather than the code. The
//! `rerun-if-changed` lines below are what connect them.

use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let dist = manifest.join("../../apps/desktop/dist");

    // Watch the built output, so `npm run build` triggers a recompile. Watching
    // `src` instead would rebuild on an edit that has not been built yet, which
    // would embed the *old* bundle and report success.
    println!("cargo:rerun-if-changed={}", dist.display());
    println!("cargo:rerun-if-changed=build.rs");

    if dist.join("index.html").is_file() {
        return;
    }

    // No site. Write the placeholder rather than failing the build: a daemon
    // that serves MCP perfectly is still worth having on a machine with no
    // Node, and the release workflow builds the site before it builds this.
    println!(
        "cargo:warning=apps/desktop/dist has no index.html, so the daemon will serve a \
         placeholder page. Run `npm ci --prefix apps/desktop && npm run build --prefix \
         apps/desktop` and rebuild to embed the real interface."
    );

    if let Err(e) = write_placeholder(&dist) {
        // A warning, not a panic. Failing the build here would mean a
        // read-only filesystem or a permissions problem stops the daemon
        // compiling at all, which is a worse outcome than no interface.
        println!("cargo:warning=could not write the placeholder site: {e}");
    }
}

fn write_placeholder(dist: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dist)?;
    std::fs::write(
        dist.join("index.html"),
        "<!doctype html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <title>Keel — interface not built</title>\n\
         <style>\n\
         body{font:16px/1.6 ui-sans-serif,system-ui,sans-serif;max-width:34rem;\
         margin:12vh auto;padding:0 1.5rem;color:#111}\n\
         code{background:#f4f4f5;padding:.15em .4em;border-radius:.25rem;\
         font:14px ui-monospace,monospace}\n\
         pre{background:#f4f4f5;padding:1rem;border-radius:.4rem;overflow-x:auto}\n\
         @media(prefers-color-scheme:dark){body{background:#0b0b0c;color:#e7e7e9}\n\
         code,pre{background:#1c1c1f}}\n\
         </style>\n\
         </head>\n\
         <body>\n\
         <h1>The interface was not compiled in</h1>\n\
         <p>This daemon is working. Its API and MCP endpoint are fine — what is \
         missing is the built site, which is a separate build step.</p>\n\
         <pre>npm ci --prefix apps/desktop\nnpm run build --prefix apps/desktop\ncargo build --release</pre>\n\
         <p>Then restart the daemon. A release build always has the real \
         interface: the release workflow builds the site first.</p>\n\
         <p>Meanwhile <code>/api/health</code> answers, and so does everything \
         else under <code>/api</code>.</p>\n\
         </body>\n\
         </html>\n",
    )
}
