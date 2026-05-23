## Summary

<!-- What changed and why. One paragraph max. Link issues with "Closes #". -->

Closes #

## Scope

<!-- Check ALL areas this PR touches. Reviewers and CI use this to gauge blast radius. -->

- [ ] `src/lib.rs` — Public SDK surface
- [ ] `src/` — Client / resources / types
- [ ] `examples/` — Example programs
- [ ] `Cargo.toml` / `Cargo.lock` — Dependencies, metadata
- [ ] `README.md` / docs
- [ ] `.github/` — CI/CD workflows
- [ ] Tests (unit + integration)

## Changes

<!-- Bullet points grouped by area. Be specific — diffs are for code, this is for intent. -->

-
-

## Verification

<!-- What you ran locally. Paste actual commands and their outcomes. -->

```bash
cargo check          # ✅ / ❌
cargo test           # ✅ / ❌
cargo clippy --all-targets -- -D warnings  # ✅ / ❌
cargo fmt --check    # ✅ / ❌
```

<details>
<summary>Additional verification (expand if applicable)</summary>

```bash
# Run an example end-to-end
SECAPI_API_KEY=... cargo run --example basic

# Build docs
cargo doc --no-deps

# Package check
cargo package --no-verify
cargo publish --dry-run
```

</details>

## Deployment Impact

<!-- Skip this section entirely for code-only changes with no release impact. -->

- [ ] New version bump in `Cargo.toml`
- [ ] Breaking API change (semver major)
- [ ] crates.io publish required
- [ ] Docs (README / examples / rustdoc) updated to match
- [ ] Companion docs PR in org docs site

## Completion Attestation

<!-- You MUST select one. This is a binding statement of delivery status. -->

- [ ] **100% complete, 100% functional.** All code is written, tested, clippy-clean, and works end-to-end against live SEC API. No outstanding work remains.
- [ ] **Not fully complete or functional.** Deltas listed below.

### Deltas (only if attesting incomplete)

<!-- Short bullets. Items intentionally deferred from this PR's stated scope. -->

-

## Screenshots / Demo

<!-- Terminal output, CLI snippets, or API response examples. Delete section if not applicable. -->

---

<details>
<summary>Agent Context</summary>

<!-- This section is for AI coding agents that may continue or review this work.
     Fill in what's relevant; delete what isn't. -->

**Key files to read first:**
<!-- List the 3-5 most important files for understanding this PR's changes. -->
- `src/lib.rs`
-

**Decisions made:**
<!-- Non-obvious choices and why. Agents should not re-litigate these. -->
-

**Relevant docs:**
- `README.md`
- https://docs.secapi.ai

**Conventions applied:**
<!-- Idiomatic Rust, error types, async runtime choice, serde patterns. -->
-

</details>
