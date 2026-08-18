<!-- specline:generated decision dec_01M09W16C249HGJQC0QGD7GYCZ v1 2026-08-18T07:23:42Z
     source of truth is Specline — edits here are not saved -->
# B-84 — The landing page lives in this repository, at site/, with no build step

**Status:** `accepted`  
**Id:** `dec_01M09W16C249HGJQC0QGD7GYCZ`

Specline needed a page a person could be sent to. It is built from this repository rather than one of its own, and it is one HTML file and one stylesheet rather than a site generator.

## Where

Same repository, `site/`, published by GitHub Pages from a workflow.

The page exists to get somebody to install Specline, and the install story lives here: three commands, two of which name `kiritbasu/specline`. That story has already changed once — the plugin flow replaced a `claude mcp add`. The worst thing this page can do is tell a visitor to run a command that no longer works, and a repository boundary between the instructions and the thing they install is how that happens.

Everything else it needs is here too. The four screenshots are generated into `docs/images` by `scripts/shoot-screenshots.mjs`, and the two Geist faces already ship with the desktop app. Across two repositories those become copies, and a copied screenshot is a screenshot of an old version within two releases with nothing to tell you.

The argument for a separate repository is the URL — `kiritbasu.github.io` serves the bare domain and a project repository serves a `/specline/` path. A custom domain solves that from either, and that namespace is KB's own rather than this project's.

## How

No framework, no bundler, no package manager. `scripts/build-site.sh` copies the page, the screenshots and the fonts into `site/_site` and that is the whole build. A toolchain would have been more moving parts than the thing it built, and the page has no state, no routing and four images.

The screenshots and the fonts are copied at build time rather than committed a second time. That is the same reasoning as everywhere else here: one canonical copy, and the copy is made by something that runs, not by a person remembering.

## Two things worth knowing

**The stylesheet is the app's.** The colours, the typeface and the radii come from `apps/desktop/src/styles.css`, because two thirds of the page is screenshots of the app and a page whose chrome disagrees with the pictures inside it looks like somebody else's page. The page adds a display type scale of its own — the app's largest size is 24px, which is right for something that has to sit in a table and wrong for a sentence read from across the room.

**Pages needs its own workflow.** It wants `pages: write` and `id-token: write`. `ci.yml` declares `contents: read` and explains in a comment that this stops a change to the default quietly handing a write token to every job in the file, including the ones that build a pull request's code. Publishing from there would have undone that, so publishing has a file of its own.

## Reversible?

Yes. It is four files and nothing else in the tree imports them.

