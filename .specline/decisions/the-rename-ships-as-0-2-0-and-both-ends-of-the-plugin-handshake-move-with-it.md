<!-- specline:generated decision dec_01M05M4BG07J97TBZVBRSEWX2P v1 2026-08-16T15:48:36Z
     source of truth is Specline — edits here are not saved -->
# B-82 — The rename ships as 0.2.0, and both ends of the plugin handshake move with it

**Status:** `accepted`  
**Id:** `dec_01M05M4BG07J97TBZVBRSEWX2P`

The rename ships as **0.2.0**, not 0.1.6, and both halves of the plugin-daemon handshake move with it.

## Why the minor rather than the patch

0.x has no formal compatibility promise, so the number is a signal rather than a contract — and the signal is the whole reason to spend it here. Every interface a person or a model touches changed at once:

- the binaries are `specline` and `specline-daemon`
- all 27 environment variables
- all thirteen MCP tool names, and the server they are registered under
- the store's location and filename
- the mirror directory inside a repository
- the plugin, its marketplace entry, its two skills and the slash command

A patch bump says "nothing you rely on moved". Everything a person relies on moved. 0.1.5 → 0.1.6 would have been the version number quietly disagreeing with the release notes, and the release notes are the thing nobody reads twice.

## The handshake moved too, and that is the part with teeth

`min_daemon_version` in the plugin manifest and `MIN_PLUGIN_VERSION` in the daemon are the two ends of a compatibility check that exists because the plugin updates over git while the binaries update from a GitHub release. They will drift in somebody else's install.

Both were `0.1.0` and both are now `0.2.0`. The doc comment on `MIN_PLUGIN_VERSION` says to raise it only when an older plugin *genuinely cannot work* — "a removed tool, a changed response shape it reads" — and warns that raising it for a cosmetic change trains people to ignore the warning.

This is the clearest case that condition will ever get. From a 0.1.x plugin's point of view all thirteen tools were removed simultaneously. It also registers its MCP server under the old name and its hooks call a script that no longer exists. Both directions are broken, so both ends were raised together rather than leaving one to discover the other at runtime.

## What this deliberately does not claim

Not 1.0. Nothing about the rename makes the design more settled than it was yesterday, and a version number that implies stability the project has not earned is the same kind of lie as a patch bump that hides a breaking change.

