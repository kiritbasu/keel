//! The read surface, compiled into the binary and served from the same port as
//! the API.
//!
//! # Why this rather than a desktop app
//!
//! A `.dmg` downloaded through a browser carries `com.apple.quarantine`, and
//! clearing that needs Developer ID signing plus notarization — $99 a year with
//! a signing pipeline to maintain, and macOS Sequoia removed the Control-click
//! bypass. Tauri also wants Node, the Xcode command line tools, WebView2 and
//! webkit2gtk, and does not meaningfully cross-compile, so it is a second full
//! release pipeline with a runner per platform.
//!
//! Serving a local page instead costs a dock icon and native menus, and is what
//! Jupyter, Syncthing, Grafana, Meilisearch, Qdrant, code-server and pgAdmin all
//! do. PHASE-10 §3 has the full argument; B-69 confirmed that a **read-only**
//! page does not touch hard constraint 7, which is what unblocked this.
//!
//! # The headers, and why they are not decoration
//!
//! KEEL-168 gates this work on a specific list, and the reasoning is worth
//! keeping in front of whoever edits this next: **document bodies and blobs in
//! this store are written by an agent that was reading prose it did not
//! write.** A prompt-influenced write is a plausible source of hostile markup,
//! and the moment a browser renders it from the daemon's own origin, that is
//! stored cross-site scripting against an API with no authentication.
//!
//! So the page is served with a content security policy that allows exactly
//! what the bundle needs and nothing else — no inline script, no remote origin,
//! no framing — plus `nosniff` so a declared type is a fact rather than a
//! suggestion. `/api/blob/{id}` already carries `default-src 'none'; sandbox`
//! for the same reason, which is why an SVG a model wrote cannot execute.
//!
//! What is deliberately **not** solved here: `POST /api/generate` is a mutating
//! endpoint, it writes files into the user's repository, and it has no token.
//! CORS restricts it to local origins, which means any other page the user has
//! open on `localhost` can reach it. That is KEEL-168's first item and it is
//! unchanged by this module — serving the read surface neither introduces nor
//! fixes it. It is more visible now, which is a reason to do it, not a reason to
//! pretend it arrived with this.

use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

/// The built site from `apps/desktop/dist`.
///
/// `build.rs` guarantees the directory exists — writing a placeholder page when
/// the real build is absent — so this compiles on a machine that has never had
/// Node, and so `cargo test --workspace` works in CI without a site build.
#[derive(rust_embed::Embed)]
#[folder = "../../apps/desktop/dist"]
struct Site;

/// What the page is allowed to do.
///
/// Written out rather than borrowed from a framework default, because every
/// clause here is answering something specific:
///
/// - `default-src 'self'` — nothing loads from anywhere but this daemon.
/// - `script-src 'self'` — **no `unsafe-inline`, no `unsafe-eval`.** This is the
///   clause that makes stored markup in a document body inert. Vite emits the
///   bundle as an external module, so nothing legitimate needs an inline
///   script; if a future build starts inlining one, this breaks loudly rather
///   than being relaxed quietly.
/// - `style-src 'self' 'unsafe-inline'` — the honest exception. React sets
///   inline styles and Tailwind injects a style element, so this cannot be
///   tightened without hashing every style the app emits. Inline *style* cannot
///   execute; it can only make something look wrong.
/// - `img-src 'self' data: blob:` — images come from `/api/blob`, which is
///   already sandboxed, and from data URLs the app builds itself.
/// - `connect-src 'self'` — the API and the SSE stream, and nowhere else. This
///   is what stops injected script exfiltrating the store to another host.
/// - `object-src 'none'`, `frame-ancestors 'none'`, `base-uri 'self'` — no
///   plugins, nothing may frame this page, and injected markup cannot retarget
///   every relative URL on it by rewriting the base.
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; \
     script-src 'self'; \
     style-src 'self' 'unsafe-inline'; \
     img-src 'self' data: blob:; \
     font-src 'self'; \
     connect-src 'self'; \
     object-src 'none'; \
     frame-ancestors 'none'; \
     base-uri 'self'";

