use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use releasy_client::{
    AdminCustomerListQuery, AdminUpdateCustomerRequest, Auth, Client, Error, ReleaseCreateRequest,
    ReleaseListQuery, ResetCredentialsRequest, UserCreateRequest, UserGroupsReplaceRequest,
    UserListQuery, UserPatchRequest,
};

struct RawRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

struct ResponseSpec {
    status_line: String,
    headers: Vec<(String, String)>,
    body: String,
}

fn spawn_server<F>(handler: F) -> (String, thread::JoinHandle<()>)
where
    F: FnOnce(RawRequest) -> ResponseSpec + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("timeout");
        let request = read_request(&mut stream);
        let response = handler(request);
        write_response(&mut stream, response);
    });
    (format!("http://{}", addr), handle)
}

fn read_request(stream: &mut TcpStream) -> RawRequest {
    let mut buffer = Vec::new();
    let mut temp = [0u8; 1024];
    let header_end = loop {
        let read = stream.read(&mut temp).expect("read");
        if read == 0 {
            break None;
        }
        buffer.extend_from_slice(&temp[..read]);
        if let Some(pos) = find_subsequence(&buffer, b"\r\n\r\n") {
            break Some(pos + 4);
        }
    }
    .expect("headers");

    let headers_bytes = &buffer[..header_end];
    let mut body = buffer[header_end..].to_vec();

    let headers_text = String::from_utf8_lossy(headers_bytes);
    let mut lines = headers_text.split("\r\n");
    let request_line = lines.next().expect("request line");
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or("").to_string();
    let path = request_parts.next().unwrap_or("").to_string();

    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);

    while body.len() < content_length {
        let read = stream.read(&mut temp).expect("read body");
        if read == 0 {
            break;
        }
        body.extend_from_slice(&temp[..read]);
    }
    body.truncate(content_length);

    RawRequest {
        method,
        path,
        headers,
        body,
    }
}

fn write_response(stream: &mut TcpStream, response: ResponseSpec) {
    let mut headers = response.headers;
    headers.push((
        "Content-Length".to_string(),
        response.body.len().to_string(),
    ));
    headers.push(("Connection".to_string(), "close".to_string()));

    let mut response_text = String::new();
    response_text.push_str(&response.status_line);
    response_text.push_str("\r\n");
    for (name, value) in headers {
        response_text.push_str(&name);
        response_text.push_str(": ");
        response_text.push_str(&value);
        response_text.push_str("\r\n");
    }
    response_text.push_str("\r\n");
    response_text.push_str(&response.body);

    stream
        .write_all(response_text.as_bytes())
        .expect("write response");
    stream.flush().expect("flush");
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn parse_query(path: &str) -> (String, HashMap<String, String>) {
    let mut parts = path.splitn(2, '?');
    let base = parts.next().unwrap_or("").to_string();
    let query = parts.next().unwrap_or("");
    let mut params = HashMap::new();
    if !query.is_empty() {
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                params.insert(key.to_string(), value.to_string());
            }
        }
    }
    (base, params)
}

fn write_temp_file(contents: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    path.push(format!("releasy-client-upload-{suffix}.bin"));
    std::fs::write(&path, contents).expect("write temp file");
    path
}

#[test]
fn health_check_happy_path() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/health");

        let body = r#"{"status":"ok"}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 200 OK".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::None).unwrap();
    let response = client.health_check().unwrap();
    assert_eq!(response.status, "ok");

    handle.join().expect("server join");
}

#[test]
fn live_check_happy_path() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/live");

        let body = r#"{"status":"live"}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 200 OK".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::None).unwrap();
    let response = client.live_check().unwrap();
    assert_eq!(response.status, "live");

    handle.join().expect("server join");
}

#[test]
fn ready_check_service_unavailable_returns_error() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/ready");

        let body = r#"{"error":{"code":"unavailable","message":"maintenance"}}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 503 Service Unavailable".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::None).unwrap();
    let error = client.ready_check().expect_err("expected error");
    match error {
        Error::Api { status, error, .. } => {
            assert_eq!(status, 503);
            let detail = error.expect("error body");
            assert_eq!(detail.error.code, "unavailable");
            assert_eq!(detail.error.message, "maintenance");
        }
        other => panic!("unexpected error: {other:?}"),
    }

    handle.join().expect("server join");
}

