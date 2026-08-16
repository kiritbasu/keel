# Security

## Reporting a vulnerability

Use GitHub's private reporting: **[Report a vulnerability](https://github.com/kiritbasu/keel/security/advisories/new)**.
It is enabled on this repository, so the report reaches the maintainer without
being public first. Please do not open an ordinary issue for anything you think
is exploitable.

There is one maintainer. Expect an acknowledgement within a week; if a fix is
warranted it ships in the next release, and the advisory is published with it.
If a week passes with silence, open a public issue saying only that you sent a
private report and heard nothing — no details.

## What Specline is, in terms of what that risks

Specline is a local-first store. The daemon binds `127.0.0.1` and holds one SQLite
file under `~/.specline`. There is no account, no server, and nothing multi-tenant,
so the threat model is not the usual one for a web application. What is worth
reporting:

- **Anything that reaches the store from outside the machine.** The MCP endpoint
  is the daemon's one write path, and it is loopback-only by design.
  `--allow-network-access` is deliberate and documented; a way past the binding
  without it is not.
- **Anything that reads or writes a file outside the paths Specline is supposed to
  touch.** Stored values name files Specline later writes, and they arrive from a
  model that can be prompt-injected. `crate::safe_path` exists for exactly this
  and a way around it is a real finding.
- **Anything that makes the daemon act on instructions found in content rather
  than from its caller** — a document body, a task title, a file it read.
- **The update path.** Releases are verified against the SHA-256 in the release
  manifest. A way to make Specline install bytes that do not match, or to make it
  fetch from somewhere else, matters.

Known and not a finding: the daemon trusts anything that can already reach
loopback on your machine, and provenance is cooperative — a caller supplies its
own `session_id`, so it says who *claims* to have written something.

## Supported versions

The latest release. Specline is 0.x and there are no maintained branches behind it;
a fix means a new release rather than a backport.
