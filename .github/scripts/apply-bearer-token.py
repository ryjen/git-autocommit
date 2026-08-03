from pathlib import Path


def replace_once(path: str, old: str, new: str, label: str) -> None:
    target = Path(path)
    text = target.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match in {path}, found {count}")
    target.write_text(text.replace(old, new, 1))


replace_once(
    "src/app.rs",
    '''use reqwest::{StatusCode, Url, blocking::Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::env;''',
    '''use reqwest::{
    StatusCode, Url,
    blocking::Client,
    header::{AUTHORIZATION, HeaderValue},
    redirect::Policy,
};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;''',
    "secret-related imports",
)

replace_once(
    "src/app.rs",
    '''const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8000/v1";
const DEFAULT_MODEL: &str = "dubnium-local";''',
    '''const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8000/v1";
const DEFAULT_MODEL: &str = "dubnium-local";
const BEARER_TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";''',
    "bearer token environment constant",
)

replace_once(
    "src/app.rs",
    '''#[derive(Debug, Serialize)]
struct Settings {
    base_url: String,
    model: String,''',
    '''struct BearerToken(HeaderValue);

impl BearerToken {
    fn parse(value: String) -> Result<Self> {
        if value.is_empty() {
            bail!("{BEARER_TOKEN_ENV} must not be empty");
        }
        if value.bytes().any(|byte| byte.is_ascii_whitespace()) {
            bail!("{BEARER_TOKEN_ENV} must not contain whitespace");
        }
        let header_text = format!("Bearer {value}");
        let mut header = HeaderValue::from_bytes(header_text.as_bytes()).map_err(|_| {
            anyhow!("{BEARER_TOKEN_ENV} contains characters invalid in an HTTP header")
        })?;
        header.set_sensitive(true);
        Ok(Self(header))
    }

    fn from_env() -> Result<Option<Self>> {
        match env::var(BEARER_TOKEN_ENV) {
            Ok(value) => Self::parse(value).map(Some),
            Err(env::VarError::NotPresent) => Ok(None),
            Err(env::VarError::NotUnicode(_)) => {
                bail!("{BEARER_TOKEN_ENV} must be valid UTF-8")
            }
        }
    }

    fn header_value(&self) -> HeaderValue {
        self.0.clone()
    }
}

impl fmt::Debug for BearerToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

impl Serialize for BearerToken {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str("<redacted>")
    }
}

#[derive(Debug, Serialize)]
struct Settings {
    base_url: String,
    model: String,
    bearer_token: Option<BearerToken>,''',
    "redacting bearer token type and setting",
)

replace_once(
    "src/app.rs",
    '''        model: cli
            .model
            .clone()
            .or_else(|| env_string("GIT_AUTOCOMMIT_MODEL", Some("DUBNIUM_LOCAL_LLM_MODEL")))
            .or(config.model)
            .unwrap_or_else(|| DEFAULT_MODEL.to_owned()),
        timeout_seconds: positive_f64(timeout, "timeout")?,''',
    '''        model: cli
            .model
            .clone()
            .or_else(|| env_string("GIT_AUTOCOMMIT_MODEL", Some("DUBNIUM_LOCAL_LLM_MODEL")))
            .or(config.model)
            .unwrap_or_else(|| DEFAULT_MODEL.to_owned()),
        bearer_token: BearerToken::from_env()?,
        timeout_seconds: positive_f64(timeout, "timeout")?,''',
    "environment-only bearer token resolution",
)

replace_once(
    "src/app.rs",
    '''    let response = client
        .post(request_url)
        .json(&json!({
            "model": settings.model,
            "temperature": 0.1,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ]
        }))
        .send()
        .context("local AI unavailable")?;''',
    '''    let mut request = client.post(request_url).json(&json!({
        "model": settings.model,
        "temperature": 0.1,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]
    }));
    if let Some(token) = &settings.bearer_token {
        request = request.header(AUTHORIZATION, token.header_value());
    }
    let response = request.send().context("local AI unavailable")?;''',
    "sensitive authorization header",
)