#[test]
fn list_customers_happy_path() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "GET");
        let (path, params) = parse_query(&request.path);
        assert_eq!(path, "/v1/admin/customers");
        assert_eq!(params.get("customer_id"), Some(&"cust-1".to_string()));
        assert_eq!(params.get("name"), Some(&"Acme".to_string()));
        assert_eq!(params.get("plan"), Some(&"pro".to_string()));
        assert_eq!(params.get("limit"), Some(&"100".to_string()));
        assert_eq!(params.get("offset"), Some(&"10".to_string()));
        assert_eq!(
            request.headers.get("x-releasy-admin-key"),
            Some(&"admin-key".to_string())
        );

        let body = r#"{"customers":[{"id":"cust-1","name":"Acme","created_at":1700000000,"plan":"pro","suspended_at":null}],"limit":100,"offset":10}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 200 OK".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::AdminKey("admin-key".to_string())).unwrap();
    let query = AdminCustomerListQuery {
        customer_id: Some("cust-1".to_string()),
        name: Some("Acme".to_string()),
        plan: Some("pro".to_string()),
        limit: Some(100),
        offset: Some(10),
    };

    let response = client.list_customers(&query).unwrap();
    assert_eq!(response.customers.len(), 1);
    assert_eq!(response.customers[0].id, "cust-1");
    assert_eq!(response.customers[0].plan.as_deref(), Some("pro"));
    assert_eq!(response.limit, 100);
    assert_eq!(response.offset, 10);

    handle.join().expect("server join");
}

#[test]
fn get_customer_not_found_returns_error() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/v1/admin/customers/cust-missing");
        assert_eq!(
            request.headers.get("x-releasy-admin-key"),
            Some(&"admin-key".to_string())
        );

        let body = r#"{"error":{"code":"not_found","message":"customer missing"}}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 404 Not Found".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::AdminKey("admin-key".to_string())).unwrap();
    let error = client
        .get_customer("cust-missing")
        .expect_err("expected error");
    match error {
        Error::Api { status, error, .. } => {
            assert_eq!(status, 404);
            let detail = error.expect("error body");
            assert_eq!(detail.error.code, "not_found");
            assert_eq!(detail.error.message, "customer missing");
        }
        other => panic!("unexpected error: {other:?}"),
    }

    handle.join().expect("server join");
}

#[test]
fn update_customer_happy_path() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "PATCH");
        assert_eq!(request.path, "/v1/admin/customers/cust-1");
        assert_eq!(
            request.headers.get("x-releasy-admin-key"),
            Some(&"admin-key".to_string())
        );
        let body_json: serde_json::Value =
            serde_json::from_slice(&request.body).expect("json body");
        assert_eq!(body_json["plan"], "enterprise");
        assert_eq!(body_json.as_object().unwrap().len(), 1);

        let body = r#"{"id":"cust-1","name":"Acme","created_at":1700000000,"plan":"enterprise","suspended_at":null}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 200 OK".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::AdminKey("admin-key".to_string())).unwrap();
    let request = AdminUpdateCustomerRequest {
        name: None,
        plan: Some("enterprise".to_string()),
        suspended: None,
    };

    let response = client.update_customer("cust-1", &request).unwrap();
    assert_eq!(response.plan.as_deref(), Some("enterprise"));
    assert_eq!(response.id, "cust-1");

    handle.join().expect("server join");
}

#[test]
fn list_users_happy_path() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "GET");
        let (path, params) = parse_query(&request.path);
        assert_eq!(path, "/v1/admin/users");
        assert_eq!(params.get("customer_id"), Some(&"cust-1".to_string()));
        assert_eq!(params.get("email"), Some(&"alice".to_string()));
        assert_eq!(params.get("status"), Some(&"active".to_string()));
        assert_eq!(params.get("keycloak_user_id"), Some(&"kc-1".to_string()));
        assert_eq!(params.get("created_from"), Some(&"1700000000".to_string()));
        assert_eq!(params.get("created_to"), Some(&"1700001000".to_string()));
        assert_eq!(params.get("limit"), Some(&"25".to_string()));
        assert_eq!(params.get("cursor"), Some(&"cursor-1".to_string()));
        assert_eq!(
            request.headers.get("x-releasy-admin-key"),
            Some(&"admin-key".to_string())
        );

        let body = r#"{"users":[{"id":"user-1","keycloak_user_id":"kc-1","customer_id":"cust-1","email":"alice","status":"active","groups":["platform_admin"],"created_at":1700000000,"updated_at":1700001000}],"next_cursor":"cursor-2"}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 200 OK".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::AdminKey("admin-key".to_string())).unwrap();
    let query = UserListQuery {
        customer_id: Some("cust-1".to_string()),
        email: Some("alice".to_string()),
        status: Some("active".to_string()),
        keycloak_user_id: Some("kc-1".to_string()),
        created_from: Some(1_700_000_000),
        created_to: Some(1_700_001_000),
        limit: Some(25),
        cursor: Some("cursor-1".to_string()),
    };

    let response = client.list_users(&query).unwrap();
    assert_eq!(response.users.len(), 1);
    assert_eq!(response.users[0].id, "user-1");
    assert_eq!(response.next_cursor.as_deref(), Some("cursor-2"));

    handle.join().expect("server join");
}

