//! Shared, bounded native lookup and translation transport. No publication-wide uploads.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;
#[derive(Clone, Serialize, Deserialize)]
pub struct ServiceRequest {
    pub endpoint: String,
    #[serde(default)]
    pub key: String,
    pub text: String,
    pub kind: String,
    #[serde(default = "target")]
    pub target: String,
    #[serde(default = "model")]
    pub model: String,
    #[serde(default = "provider")]
    pub provider: String,
}
fn target() -> String {
    "zh".into()
}
fn model() -> String {
    "gpt-4o-mini".into()
}
fn provider() -> String {
    "auto".into()
}
impl Default for ServiceRequest {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            key: String::new(),
            text: String::new(),
            kind: String::new(),
            target: target(),
            model: model(),
            provider: provider(),
        }
    }
}

pub fn query(request: &ServiceRequest) -> Result<String, String> {
    if request.text.trim().is_empty()
        || request.text.chars().count() > 16_000
        || !matches!(request.kind.as_str(), "translate" | "dictionary")
    {
        return Err("Invalid selected text".into());
    }
    let mut url =
        reqwest::Url::parse(request.endpoint.trim()).map_err(|_| "Invalid service URL")?;
    if url.scheme() != "https"
        && !(url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))
    {
        return Err("Service requires HTTPS".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Credentials must use the API key field".into());
    }
    if !matches!(request.provider.as_str(), "auto" | "json" | "openai")
        || request.model.len() > 200
        || request.key.len() > 8192
        || request.target.len() > 48
    {
        return Err("Invalid reading service configuration".into());
    }
    let openai = request.provider == "openai"
        || (request.provider == "auto"
            && url
                .path()
                .trim_end_matches('/')
                .ends_with("/chat/completions"));
    if openai
        && !url
            .path()
            .trim_end_matches('/')
            .ends_with("/chat/completions")
    {
        let base = url.path().trim_end_matches('/');
        url.set_path(&format!(
            "{}/chat/completions",
            if base.is_empty() { "/v1" } else { base }
        ));
    } else if openai {
        let path = url.path().trim_end_matches('/').to_owned();
        url.set_path(&path);
    }
    let pointer = url.fragment().map(str::to_owned);
    url.set_fragment(None);
    let model = url
        .query_pairs()
        .find(|(key, _)| key == "model")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| request.model.clone());
    if openai && (model.trim().is_empty() || model.len() > 200) {
        return Err("Invalid reading service model".into());
    }
    if openai {
        let pairs = url
            .query_pairs()
            .filter(|(k, _)| k != "model")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<Vec<_>>();
        url.set_query(None);
        if !pairs.is_empty() {
            url.query_pairs_mut().extend_pairs(pairs);
        }
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Cannot prepare reading service")?;
    let mut call = client
        .post(url.clone())
        .header("User-Agent", "RustEpubReader");
    if !request.key.is_empty() {
        call = call.bearer_auth(&request.key);
    }
    let body = if openai {
        json!({"model":model,"messages":[{"role":"system","content":format!("{} the selected text. Target language: {}. Return only the result.",if request.kind=="dictionary"{"Explain the meaning of"}else{"Translate"},request.target)},{"role":"user","content":request.text}],"stream":false})
    } else {
        json!({"text":request.text,"source":"auto","target":request.target,"kind":request.kind})
    };
    let response = call
        .json(&body)
        .send()
        .map_err(|_| "Reading service connection failed or timed out")?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Reading service returned HTTP {}", status.as_u16()));
    }
    let mut bytes = Vec::new();
    response
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read service response")?;
    if bytes.len() > 1024 * 1024 {
        return Err("Reading service response exceeds 1 MiB".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Reading service must return JSON")?;
    let result = if let Some(pointer) = pointer {
        value.pointer(&pointer).and_then(Value::as_str)
    } else {
        value.as_str().or_else(|| {
            [
                "/text",
                "/result",
                "/translatedText",
                "/translation",
                "/choices/0/message/content",
            ]
            .iter()
            .find_map(|p| value.pointer(p).and_then(Value::as_str))
        })
    };
    result
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .ok_or("No text result in service response; set a JSON pointer in the URL fragment".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_service_contract_and_errors() {
        use std::net::TcpListener;
        for (body, expected) in [
            (r#"{"text":"译文"}"#, Some("译文")),
            (r#"{"nested":{"value":"释义"}}"#, Some("释义")),
            ("{}", None),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let worker = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                let mut bytes = [0; 8192];
                let n = socket.read(&mut bytes).unwrap();
                let headers = String::from_utf8_lossy(&bytes[..n]);
                assert!(headers.contains("POST /lookup"));
                use std::io::Write;
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            });
            let result = query(&ServiceRequest {
                endpoint: format!(
                    "http://{address}/lookup{}",
                    if body.contains("nested") {
                        "#/nested/value"
                    } else {
                        ""
                    }
                ),
                text: "selected".into(),
                kind: "translate".into(),
                ..Default::default()
            });
            worker.join().unwrap();
            if let Some(expected) = expected {
                assert_eq!(result.unwrap(), expected);
            } else {
                assert!(result.is_err());
            }
        }
    }
    #[test]
    fn rejects_unsafe_urls_and_unbounded_text() {
        for endpoint in [
            "file:///tmp/book",
            "http://example.com",
            "https://user:secret@example.com",
        ] {
            assert!(query(&ServiceRequest {
                endpoint: endpoint.into(),
                text: "word".into(),
                kind: "dictionary".into(),
                ..Default::default()
            })
            .is_err());
        }
    }
    #[test]
    fn openai_base_urls_model_language_and_auth() {
        use std::{io::Write, net::TcpListener};
        for (suffix, path, chosen) in [
            ("", "/v1/chat/completions", "custom-model"),
            ("/v1/", "/v1/chat/completions", "custom-model"),
            (
                "/api/chat/completions/?model=legacy-model",
                "/api/chat/completions",
                "legacy-model",
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let worker = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0; 4096];
                let header_end = loop {
                    let n = socket.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&bytes[..header_end]).to_lowercase();
                assert!(headers.starts_with(&format!("post {path} http")));
                assert!(headers.contains("authorization: bearer test-only-key"));
                let length: usize = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                while bytes.len() < header_end + length {
                    let n = socket.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                }
                let body: Value = serde_json::from_slice(&bytes[header_end..]).unwrap();
                assert_eq!(body["model"], chosen);
                assert_eq!(body["messages"][1]["content"], "selected text");
                assert!(body["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("Target language: en"));
                assert_eq!(body["stream"], false);
                let response = r#"{"choices":[{"message":{"content":"result"}}]}"#;
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.len(),
                    response
                )
                .unwrap();
            });
            assert_eq!(
                query(&ServiceRequest {
                    endpoint: format!("http://{address}{suffix}"),
                    key: "test-only-key".into(),
                    text: "selected text".into(),
                    kind: "translate".into(),
                    target: "en".into(),
                    model: "custom-model".into(),
                    provider: "openai".into()
                })
                .unwrap(),
                "result"
            );
            worker.join().unwrap();
        }
    }
    #[test]
    fn service_never_follows_redirects() {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut b = [0; 4096];
            let _ = socket.read(&mut b);
            write!(socket,"HTTP/1.1 302 Found\r\nLocation: https://example.com/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        assert!(query(&ServiceRequest {
            endpoint: format!("http://{address}/lookup"),
            text: "word".into(),
            kind: "dictionary".into(),
            ..Default::default()
        })
        .unwrap_err()
        .contains("HTTP 302"));
        worker.join().unwrap();
    }
}