replace_once(
    "src/app.rs",
    '''    #[test]
    fn allows_https_and_loopback_http_model_endpoints() {''',
    '''    #[test]
    fn bearer_token_is_redacted_in_debug_and_json() {
        let token = BearerToken::parse("unit-test-secret".to_owned()).unwrap();
        assert_eq!(format!("{token:?}"), "<redacted>");
        let serialized = serde_json::to_string(&token).unwrap();
        assert_eq!(serialized, "\\\"<redacted>\\\"");
        assert!(!serialized.contains("unit-test-secret"));
    }

    #[test]
    fn rejects_invalid_bearer_tokens_without_echoing_them() {
        for value in ["", "contains space", "line\\nbreak", "tökën"] {
            let error = BearerToken::parse(value.to_owned()).unwrap_err();
            assert!(error.to_string().contains(BEARER_TOKEN_ENV));
            if !value.is_empty() {
                assert!(!error.to_string().contains(value));
            }
        }
    }

    #[test]
    fn settings_json_redacts_configured_bearer_token() {
        let mut settings = settings_for(&[], FileConfig::default());
        settings.bearer_token = Some(BearerToken::parse("settings-secret".to_owned()).unwrap());
        let serialized = serde_json::to_string(&settings).unwrap();
        assert!(serialized.contains("\\\"bearer_token\\\":\\\"<redacted>\\\""));
        assert!(!serialized.contains("settings-secret"));
    }

    #[test]
    fn allows_https_and_loopback_http_model_endpoints() {''',
    "bearer token unit tests",
)

replace_once(
    "README.md",
    '''`base_url` must contain only the endpoint origin and optional API path. Embedded credentials, query parameters, and fragments are rejected before connecting. The client appends `chat/completions` as URL path segments rather than through string concatenation.

System and environment proxy settings are disabled for model requests.''',
    '''`base_url` must contain only the endpoint origin and optional API path. Embedded credentials, query parameters, and fragments are rejected before connecting. The client appends `chat/completions` as URL path segments rather than through string concatenation.

Authenticated endpoints can use `GIT_AUTOCOMMIT_BEARER_TOKEN`. The token is accepted only from the environment, sent as a sensitive `Authorization: Bearer` header, and rendered as `<redacted>` by `--show-config`. It is not accepted through CLI arguments, TOML, or URL user information. Avoid placing secrets directly in shell history.

System and environment proxy settings are disabled for model requests.''',
    "bearer token security documentation",
)

replace_once(
    "README.md",
    '''| Model | `--model` | `GIT_AUTOCOMMIT_MODEL` | `model` | `dubnium-local` |
| Timeout | `--timeout` | `GIT_AUTOCOMMIT_TIMEOUT` | `timeout_seconds` | `120` seconds |''',
    '''| Model | `--model` | `GIT_AUTOCOMMIT_MODEL` | `model` | `dubnium-local` |
| Bearer token | — | `GIT_AUTOCOMMIT_BEARER_TOKEN` | — | unset |
| Timeout | `--timeout` | `GIT_AUTOCOMMIT_TIMEOUT` | `timeout_seconds` | `120` seconds |''',
    "bearer token configuration table",
)

replace_once(
    "README.md",
    '''Use `git autocommit --show-config` to inspect the resolved values.

## Prompt customization''',
    '''Use `git autocommit --show-config` to inspect the resolved values. A configured bearer token appears only as `<redacted>`.

For an authenticated endpoint, provide the token in the process environment:

```sh
GIT_AUTOCOMMIT_BEARER_TOKEN="$(cat /secure/path/token)" git autocommit --dry-run
```

## Prompt customization''',
    "bearer token usage example",
)

replace_once(
    "README.md",
    '''| `local AI base_url must not include...` | `base_url` contains embedded credentials, a query string, or a fragment; URL-based authentication is not supported, so configure only the endpoint origin/path. |
| `local AI endpoint returned HTTP redirect...` | `base_url` points to a redirecting URL; configure the final endpoint directly. |''',
    '''| `local AI base_url must not include...` | `base_url` contains embedded credentials, a query string, or a fragment; configure only the endpoint origin/path and use `GIT_AUTOCOMMIT_BEARER_TOKEN` when bearer authentication is required. |
| `GIT_AUTOCOMMIT_BEARER_TOKEN must...` | The configured token is empty, contains whitespace, is not UTF-8, or cannot form a valid HTTP header value. |
| `local AI endpoint returned HTTP redirect...` | `base_url` points to a redirecting URL; configure the final endpoint directly. |''',
    "bearer token troubleshooting",
)

