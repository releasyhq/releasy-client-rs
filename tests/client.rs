use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use releasy_client::{Auth, Client, Error, ReleaseCreateRequest, ReleaseListQuery};

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