/// Serve a file from the embedded site, falling back to `index.html`.
///
/// The fallback is what makes a deep link work. The app routes on the hash
/// (`#/projects/keel`), so the server sees `/` for every screen and the
/// fallback is rarely reached — but a stale bookmark from the Tauri build, or a
/// future move to history routing, both land here, and answering 404 would show
/// a browser error page for a route the app knows perfectly well.
///
/// A request for a *missing asset* is a different thing and must not be
/// disguised: anything under a path with a file extension gets a real 404, so a
/// broken bundle reference fails as a broken bundle reference rather than as an
/// HTML page arriving where JavaScript was expected — which is a confusing
/// syntax error rather than a missing file.
pub async fn serve(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
    uri: Uri,
) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    // An API path that reached the fallback is one no route claimed, and
    // answering it with the app shell is worse than useless: a client that
    // mistyped an endpoint, or posted to one that does not exist, gets HTML and
    // a 200 and has to work out from the body that nothing happened.
    //
    // The router's comment beside this fallback has always claimed a typo'd
    // API route "still 404s as an API call rather than silently returning the
    // app shell". It did not, until KEEL-240 wrote a test that assumed the
    // comment was true and found out.
    if path == "api" || path.starts_with("api/") {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            format!("{{\"error\":{{\"code\":-32601,\"message\":\"no API endpoint at /{path}\"}}}}"),
        )
            .into_response();
    }

    if let Some(file) = Site::get(path) {
        return respond(path, file, state.token());
    }

    // A path whose last segment has a dot is asking for a file, not a route.
    if path
        .rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'))
    {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("{path} is not part of the built interface\n"),
        )
            .into_response();
    }

    match Site::get("index.html") {
        Some(file) => respond("index.html", file, state.token()),
        // Only reachable if the embed is empty, which `build.rs` prevents.
        // Saying so plainly beats a bare 404 that reads as a routing bug.
        None => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "no interface is compiled into this daemon. Build it with \
             `npm run build --prefix apps/desktop` and rebuild.\n",
        )
            .into_response(),
    }
}

