---
description: Install Specline — download the binaries, create the store, and start the daemon.
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

Relay the "What leaves your machine" paragraph too, rather than summarising it
away. Specline checks hourly for a new release and that is the only request it
makes; the person installing a local-first tool is entitled to hear about it
from the tool rather than find it later. `--no-update-check` turns it off at
install time and `SPECLINE_AUTO_UPDATE=0` afterwards — if they say they would rather
not have it, re-run the script with that flag rather than explaining how to edit
a service file.

If the script reports that the download returned 404, relay its instructions
verbatim. That means either no release has been published or the repository is
private and the download needs a token — and which of the two it is is not
something to guess at on the user's behalf.