#[test]
fn create_user_happy_path() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/v1/admin/users");
        assert_eq!(
            request.headers.get("x-releasy-admin-key"),
            Some(&"admin-key".to_string())
        );
        let body_json: serde_json::Value =
            serde_json::from_slice(&request.body).expect("json body");
        assert_eq!(body_json["email"], "alice");
        assert_eq!(body_json["customer_id"], "cust-1");
        assert_eq!(body_json["status"], "active");
        assert_eq!(
            body_json["groups"].as_array().unwrap()[0].as_str().unwrap(),
            "platform_admin"
        );

        let body = r#"{"id":"user-1","keycloak_user_id":"kc-1","customer_id":"cust-1","email":"alice","status":"active","groups":["platform_admin"],"created_at":1700000000,"updated_at":1700001000}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 201 Created".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::AdminKey("admin-key".to_string())).unwrap();
    let request = UserCreateRequest {
        email: "alice".to_string(),
        customer_id: "cust-1".to_string(),
        display_name: None,
        groups: Some(vec!["platform_admin".to_string()]),
        metadata: None,
        status: Some("active".to_string()),
    };

    let response = client.create_user(&request).unwrap();
    assert_eq!(response.id, "user-1");
    assert_eq!(response.email, "alice");

    handle.join().expect("server join");
}

#[test]
fn get_user_not_found_returns_error() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/v1/admin/users/user-missing");
        assert_eq!(
            request.headers.get("x-releasy-admin-key"),
            Some(&"admin-key".to_string())
        );

        let body = r#"{"error":{"code":"not_found","message":"user missing"}}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 404 Not Found".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::AdminKey("admin-key".to_string())).unwrap();
    let error = client.get_user("user-missing").expect_err("expected error");
    match error {
        Error::Api { status, error, .. } => {
            assert_eq!(status, 404);
            let detail = error.expect("error body");
            assert_eq!(detail.error.code, "not_found");
            assert_eq!(detail.error.message, "user missing");
        }
        other => panic!("unexpected error: {other:?}"),
    }

    handle.join().expect("server join");
}

#[test]
fn patch_user_happy_path() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "PATCH");
        assert_eq!(request.path, "/v1/admin/users/user-1");
        assert_eq!(
            request.headers.get("x-releasy-admin-key"),
            Some(&"admin-key".to_string())
        );
        let body_json: serde_json::Value =
            serde_json::from_slice(&request.body).expect("json body");
        assert_eq!(body_json["display_name"], "Alice");
        assert_eq!(body_json.as_object().unwrap().len(), 1);

        let body = r#"{"id":"user-1","keycloak_user_id":"kc-1","customer_id":"cust-1","email":"alice","status":"active","groups":["platform_admin"],"created_at":1700000000,"updated_at":1700002000,"display_name":"Alice"}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 200 OK".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::AdminKey("admin-key".to_string())).unwrap();
    let request = UserPatchRequest {
        display_name: Some("Alice".to_string()),
        groups: None,
        metadata: None,
        status: None,
    };

    let response = client.patch_user("user-1", &request).unwrap();
    assert_eq!(response.display_name.as_deref(), Some("Alice"));

    handle.join().expect("server join");
}

#[test]
fn replace_groups_happy_path() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "PUT");
        assert_eq!(request.path, "/v1/admin/users/user-1/groups");
        assert_eq!(
            request.headers.get("x-releasy-admin-key"),
            Some(&"admin-key".to_string())
        );
        let body_json: serde_json::Value =
            serde_json::from_slice(&request.body).expect("json body");
        assert_eq!(
            body_json["groups"].as_array().unwrap()[0].as_str().unwrap(),
            "platform_support"
        );

        let body = r#"{"id":"user-1","keycloak_user_id":"kc-1","customer_id":"cust-1","email":"alice","status":"active","groups":["platform_support"],"created_at":1700000000,"updated_at":1700002000}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 200 OK".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::AdminKey("admin-key".to_string())).unwrap();
    let request = UserGroupsReplaceRequest {
        groups: vec!["platform_support".to_string()],
    };

    let response = client.replace_groups("user-1", &request).unwrap();
    assert_eq!(response.groups, vec!["platform_support".to_string()]);

    handle.join().expect("server join");
}