fn respond(path: &str, file: rust_embed::EmbeddedFile, token: &str) -> Response {
    let mime = file.metadata.mimetype();

    // **This is how the interface gets its token, and the delivery is the
    // security property.** The page can write to the daemon; a page served by
    // anything else cannot, because it has no way to read this response — the
    // same-origin policy is doing the work, not the header.
    //
    // Only `index.html`. An asset carrying the secret would be cached
    // immutably, which is the opposite of a token that lives for one daemon.
    let body = if path == "index.html" {
        let html = String::from_utf8_lossy(&file.data);
        // Escaped, though the token is hex from the operating system and cannot
        // contain a quote. Building markup by concatenation is a habit worth
        // not having, and the day the token format changes is the day this
        // would have been an injection point.
        let meta = format!(
            "<meta name=\"keel-token\" content=\"{}\">",
            token.replace('&', "&amp;").replace('"', "&quot;")
        );
        Body::from(match html.split_once("<head>") {
            Some((before, after)) => format!("{before}<head>{meta}{after}"),
            // No `<head>` means this is not the document the build produces.
            // Serving it unmodified is right: the app will say it has no token
            // when it tries to write, which is a legible failure, where a
            // mangled document is not.
            None => html.into_owned(),
        })
    } else {
        Body::from(file.data)
    };

    // Hashed asset names are content-addressed by Vite, so they can be cached
    // hard. `index.html` never can: it is what points at the current hashes, and
    // a cached copy after an update points at bundles that no longer exist —
    // which presents as a blank page with a console error, the least
    // diagnosable failure this could have.
    let cache = if path == "index.html" {
        "no-cache"
    } else {
        "public, max-age=31536000, immutable"
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime.to_owned()),
            (header::CACHE_CONTROL, cache.to_owned()),
            (
                header::CONTENT_SECURITY_POLICY,
                CONTENT_SECURITY_POLICY.to_owned(),
            ),
            (
                header::HeaderName::from_static("x-content-type-options"),
                "nosniff".to_owned(),
            ),
            // Belt and braces with `frame-ancestors`, which older browsers
            // ignore. Cheap, and this is a page with an unauthenticated API
            // behind it.
            (
                header::HeaderName::from_static("x-frame-options"),
                "DENY".to_owned(),
            ),
            // A local page has no business telling anyone where its links came
            // from, and the paths carry entity ids.
            (header::REFERRER_POLICY, "no-referrer".to_owned()),
        ],
        body,
    )
        .into_response()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    async fn get(path: &str) -> Response {
        let uri: Uri = path.parse().unwrap();
        let store = keel_core::Store::in_memory().expect("an in-memory store");
        let state = crate::state::AppState::from_store_with_token(store, TEST_TOKEN);
        serve(axum::extract::State(state), uri).await
    }

    /// A token the injection test can look for. Any value will do; what is
    /// being checked is that the page carries the one the daemon holds.
    const TEST_TOKEN: &str = "token-for-the-page";

    fn header_of(response: &Response, name: &str) -> String {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    }

    /// The root serves something, whether that is the real bundle or the
    /// placeholder `build.rs` writes.
    #[tokio::test]
    async fn the_root_serves_a_page() {
        let response = get("/").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(header_of(&response, "content-type").contains("text/html"));
    }

    /// The clause that makes stored markup inert. If someone relaxes this to
    /// get an inline script working, this test is the argument against it.
    #[tokio::test]
    async fn the_page_forbids_inline_and_remote_script() {
        let csp = header_of(&get("/").await, "content-security-policy");
        assert!(csp.contains("script-src 'self'"), "{csp}");
        assert!(
            !csp.contains("unsafe-inline; ") && !csp.contains("script-src 'self' 'unsafe-inline'"),
            "inline script must stay forbidden — document bodies in this store are \
             written by an agent reading prose it did not write: {csp}"
        );
        assert!(
            !csp.contains("unsafe-eval"),
            "eval must stay forbidden: {csp}"
        );
    }

    /// Exfiltration is the failure that matters if script ever does run, so the
    /// page may talk to this daemon and nowhere else.
    #[tokio::test]
    async fn the_page_may_only_talk_to_this_daemon() {
        let csp = header_of(&get("/").await, "content-security-policy");
        assert!(csp.contains("connect-src 'self'"), "{csp}");
        assert!(csp.contains("frame-ancestors 'none'"), "{csp}");
        assert!(csp.contains("object-src 'none'"), "{csp}");
    }

    #[tokio::test]
    async fn every_page_response_carries_nosniff() {
        assert_eq!(
            header_of(&get("/").await, "x-content-type-options"),
            "nosniff"
        );
    }

    /// A route the app knows falls back to the shell.
    #[tokio::test]
    async fn an_unknown_route_serves_the_app_shell() {
        let response = get("/projects/keel/board").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(header_of(&response, "content-type").contains("text/html"));
    }

    /// A missing *asset* must not. Serving HTML where JavaScript was asked for
    /// turns a missing file into a syntax error, which is a much worse hour.
    #[tokio::test]
    async fn a_missing_asset_is_a_real_404() {
        let response = get("/assets/does-not-exist-a1b2c3.js").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    /// `index.html` names the current bundle hashes, so a cached copy after an
    /// update points at files that are gone — a blank page with a console
    /// error, and nothing to suggest the cache.
    #[tokio::test]
    async fn the_shell_is_not_cached_even_though_assets_are() {
        assert_eq!(header_of(&get("/").await, "cache-control"), "no-cache");
    }
}
