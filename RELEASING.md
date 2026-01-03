# Releasing releasy-client

## Prerequisites

- `cargo login` has been run with a crates.io token.
- The working tree is clean.
- The release version is decided (SemVer).

## Steps

1. Update `Cargo.toml` version.
2. Run formatting + tests:
   - `just fmt`
   - `just pre-commit`
3. Confirm package contents:
   - `cargo package --list`
   - Note: `openapi.json` is currently excluded via `Cargo.toml`.
4. Dry run publish:
   - `cargo publish --dry-run`
5. Tag and push:
   - `git tag vX.Y.Z`
   - `git push --tags`
6. Publish:
   - `cargo publish`
7. Verify:
   - Check crates.io and docs.rs for the new version.