#[test]
fn reset_credentials_accepts_202() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/v1/admin/users/user-1/reset-credentials");
        assert_eq!(
            request.headers.get("x-releasy-admin-key"),
            Some(&"admin-key".to_string())
        );
        let body_json: serde_json::Value =
            serde_json::from_slice(&request.body).expect("json body");
        assert_eq!(body_json["send_email"], true);

        ResponseSpec {
            status_line: "HTTP/1.1 202 Accepted".to_string(),
            headers: vec![],
            body: "".to_string(),
        }
    });

    let client = Client::new(base_url, Auth::AdminKey("admin-key".to_string())).unwrap();
    let request = ResetCredentialsRequest {
        send_email: Some(true),
    };

    client.reset_credentials("user-1", &request).expect("reset");

    handle.join().expect("server join");
}

#[test]
fn list_releases_happy_path() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "GET");
        let (path, params) = parse_query(&request.path);
        assert_eq!(path, "/v1/releases");
        assert_eq!(params.get("product"), Some(&"demo".to_string()));
        assert_eq!(params.get("limit"), Some(&"10".to_string()));
        assert_eq!(params.get("offset"), Some(&"0".to_string()));
        assert_eq!(params.get("include_artifacts"), Some(&"true".to_string()));
        assert_eq!(
            request.headers.get("x-releasy-api-key"),
            Some(&"test-key".to_string())
        );

        let body = r#"{"releases":[],"limit":10,"offset":0}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 200 OK".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::ApiKey("test-key".to_string())).unwrap();
    let query = ReleaseListQuery {
        product: Some("demo".to_string()),
        limit: Some(10),
        offset: Some(0),
        include_artifacts: Some(true),
        ..Default::default()
    };

    let response = client.list_releases(&query).unwrap();
    assert_eq!(response.releases.len(), 0);
    assert_eq!(response.limit, 10);
    assert_eq!(response.offset, 0);

    handle.join().expect("server join");
}

#[test]
fn create_release_returns_api_error() {
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/v1/releases");
        assert_eq!(
            request.headers.get("x-releasy-admin-key"),
            Some(&"admin-key".to_string())
        );
        let body_json: serde_json::Value =
            serde_json::from_slice(&request.body).expect("json body");
        assert_eq!(body_json["product"], "demo");
        assert_eq!(body_json["version"], "1.0.0");

        let body = r#"{"error":{"code":"release_conflict","message":"already exists"}}"#;
        ResponseSpec {
            status_line: "HTTP/1.1 409 Conflict".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    });

    let client = Client::new(base_url, Auth::AdminKey("admin-key".to_string())).unwrap();
    let request = ReleaseCreateRequest {
        product: "demo".to_string(),
        version: "1.0.0".to_string(),
    };

    let error = client.create_release(&request).expect_err("expected error");
    match error {
        Error::Api { status, error, .. } => {
            assert_eq!(status, 409);
            let detail = error.expect("error body");
            assert_eq!(detail.error.code, "release_conflict");
            assert_eq!(detail.error.message, "already exists");
        }
        other => panic!("unexpected error: {other:?}"),
    }

    handle.join().expect("server join");
}

#[test]
fn upload_presigned_artifact_puts_file_body() {
    let payload = b"releasy-upload-bytes";
    let path = write_temp_file(payload);
    let (base_url, handle) = spawn_server(move |request| {
        assert_eq!(request.method, "PUT");
        assert_eq!(request.path, "/upload");
        assert_eq!(
            request.headers.get("content-length"),
            Some(&payload.len().to_string())
        );
        assert_eq!(request.body, payload);

        ResponseSpec {
            status_line: "HTTP/1.1 200 OK".to_string(),
            headers: vec![],
            body: "".to_string(),
        }
    });

    let client = Client::new(base_url.clone(), Auth::None).unwrap();
    let upload_url = format!("{}/upload", base_url);
    client
        .upload_presigned_artifact(&upload_url, &path)
        .expect("upload");

    handle.join().expect("server join");
    let _ = std::fs::remove_file(path);
}

#[test]
fn upload_presigned_artifact_missing_file_returns_transport_error() {
    let client = Client::new("http://localhost", Auth::None).unwrap();
    let missing_path = std::env::temp_dir().join("releasy-client-missing-upload.bin");
    let _ = std::fs::remove_file(&missing_path);

    let error = client
        .upload_presigned_artifact("http://localhost/upload", &missing_path)
        .expect_err("expected error");
    match error {
        Error::Transport(ureq::Error::Io(_)) => {}
        other => panic!("unexpected error: {other:?}"),
    }
}
