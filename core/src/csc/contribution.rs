//! Reviewed, bounded samples and explicit public contribution. Never called by sync.
use crate::{
    epub::{ContentBlock, EpubBook},
    library::BookConfig,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Sample {
    pub input: String,
    pub output: String,
}
pub fn samples(book: &EpubBook, cfg: &BookConfig) -> Result<Vec<Sample>, String> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for c in cfg
        .corrections
        .iter()
        .filter(|c| c.status == "accepted")
        .take(10_000)
    {
        let chapter = book.chapter(c.chapter)?;
        let Some(ContentBlock::Paragraph { spans, .. } | ContentBlock::Heading { spans, .. }) =
            chapter.blocks.get(c.block_idx)
        else {
            continue;
        };
        let chars: Vec<char> = spans.iter().flat_map(|s| s.text.chars()).collect();
        let original: Vec<char> = c.original.chars().collect();
        if original.is_empty()
            || original.len() != c.corrected.chars().count()
            || chars.get(c.char_offset..c.char_offset.saturating_add(original.len()))
                != Some(original.as_slice())
        {
            continue;
        }
        let start = c.char_offset.saturating_sub(10);
        let end = (c.char_offset + original.len() + 10).min(chars.len());
        let input: String = chars[start..end].iter().collect();
        let mut corrected = chars[start..end].to_vec();
        corrected.splice(
            c.char_offset - start..c.char_offset - start + original.len(),
            c.corrected.chars(),
        );
        let output: String = corrected.iter().collect();
        if input == output
            || input.chars().count() > 50
            || !seen.insert((input.clone(), output.clone()))
        {
            continue;
        }
        result.push(Sample { input, output });
        if result.len() == 500 {
            break;
        }
    }
    Ok(result)
}
pub fn jsonl(samples: &[Sample]) -> Result<String, String> {
    if samples.is_empty() || samples.len() > 500 {
        return Err("Select 1–500 contribution samples".into());
    }
    let mut result = String::new();
    for sample in samples {
        let len = sample.input.chars().count();
        if len == 0
            || len > 50
            || len != sample.output.chars().count()
            || sample.input == sample.output
            || sample.input.contains(['\n', '\r'])
            || sample.output.contains(['\n', '\r'])
        {
            return Err("Invalid contribution sample".into());
        }
        result.push_str(&serde_json::to_string(sample).map_err(|e| e.to_string())?);
        result.push('\n');
    }
    Ok(result)
}
#[cfg(feature = "github-sync")]
pub fn submit(token: &str, samples: &[Sample]) -> Result<String, String> {
    submit_at("https://api.github.com", token, samples)
}
#[cfg(feature = "github-sync")]
fn submit_at(api: &str, token: &str, samples: &[Sample]) -> Result<String, String> {
    use base64::Engine;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    let content = jsonl(samples)?;
    if token.is_empty() {
        return Err("GitHub login required".into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Cannot prepare contribution")?;
    let upstream = "zhongbai2333/RustEpubReader-Model";
    let call =
        |method: reqwest::Method, path: &str, body: Option<Value>| -> Result<Value, String> {
            let mut request = client
                .request(method, format!("{api}/{path}"))
                .bearer_auth(token)
                .header("User-Agent", "RustEpubReader")
                .header("Accept", "application/vnd.github+json");
            if let Some(body) = body {
                request = request.json(&body);
            }
            let response = request
                .send()
                .map_err(|_| "GitHub contribution connection failed")?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("GitHub contribution HTTP {}", status.as_u16()));
            }
            response
                .json()
                .map_err(|_| "Invalid GitHub contribution response".into())
        };
    let user = call(reqwest::Method::GET, "user", None)?;
    let owner = user["login"]
        .as_str()
        .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'))
        .ok_or("Invalid GitHub identity")?;
    let repo = format!("{owner}/RustEpubReader-Model");
    let upstream_info = call(reqwest::Method::GET, &format!("repos/{upstream}"), None)?;
    let base = upstream_info["default_branch"]
        .as_str()
        .ok_or("Missing contribution base")?;
    let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
    let branch = format!("reader-csc-{}", &hash[..20]);
    let existing = call(
        reqwest::Method::GET,
        &format!("repos/{upstream}/pulls?head={owner}:{branch}&state=all"),
        None,
    )?;
    if let Some(url) = existing
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v["html_url"].as_str())
    {
        return Ok(url.into());
    }
    // Fork creation is idempotent. Never write the user's default branch.
    let _ = call(
        reqwest::Method::POST,
        &format!("repos/{upstream}/forks"),
        Some(json!({"default_branch_only":true})),
    )?;
    let mut fork = None;
    for _ in 0..5 {
        match call(reqwest::Method::GET, &format!("repos/{repo}"), None) {
            Ok(value) => {
                fork = Some(value);
                break;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(500)),
        }
    }
    let fork = fork.ok_or("GitHub fork is being prepared; retry shortly")?;
    if fork["parent"]["full_name"].as_str() != Some(upstream) {
        return Err("Contribution fork has a different upstream".into());
    }
    let default = fork["default_branch"]
        .as_str()
        .ok_or("Missing fork branch")?;
    let reference = call(
        reqwest::Method::GET,
        &format!("repos/{repo}/git/ref/heads/{default}"),
        None,
    )?;
    let sha = reference["object"]["sha"]
        .as_str()
        .ok_or("Missing fork revision")?;
    let refpath = format!("repos/{repo}/git/ref/heads/{branch}");
    if call(reqwest::Method::GET, &refpath, None).is_err() {
        call(
            reqwest::Method::POST,
            &format!("repos/{repo}/git/refs"),
            Some(json!({"ref":format!("refs/heads/{branch}"),"sha":sha})),
        )?;
    }
    let path = format!("datasets/submissions/{hash}.jsonl");
    let contentpath = format!("repos/{repo}/contents/{path}");
    if call(
        reqwest::Method::GET,
        &format!("{contentpath}?ref={branch}"),
        None,
    )
    .is_err()
    {
        call(
            reqwest::Method::PUT,
            &contentpath,
            Some(
                json!({"message":"Add reviewed correction samples","branch":branch,"content":base64::engine::general_purpose::STANDARD.encode(content)}),
            ),
        )?;
    }
    let pr = call(
        reqwest::Method::POST,
        &format!("repos/{upstream}/pulls"),
        Some(
            json!({"title":format!("CSC correction data from {owner}"),"body":format!("User-reviewed correction samples from RustEpubReader.\n\nSamples: {}",samples.len()),"head":format!("{owner}:{branch}"),"base":base}),
        ),
    )?;
    pr["html_url"]
        .as_str()
        .map(str::to_owned)
        .ok_or("Missing contribution URL".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_character_lengths_not_utf8_byte_lengths() {
        assert!(jsonl(&[Sample {
            input: "中a".into(),
            output: "文b".into()
        }])
        .is_ok());
        assert!(jsonl(&[Sample {
            input: "中a".into(),
            output: "文".into()
        }])
        .is_err());
        assert!(jsonl(&[Sample {
            input: "私\n文".into(),
            output: "公\n文".into()
        }])
        .is_err());
    }
}

