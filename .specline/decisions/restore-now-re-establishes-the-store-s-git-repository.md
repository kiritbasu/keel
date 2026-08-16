<!-- specline:generated decision dec_01KZN3K1A6PBRFVJ9H9H6542HM v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-31 — restore now re-establishes the store's git repository

**Status:** `accepted`  
**Decided:** 2026-08-10  
**Id:** `dec_01KZN3K1A6PBRFVJ9H9H6542HM`

Found one command before deleting the only copy that still had it. SPEC §11 names three recovery tiers: the store's own git history (full fidelity, every revision), the Parquet backup, and the markdown mirror. keel restore rebuilds from tier 2 into a fresh directory - and handed back a store with no .git, so a restore silently cost you tier 1, the tier with the most fidelity.

That is worse than it sounds because of when it fires: you only restore after something has already gone wrong, so the moment you use tier 2 is exactly the moment you lose tier 1. Nothing warned, and verify_restore passed because it checks rows, not recovery properties.

The fix lives in keel-cli rather than keel-core, mirroring plugin/install.sh: keel-core does not spawn processes, and 'a store should be a git repo' is policy rather than storage. After a verified restore the CLI runs git init, writes the models/ .gitignore, and commits the restored state - an empty repository restores nothing, so the state has to be in a commit. No remote, which is Q-2 and KB's call.

It never fails the restore. A missing git binary prints a warning naming the exact command to run, because the rows being back matters more than the tier being re-established this second.

Two tests: a restored store becomes a repo with its state committed and models/ ignored, and an existing repo is left alone - the same loss in the other direction.

