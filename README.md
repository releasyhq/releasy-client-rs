# Releasy Client (Rust)

Rust client for the Releasy release management API. It is a small,
blocking HTTP client built on `ureq` with typed request/response models.

## Quick start

```rust,no_run
use releasy_client::{Auth, Client, ReleaseListQuery, Result};

fn main() -> Result<()> {
    // Base URL should point at your Releasy instance (e.g. https://api.releasyhq.com)
    let client = Client::new("https://api.releasyhq.com", Auth::ApiKey("your-key".into()))?;

    let releases = client.list_releases(&ReleaseListQuery {
        product: Some("demo-app".into()),
        include_artifacts: Some(true),
        ..Default::default()
    })?;

    println!("found {} releases", releases.releases.len());
    Ok(())
}
```

### Authentication options

- `Auth::ApiKey`: supply the `x-releasy-api-key` header for end-user actions.
- `Auth::AdminKey`: supply the `x-releasy-admin-key` header for admin-only
  endpoints (creating customers, keys, releases, etc.).
- `Auth::OperatorJwt`: supply a bearer token when acting as an operator.
- `Auth::None`: for unauthenticated endpoints (mainly tests).

### Common operations

- Introspect an API key: `client.auth_introspect()?`
- Create a release: `client.create_release(&ReleaseCreateRequest { ... })?`
- Register and upload artifacts: use `register_release_artifact`, then
  `presign_release_artifact_upload`, then `upload_presigned_artifact`.
- Publish/unpublish a release: `publish_release` / `unpublish_release`.

### Error handling

All fallible methods return `releasy_client::Result<T>`. On non-success
status codes you receive `Error::Api { status, error, body }`, which may
carry the parsed `ErrorBody`. Transport issues (I/O, TLS, etc.) surface as
`Error::Transport`.

## Minimum supported Rust version

MSRV is Rust 1.85 (edition 2024). The crate is tested on stable.

## Development

Ensure the `just` command runner is installed. Helpful tasks:

- `just fmt` — format the workspace.
- `just pre-commit` — fmt + clippy (deny warnings) + tests.
- `cargo test` — run the integration tests.

## License

Licensed under the Apache License, Version 2.0. See `LICENSE` for details.