#[cfg(all(test, feature = "github-sync"))]
mod submission_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::{Duration, Instant},
    };
    fn server(
        responses: Vec<(u16, serde_json::Value)>,
    ) -> (
        String,
        std::thread::JoinHandle<Vec<(String, serde_json::Value)>>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let worker = std::thread::spawn(move || {
            let mut calls = Vec::new();
            for (status, body) in responses {
                let deadline = Instant::now() + Duration::from_secs(4);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                Instant::now() < deadline,
                                "Missing mocked contribution request"
                            );
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                let (headers, length, offset) = loop {
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(at) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                        let headers = String::from_utf8(bytes[..at].to_vec()).unwrap();
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(str::trim)
                                    .and_then(|s| s.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        break (headers, length, at + 4);
                    }
                };
                while bytes.len() < offset + length {
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                calls.push((
                    headers.lines().next().unwrap().to_owned(),
                    if length == 0 {
                        serde_json::Value::Null
                    } else {
                        serde_json::from_slice(&bytes[offset..offset + length]).unwrap()
                    },
                ));
                let body = body.to_string();
                write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
            calls
        });
        (address, worker)
    }
    #[test]
    fn reviewed_submission_uses_dedicated_content_branch_and_exact_selected_samples() {
        use serde_json::json;
        let responses = vec![
            (200, json!({"login":"reader-test"})),
            (200, json!({"default_branch":"main"})),
            (200, json!([])),
            (202, json!({})),
            (
                200,
                json!({"parent":{"full_name":"zhongbai2333/RustEpubReader-Model"},"default_branch":"main"}),
            ),
            (200, json!({"object":{"sha":"abc"}})),
            (404, json!({})),
            (201, json!({})),
            (404, json!({})),
            (201, json!({})),
            (
                201,
                json!({"html_url":"https://github.com/zhongbai2333/RustEpubReader-Model/pull/123"}),
            ),
        ];
        let (api, worker) = server(responses);
        let samples = vec![Sample {
            input: "原文".into(),
            output: "新文".into(),
        }];
        let result = submit_at(&api, "mock-token-not-a-secret", &samples).unwrap();
        assert!(result.ends_with("/123"));
        let calls = worker.join().unwrap();
        assert_eq!(calls.len(), 11);
        let branch = calls[7].1["ref"].as_str().unwrap();
        assert!(branch.starts_with("refs/heads/reader-csc-"));
        assert_ne!(branch, "refs/heads/main");
        assert_eq!(
            calls[9].1["branch"],
            branch.trim_start_matches("refs/heads/")
        );
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(calls[9].1["content"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            String::from_utf8(decoded).unwrap(),
            jsonl(&samples).unwrap()
        );
        assert_eq!(
            calls[10].1["head"],
            format!("reader-test:{}", branch.trim_start_matches("refs/heads/"))
        );
    }
    #[test]
    fn duplicate_contribution_returns_existing_request_without_new_write() {
        use serde_json::json;
        let (api, worker) = server(vec![
            (200, json!({"login":"reader-test"})),
            (200, json!({"default_branch":"main"})),
            (
                200,
                json!([{"html_url":"https://github.com/zhongbai2333/RustEpubReader-Model/pull/123"}]),
            ),
        ]);
        let url = submit_at(
            &api,
            "mock-token-not-a-secret",
            &[Sample {
                input: "原文".into(),
                output: "新文".into(),
            }],
        )
        .unwrap();
        assert!(url.ends_with("/123"));
        assert!(worker
            .join()
            .unwrap()
            .iter()
            .all(|(line, _)| line.starts_with("GET ")));
    }
    #[test]
    fn rejected_credentials_stop_before_any_public_write() {
        let (api, worker) = server(vec![(401, serde_json::json!({}))]);
        assert!(submit_at(
            &api,
            "invalid-mock-token",
            &[Sample {
                input: "原文".into(),
                output: "新文".into()
            }]
        )
        .unwrap_err()
        .contains("401"));
        assert_eq!(worker.join().unwrap().len(), 1);
    }
}
