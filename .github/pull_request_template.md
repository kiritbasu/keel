<!--
Short is fine. The point of these three headings is that a reviewer can tell
what changed and how you know it works without reading the diff first.
-->

## What this changes

<!-- One or two sentences. The problem, not the patch. -->

## Why

<!-- What made it worth doing. Link an issue if there is one. -->

## How it was checked

<!-- The commands you ran, or the case you exercised. "Tests pass" is weaker
     than naming the test that would have failed before. -->

---

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` all pass, run through the pinned toolchain
- [ ] A test covers this, including a case that fails without the change
- [ ] No generated file under `product/` or `.keel/` was hand-edited

<!--
Found a security problem? Do not open a pull request for it — see SECURITY.md
for the private route.
-->
