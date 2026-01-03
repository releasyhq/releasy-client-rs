use std::fs::File;
use std::path::Path;
use std::time::Duration;

use serde::de::DeserializeOwned;
use ureq::{Agent, RequestBuilder};

use crate::error::{Error, Result};
use crate::models::*;

/// Authentication strategy for API requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Auth {
    None,
    AdminKey(String),
    ApiKey(String),
    OperatorJwt(String),
}

/// Blocking HTTP client for the Releasy API.
#[derive(Clone, Debug)]
pub struct Client {
    base_url: String,
    auth: Auth,
    user_agent: Option<String>,
    agent: Agent,
}

/// Builder for configuring a `Client`.
#[derive(Clone, Debug)]
pub struct ClientBuilder {
    base_url: String,
    auth: Auth,
    user_agent: Option<String>,
    timeout_global: Option<Duration>,
    agent: Option<Agent>,
}

/// Resolved download redirect location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadResolution {
    pub location: String,
}

impl Client {
    /// Start building a client with the given base URL and auth.
    pub fn builder(base_url: impl Into<String>, auth: Auth) -> Result<ClientBuilder> {
        ClientBuilder::new(base_url, auth)
    }

    /// Build a client with default configuration.
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Result<Self> {
        ClientBuilder::new(base_url, auth)?.build()
    }

    /// Return a cloned client with updated authentication.
    pub fn with_auth(&self, auth: Auth) -> Self {
        let mut updated = self.clone();
        updated.auth = auth;
        updated
    }

    /// Fetch the OpenAPI document from the server.
    pub fn openapi_json(&self) -> Result<serde_json::Value> {
        let url = self.url("/openapi.json");
        let request = self.apply_headers(self.agent.get(&url));
        let response = request.call()?;
        self.parse_json_response(response)
    }

    /// Check service health (API + database).
    pub fn health_check(&self) -> Result<HealthResponse> {
        let url = self.url("/health");
        let request = self.apply_headers(self.agent.get(&url));
        let response = request.call()?;
        self.parse_json_response(response)
    }

    /// Check service liveness.
    pub fn live_check(&self) -> Result<HealthResponse> {
        let url = self.url("/live");
        let request = self.apply_headers(self.agent.get(&url));
        let response = request.call()?;
        self.parse_json_response(response)
    }

    /// Check service readiness.
    pub fn ready_check(&self) -> Result<HealthResponse> {
        let url = self.url("/ready");
        let request = self.apply_headers(self.agent.get(&url));
        let response = request.call()?;
        self.parse_json_response(response)
    }

    /// List audit events with optional filters.
    pub fn list_audit_events(&self, query: &AuditEventListQuery) -> Result<AuditEventListResponse> {
        let url = self.url("/v1/admin/audit-events");
        let mut request = self.apply_headers(self.agent.get(&url));
        if let Some(value) = &query.customer_id {
            request = request.query("customer_id", value);
        }
        if let Some(value) = &query.actor {
            request = request.query("actor", value);
        }
        if let Some(value) = &query.event {
            request = request.query("event", value);
        }
        if let Some(value) = query.created_from {
            let value = value.to_string();
            request = request.query("created_from", &value);
        }
        if let Some(value) = query.created_to {
            let value = value.to_string();
            request = request.query("created_to", &value);
        }
        if let Some(value) = query.limit {
            let value = value.to_string();
            request = request.query("limit", &value);
        }
        if let Some(value) = query.offset {
            let value = value.to_string();
            request = request.query("offset", &value);
        }
        let response = request.call()?;
        self.parse_json_response(response)
    }

    /// List customers with optional filters.
    pub fn list_customers(
        &self,
        query: &AdminCustomerListQuery,
    ) -> Result<AdminCustomerListResponse> {
        let url = self.url("/v1/admin/customers");
        let mut request = self.apply_headers(self.agent.get(&url));
        if let Some(value) = &query.customer_id {
            request = request.query("customer_id", value);
        }
        if let Some(value) = &query.name {
            request = request.query("name", value);
        }
        if let Some(value) = &query.plan {
            request = request.query("plan", value);
        }
        if let Some(value) = query.limit {
            let value = value.to_string();
            request = request.query("limit", &value);
        }
        if let Some(value) = query.offset {
            let value = value.to_string();
            request = request.query("offset", &value);
        }
        let response = request.call()?;
        self.parse_json_response(response)
    }

    /// Create a customer (admin only).
    pub fn admin_create_customer(
        &self,
        body: &AdminCreateCustomerRequest,
    ) -> Result<AdminCreateCustomerResponse> {
        self.admin_create_customer_with_idempotency(body, None)
    }

