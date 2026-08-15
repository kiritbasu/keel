---
description: Install Keel — download the binaries, create the store, and start the daemon.
allowed-tools: Bash(${CLAUDE_PLUGIN_ROOT}/scripts/setup.sh:*)
---

Run this and nothing else:

```
${CLAUDE_PLUGIN_ROOT}/scripts/setup.sh
```

Show the user its output as it goes. Do not substitute your own steps for it,
do not run parts of it separately, and do not carry on to anything else if it
fails — the script says what went wrong and what to do about it, and a second
opinion improvised here would only compete with that.

When it finishes, tell the user in one line that they need to **restart Claude
Code**: MCP servers are connected at startup, so the `keel_*` tools will not
appear in this session however well the install went.

If the script reports that the download returned 404, relay its instructions
verbatim. That means either no release has been published or the repository is
private and the download needs a token — and which of the two it is is not
something to guess at on the user's behalf.