Path("tests/bearer_token.rs").write_text(r'''use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command as StdCommand;
use std::thread;
use std::time::Duration;
use tempfile::{TempDir, tempdir};

const TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";
const TEST_TOKEN: &str = "integration-secret-token";

fn git(repo: &Path, args: &[&str]) -> std::process::Output {
    StdCommand::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git should be available")
}

fn assert_git_success(repo: &Path, args: &[&str]) {
    let output = git(repo, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_staged_repo() -> TempDir {
    let repo = tempdir().expect("temporary repository");
    assert_git_success(repo.path(), &["init", "--quiet"]);
    fs::write(repo.path().join("README.md"), "initial\n").expect("write initial file");
    assert_git_success(repo.path(), &["add", "README.md"]);
    assert_git_success(
        repo.path(),
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--quiet",
            "-m",
            "initial",
        ],
    );
    fs::write(repo.path().join("staged.txt"), "staged content\n").expect("write staged file");
    assert_git_success(repo.path(), &["add", "staged.txt"]);
    repo
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set read timeout");
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                request.extend_from_slice(&buffer[..count]);
                if let Some(headers_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers_end = headers_end + 4;
                    let headers = String::from_utf8_lossy(&request[..headers_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if request.len() >= headers_end + content_length {
                        break;
                    }
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                break;
            }
            Err(error) => panic!("request read failed: {error}"),
        }
    }
    request
}

#[test]
fn bearer_token_is_sent_and_show_config_redacts_it() {
    let repo = init_staged_repo();
    let endpoint = TcpListener::bind("127.0.0.1:0").expect("bind endpoint");
    let endpoint_addr = endpoint.local_addr().expect("endpoint address");

    let endpoint_thread = thread::spawn(move || {
        let (mut stream, _) = endpoint.accept().expect("accept model request");
        let request = read_request(&mut stream);
        let request_text = String::from_utf8_lossy(&request);
        assert!(
            request_text.starts_with("POST /v1/chat/completions HTTP/1.1"),
            "unexpected request path: {request_text}"
        );
        let authorization = request_text.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("authorization")
                .then(|| value.trim())
        });
        assert_eq!(authorization, Some("Bearer integration-secret-token"));

        let plan = r#"[{"message":"test: commit staged change","files":["staged.txt"]}]"#;
        let body = format!(
            r#"{{"choices":[{{"message":{{"content":{}}}}}]}}"#,
            serde_json::to_string(plan).unwrap()
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write endpoint response");
    });

    let base_url = format!("http://{endpoint_addr}/v1");
    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--dry-run")
        .arg("--base-url")
        .arg(&base_url)
        .env(TOKEN_ENV, TEST_TOKEN);
    command
        .assert()
        .success()
        .stdout(predicate::str::contains("test: commit staged change"));
    endpoint_thread.join().expect("endpoint thread");

    let mut show_config = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    show_config
        .current_dir(repo.path())
        .arg("--show-config")
        .arg("--base-url")
        .arg(&base_url)
        .env(TOKEN_ENV, TEST_TOKEN);
    show_config
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"bearer_token\": \"<redacted>\"",
        ))
        .stdout(predicate::str::contains(TEST_TOKEN).not());
}

#[test]
fn invalid_bearer_token_fails_before_connecting_without_echoing_it() {
    let repo = init_staged_repo();
    let endpoint = TcpListener::bind("127.0.0.1:0").expect("bind endpoint");
    endpoint
        .set_nonblocking(true)
        .expect("set endpoint nonblocking");
    let endpoint_addr = endpoint.local_addr().expect("endpoint address");
    let invalid = "secret token";

    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--dry-run")
        .arg("--base-url")
        .arg(format!("http://{endpoint_addr}/v1"))
        .env(TOKEN_ENV, invalid);
    command
        .assert()
        .failure()
        .stderr(predicate::str::contains(TOKEN_ENV))
        .stderr(predicate::str::contains(invalid).not());

    thread::sleep(Duration::from_millis(50));
    match endpoint.accept() {
        Err(error) if error.kind() == ErrorKind::WouldBlock => {}
        Ok(_) => panic!("endpoint unexpectedly received a request with an invalid token"),
        Err(error) => panic!("unexpected endpoint error: {error}"),
    }
}
''')