    /// Create a customer with an optional idempotency key.
    pub fn admin_create_customer_with_idempotency(
        &self,
        body: &AdminCreateCustomerRequest,
        idempotency_key: Option<&str>,
    ) -> Result<AdminCreateCustomerResponse> {
        let url = self.url("/v1/admin/customers");
        let mut request = self.apply_headers(self.agent.post(&url));
        if let Some(key) = idempotency_key {
            request = request.header("Idempotency-Key", key);
        }
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    /// Fetch a customer by id.
    pub fn get_customer(&self, customer_id: &str) -> Result<AdminCustomerResponse> {
        let url = self.url(&format!("/v1/admin/customers/{}", customer_id));
        let request = self.apply_headers(self.agent.get(&url));
        let response = request.call()?;
        self.parse_json_response(response)
    }

    /// Update customer fields.
    pub fn update_customer(
        &self,
        customer_id: &str,
        body: &AdminUpdateCustomerRequest,
    ) -> Result<AdminCustomerResponse> {
        let url = self.url(&format!("/v1/admin/customers/{}", customer_id));
        let request = self.apply_headers(self.agent.patch(&url));
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    /// List users with optional filters.
    pub fn list_users(&self, query: &UserListQuery) -> Result<UserListResponse> {
        let url = self.url("/v1/admin/users");
        let mut request = self.apply_headers(self.agent.get(&url));
        if let Some(value) = &query.customer_id {
            request = request.query("customer_id", value);
        }
        if let Some(value) = &query.email {
            request = request.query("email", value);
        }
        if let Some(value) = &query.status {
            request = request.query("status", value);
        }
        if let Some(value) = &query.keycloak_user_id {
            request = request.query("keycloak_user_id", value);
        }
        if let Some(value) = query.created_from {
            let value = value.to_string();
            request = request.query("created_from", &value);
        }
        if let Some(value) = query.created_to {
            let value = value.to_string();
            request = request.query("created_to", &value);
        }
        if let Some(value) = query.limit {
            let value = value.to_string();
            request = request.query("limit", &value);
        }
        if let Some(value) = &query.cursor {
            request = request.query("cursor", value);
        }
        let response = request.call()?;
        self.parse_json_response(response)
    }

    /// Create a user (admin only).
    pub fn create_user(&self, body: &UserCreateRequest) -> Result<UserResponse> {
        self.create_user_with_idempotency(body, None)
    }

    /// Create a user with an optional idempotency key.
    pub fn create_user_with_idempotency(
        &self,
        body: &UserCreateRequest,
        idempotency_key: Option<&str>,
    ) -> Result<UserResponse> {
        let url = self.url("/v1/admin/users");
        let mut request = self.apply_headers(self.agent.post(&url));
        if let Some(key) = idempotency_key {
            request = request.header("Idempotency-Key", key);
        }
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    /// Fetch a user by id.
    pub fn get_user(&self, user_id: &str) -> Result<UserResponse> {
        let url = self.url(&format!("/v1/admin/users/{}", user_id));
        let request = self.apply_headers(self.agent.get(&url));
        let response = request.call()?;
        self.parse_json_response(response)
    }

    /// Patch a user by id.
    pub fn patch_user(&self, user_id: &str, body: &UserPatchRequest) -> Result<UserResponse> {
        let url = self.url(&format!("/v1/admin/users/{}", user_id));
        let request = self.apply_headers(self.agent.patch(&url));
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    /// Replace the user's groups.
    pub fn replace_groups(
        &self,
        user_id: &str,
        body: &UserGroupsReplaceRequest,
    ) -> Result<UserResponse> {
        let url = self.url(&format!("/v1/admin/users/{}/groups", user_id));
        let request = self.apply_headers(self.agent.put(&url));
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    /// Trigger a credential reset email for the user.
    pub fn reset_credentials(&self, user_id: &str, body: &ResetCredentialsRequest) -> Result<()> {
        let url = self.url(&format!("/v1/admin/users/{}/reset-credentials", user_id));
        let request = self.apply_headers(self.agent.post(&url));
        let response = request.send_json(body)?;
        self.parse_empty_response(response, 202)
    }

    pub fn list_entitlements(
        &self,
        customer_id: &str,
        query: &EntitlementListQuery,
    ) -> Result<EntitlementListResponse> {
        let url = self.url(&format!("/v1/admin/customers/{}/entitlements", customer_id));
        let mut request = self.apply_headers(self.agent.get(&url));
        if let Some(value) = &query.product {
            request = request.query("product", value);
        }
        if let Some(value) = query.limit {
            let value = value.to_string();
            request = request.query("limit", &value);
        }
        if let Some(value) = query.offset {
            let value = value.to_string();
            request = request.query("offset", &value);
        }
        let response = request.call()?;
        self.parse_json_response(response)
    }

    pub fn create_entitlement(
        &self,
        customer_id: &str,
        body: &EntitlementCreateRequest,
    ) -> Result<EntitlementResponse> {
        let url = self.url(&format!("/v1/admin/customers/{}/entitlements", customer_id));
        let request = self.apply_headers(self.agent.post(&url));
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    pub fn update_entitlement(
        &self,
        customer_id: &str,
        entitlement_id: &str,
        body: &EntitlementUpdateRequest,
    ) -> Result<EntitlementResponse> {
        let url = self.url(&format!(
            "/v1/admin/customers/{}/entitlements/{}",
            customer_id, entitlement_id
        ));
        let request = self.apply_headers(self.agent.patch(&url));
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    pub fn delete_entitlement(&self, customer_id: &str, entitlement_id: &str) -> Result<()> {
        let url = self.url(&format!(
            "/v1/admin/customers/{}/entitlements/{}",
            customer_id, entitlement_id
        ));
        let request = self.apply_headers(self.agent.delete(&url));
        let response = request.call()?;
        self.parse_empty_response(response, 204)
    }

    pub fn admin_create_key(&self, body: &AdminCreateKeyRequest) -> Result<AdminCreateKeyResponse> {
        let url = self.url("/v1/admin/keys");
        let request = self.apply_headers(self.agent.post(&url));
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    pub fn admin_revoke_key(&self, body: &AdminRevokeKeyRequest) -> Result<AdminRevokeKeyResponse> {
        let url = self.url("/v1/admin/keys/revoke");
        let request = self.apply_headers(self.agent.post(&url));
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    pub fn auth_introspect(&self) -> Result<ApiKeyIntrospection> {
        let url = self.url("/v1/auth/introspect");
        let request = self.apply_headers(self.agent.post(&url));
        let response = request.send("")?;
        self.parse_json_response(response)
    }

    pub fn create_download_token(
        &self,
        body: &DownloadTokenRequest,
    ) -> Result<DownloadTokenResponse> {
        let url = self.url("/v1/downloads/token");
        let request = self.apply_headers(self.agent.post(&url));
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    pub fn resolve_download_token(&self, token: &str) -> Result<DownloadResolution> {
        let url = self.url(&format!("/v1/downloads/{}", token));
        let request = self.apply_headers(self.agent.get(&url));
        let response = request.call()?;
        let status = response.status().as_u16();
        if status == 302 {
            let location = response
                .headers()
                .get(ureq::http::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .map(|value| value.to_string())
                .ok_or(Error::MissingLocationHeader)?;
            return Ok(DownloadResolution { location });
        }
        Err(self.error_from_response(response, status))
    }

    /// List releases with optional filters.
    pub fn list_releases(&self, query: &ReleaseListQuery) -> Result<ReleaseListResponse> {
        let url = self.url("/v1/releases");
        let mut request = self.apply_headers(self.agent.get(&url));
        if let Some(value) = &query.product {
            request = request.query("product", value);
        }
        if let Some(value) = &query.version {
            request = request.query("version", value);
        }
        if let Some(value) = &query.status {
            request = request.query("status", value);
        }
        if let Some(value) = query.include_artifacts {
            request = request.query("include_artifacts", if value { "true" } else { "false" });
        }
        if let Some(value) = query.limit {
            let value = value.to_string();
            request = request.query("limit", &value);
        }
        if let Some(value) = query.offset {
            let value = value.to_string();
            request = request.query("offset", &value);
        }
        let response = request.call()?;
        self.parse_json_response(response)
    }

    /// Create a new release.
    pub fn create_release(&self, body: &ReleaseCreateRequest) -> Result<ReleaseResponse> {
        let url = self.url("/v1/releases");
        let request = self.apply_headers(self.agent.post(&url));
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    pub fn delete_release(&self, release_id: &str) -> Result<()> {
        let url = self.url(&format!("/v1/releases/{}", release_id));
        let request = self.apply_headers(self.agent.delete(&url));
        let response = request.call()?;
        self.parse_empty_response(response, 204)
    }

    /// Register a release artifact.
    pub fn register_release_artifact(
        &self,
        release_id: &str,
        body: &ArtifactRegisterRequest,
    ) -> Result<ArtifactRegisterResponse> {
        let url = self.url(&format!("/v1/releases/{}/artifacts", release_id));
        let request = self.apply_headers(self.agent.post(&url));
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    /// Request a presigned upload URL for an artifact.
    pub fn presign_release_artifact_upload(
        &self,
        release_id: &str,
        body: &ArtifactPresignRequest,
    ) -> Result<ArtifactPresignResponse> {
        let url = self.url(&format!("/v1/releases/{}/artifacts/presign", release_id));
        let request = self.apply_headers(self.agent.post(&url));
        let response = request.send_json(body)?;
        self.parse_json_response(response)
    }

    /// Upload artifact bytes to a presigned URL.
    pub fn upload_presigned_artifact(
        &self,
        upload_url: &str,
        file_path: impl AsRef<Path>,
    ) -> Result<()> {
        let file = File::open(file_path.as_ref())
            .map_err(|err| Error::Transport(ureq::Error::from(err)))?;
        let response = self.agent.put(upload_url).send(file)?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(());
        }
        Err(self.error_from_response(response, status))
    }

    /// Publish a release.
    pub fn publish_release(&self, release_id: &str) -> Result<ReleaseResponse> {
        let url = self.url(&format!("/v1/releases/{}/publish", release_id));
        let request = self.apply_headers(self.agent.post(&url));
        let response = request.send("")?;
        self.parse_json_response(response)
    }

    /// Unpublish a release.
    pub fn unpublish_release(&self, release_id: &str) -> Result<ReleaseResponse> {
        let url = self.url(&format!("/v1/releases/{}/unpublish", release_id));
        let request = self.apply_headers(self.agent.post(&url));
        let response = request.send("")?;
        self.parse_json_response(response)
    }

    fn url(&self, path: &str) -> String {
        let trimmed = path.trim_start_matches('/');
        format!("{}/{}", self.base_url, trimmed)
    }

    fn apply_headers<B>(&self, request: RequestBuilder<B>) -> RequestBuilder<B> {
        let mut request = request.header("Accept", "application/json");
        if let Some(user_agent) = &self.user_agent {
            request = request.header("User-Agent", user_agent);
        }
        self.apply_auth(request)
    }

    fn apply_auth<B>(&self, request: RequestBuilder<B>) -> RequestBuilder<B> {
        match &self.auth {
            Auth::None => request,
            Auth::AdminKey(key) => request.header("x-releasy-admin-key", key),
            Auth::ApiKey(key) => request.header("x-releasy-api-key", key),
            Auth::OperatorJwt(token) => {
                let value = format!("Bearer {}", token);
                request.header("Authorization", &value)
            }
        }
    }

    fn parse_json_response<T: DeserializeOwned>(
        &self,
        response: ureq::http::Response<ureq::Body>,
    ) -> Result<T> {
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            let mut response = response;
            let parsed = response.body_mut().read_json::<T>()?;
            return Ok(parsed);
        }
        Err(self.error_from_response(response, status))
    }

    fn parse_empty_response(
        &self,
        response: ureq::http::Response<ureq::Body>,
        expected_status: u16,
    ) -> Result<()> {
        let status = response.status().as_u16();
        if status == expected_status {
            return Ok(());
        }
        Err(self.error_from_response(response, status))
    }

    fn error_from_response(
        &self,
        mut response: ureq::http::Response<ureq::Body>,
        status: u16,
    ) -> Error {
        let body = match response.body_mut().read_to_string() {
            Ok(body) => body,
            Err(err) => return Error::Transport(err),
        };
        let parsed = serde_json::from_str::<ErrorBody>(&body).ok();
        Error::Api {
            status,
            error: parsed,
            body: if body.is_empty() { None } else { Some(body) },
        }
    }
}

impl ClientBuilder {
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Result<Self> {
        let base_url = normalize_base_url(base_url.into())?;
        Ok(Self {
            base_url,
            auth,
            user_agent: None,
            timeout_global: None,
            agent: None,
        })
    }

    pub fn user_agent(mut self, value: impl Into<String>) -> Self {
        self.user_agent = Some(value.into());
        self
    }

    pub fn timeout_global(mut self, timeout: Duration) -> Self {
        self.timeout_global = Some(timeout);
        self
    }

    pub fn agent(mut self, agent: Agent) -> Self {
        self.agent = Some(agent);
        self
    }

    pub fn build(self) -> Result<Client> {
        let agent = match self.agent {
            Some(agent) => agent,
            None => {
                let mut builder = Agent::config_builder().http_status_as_error(false);
                if let Some(timeout) = self.timeout_global {
                    builder = builder.timeout_global(Some(timeout));
                }
                let config = builder.build();
                config.into()
            }
        };
        Ok(Client {
            base_url: self.base_url,
            auth: self.auth,
            user_agent: self.user_agent,
            agent,
        })
    }
}

fn normalize_base_url(base_url: String) -> Result<String> {
    let trimmed = base_url.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err(Error::InvalidBaseUrl(base_url));
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(Error::InvalidBaseUrl(base_url));
    }
    Ok(trimmed)
}
