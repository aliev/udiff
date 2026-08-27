# Releasing

Loopdiff uses cargo-dist, following the release setup used by
[aliev/baker](https://github.com/aliev/baker). A version tag builds archives and
installers, creates a GitHub Release, and updates
[aliev/homebrew-tap](https://github.com/aliev/homebrew-tap).

## One-time GitHub setup

Create a fine-grained personal access token that can write repository contents
in `aliev/homebrew-tap`. Add it to the Loopdiff repository as an Actions secret
named `HOMEBREW_TAP_TOKEN`.

The generated release workflow uses the repository-provided `GITHUB_TOKEN` for
the Loopdiff release itself. No additional secret is required for release
artifacts.

## Release

1. Update the version in `Cargo.toml` and run `cargo update -w` if needed.
2. Move the relevant entries in `CHANGELOG.md` from Unreleased to the new
   version and date.
3. Run the full verification suite:

   ```bash
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test --locked
   cargo dist plan
   ```

4. Merge the release changes to `main`.
5. Create and push the matching version tag:

   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

The Release workflow publishes the GitHub artifacts first and updates
`Formula/loopdiff.rb` in the tap only after the release succeeds. Prerelease
tags do not update the stable Homebrew formula.
