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
    '''const DEFAULT_MODEL: &str = "dubnium-local";
const BEARER_TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";
const DEFAULT_TIMEOUT_SECONDS: f64 = 120.0;''',
    '''const DEFAULT_MODEL: &str = "dubnium-local";
const BEARER_TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";
const BEARER_TOKEN_FILE_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN_FILE";
const DEFAULT_TIMEOUT_SECONDS: f64 = 120.0;''',
    "bearer token file environment constant",
)

replace_once(
    "src/app.rs",
    '''impl BearerToken {
    fn parse(value: String) -> Result<Self> {
        if value.is_empty() {
            bail!("{BEARER_TOKEN_ENV} must not be empty");
        }
        if value.bytes().any(|byte| byte.is_ascii_whitespace()) {
            bail!("{BEARER_TOKEN_ENV} must not contain whitespace");
        }
        if !value.is_ascii() {
            bail!("{BEARER_TOKEN_ENV} must contain only ASCII characters");
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
}''',
    '''impl BearerToken {
    fn parse(value: String) -> Result<Self> {
        Self::parse_named(value, BEARER_TOKEN_ENV)
    }

    fn parse_named(value: String, source: &str) -> Result<Self> {
        if value.is_empty() {
            bail!("{source} must not be empty");
        }
        if value.bytes().any(|byte| byte.is_ascii_whitespace()) {
            bail!("{source} must not contain whitespace");
        }
        if !value.is_ascii() {
            bail!("{source} must contain only ASCII characters");
        }
        let header_text = format!("Bearer {value}");
        let mut header = HeaderValue::from_bytes(header_text.as_bytes())
            .map_err(|_| anyhow!("{source} contains characters invalid in an HTTP header"))?;
        header.set_sensitive(true);
        Ok(Self(header))
    }

    fn from_file(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)
            .with_context(|| format!("unable to read {BEARER_TOKEN_FILE_ENV}"))?;
        let mut value = String::from_utf8(bytes)
            .map_err(|_| anyhow!("{BEARER_TOKEN_FILE_ENV} must contain valid UTF-8"))?;
        if value.ends_with("\\r\\n") {
            value.truncate(value.len() - 2);
        } else if value.ends_with('\\n') {
            value.pop();
        }
        Self::parse_named(value, BEARER_TOKEN_FILE_ENV)
    }

    fn from_sources(
        direct: Option<OsString>,
        file: Option<OsString>,
    ) -> Result<Option<Self>> {
        match (direct, file) {
            (Some(_), Some(_)) => bail!(
                "{BEARER_TOKEN_ENV} and {BEARER_TOKEN_FILE_ENV} cannot be used together"
            ),
            (Some(value), None) => {
                let value = value
                    .into_string()
                    .map_err(|_| anyhow!("{BEARER_TOKEN_ENV} must be valid UTF-8"))?;
                Self::parse(value).map(Some)
            }
            (None, Some(path)) => {
                if path.is_empty() {
                    bail!("{BEARER_TOKEN_FILE_ENV} must not be empty");
                }
                Self::from_file(&PathBuf::from(path)).map(Some)
            }
            (None, None) => Ok(None),
        }
    }

    fn from_env() -> Result<Option<Self>> {
        Self::from_sources(
            env::var_os(BEARER_TOKEN_ENV),
            env::var_os(BEARER_TOKEN_FILE_ENV),
        )
    }

    fn header_value(&self) -> HeaderValue {
        self.0.clone()
    }
}''',
    "bearer token source resolution",
)

replace_once(
    "src/app.rs",
    '''fn git_command() -> Command {
    let mut command = Command::new("git");
    command.env_remove(BEARER_TOKEN_ENV);
    command
}''',
    '''fn git_command() -> Command {
    let mut command = Command::new("git");
    command.env_remove(BEARER_TOKEN_ENV);
    command.env_remove(BEARER_TOKEN_FILE_ENV);
    command
}''',
    "bearer token file subprocess scrubbing",
)

replace_once(
    "src/app.rs",
    '''    #[test]
    fn git_subprocesses_remove_the_bearer_token_environment() {
        let command = git_command();
        let removed = command
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new(BEARER_TOKEN_ENV));
        assert!(matches!(removed, Some((_, None))));
    }

    #[test]
    fn bearer_token_is_redacted_in_debug_and_json() {''',
    '''    #[test]
    fn git_subprocesses_remove_bearer_token_environments() {
        let command = git_command();
        for name in [BEARER_TOKEN_ENV, BEARER_TOKEN_FILE_ENV] {
            let removed = command
                .get_envs()
                .find(|(key, _)| *key == std::ffi::OsStr::new(name));
            assert!(matches!(removed, Some((_, None))), "{name} was inherited");
        }
    }

    #[test]
    fn bearer_token_file_accepts_no_terminator_lf_or_crlf() {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("token");
        for contents in ["unit-file-secret", "unit-file-secret\\n", "unit-file-secret\\r\\n"] {
            fs::write(&path, contents).unwrap();
            let token = BearerToken::from_file(&path).unwrap();
            assert_eq!(token.header_value().to_str().unwrap(), "Bearer unit-file-secret");
        }
    }

    #[test]
    fn bearer_token_sources_are_mutually_exclusive_without_echoing_values() {
        let error = BearerToken::from_sources(
            Some(OsString::from("direct-unit-secret")),
            Some(OsString::from("/mounted/unit-secret")),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains(BEARER_TOKEN_ENV));
        assert!(message.contains(BEARER_TOKEN_FILE_ENV));
        assert!(!message.contains("direct-unit-secret"));
        assert!(!message.contains("/mounted/unit-secret"));
    }

    #[test]
    fn invalid_bearer_token_file_content_is_not_echoed() {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("token");
        fs::write(&path, "file-unit-secret\\n\\n").unwrap();
        let error = BearerToken::from_file(&path).unwrap_err();
        assert!(error.to_string().contains(BEARER_TOKEN_FILE_ENV));
        assert!(!error.to_string().contains("file-unit-secret"));
    }

    #[test]
    fn bearer_token_is_redacted_in_debug_and_json() {''',
    "bearer token file unit tests",
)

replace_once(
    "README.md",
    '''Authenticated endpoints can use `GIT_AUTOCOMMIT_BEARER_TOKEN`. The token is accepted only from the environment, sent as a sensitive `Authorization: Bearer` header, and rendered as `<redacted>` by `--show-config`. It is not accepted through CLI arguments, TOML, or URL user information, and it is removed from child Git and signing-process environments. Avoid placing secrets directly in shell history.''',
    '''Authenticated endpoints can use either `GIT_AUTOCOMMIT_BEARER_TOKEN` or `GIT_AUTOCOMMIT_BEARER_TOKEN_FILE`. The variables are mutually exclusive. The file form supports mounted container secrets and accepts the token with no terminator, one LF, or one CRLF; no other whitespace is trimmed. The credential is sent as a sensitive `Authorization: Bearer` header and rendered as `<redacted>` by `--show-config`, while the configured file path is omitted. Credentials are not accepted through CLI arguments, TOML, or URL user information, and both variables are removed from child Git and signing-process environments. Prefer the file form for mounted secrets and avoid placing direct tokens in shell history.''',
    "bearer token file security documentation",
)

replace_once(
    "README.md",
    '''| Bearer token | — | `GIT_AUTOCOMMIT_BEARER_TOKEN` | — | unset |
| Timeout | `--timeout` | `GIT_AUTOCOMMIT_TIMEOUT` | `timeout_seconds` | `120` seconds |''',
    '''| Bearer token | — | `GIT_AUTOCOMMIT_BEARER_TOKEN` | — | unset |
| Bearer token file | — | `GIT_AUTOCOMMIT_BEARER_TOKEN_FILE` | — | unset |
| Timeout | `--timeout` | `GIT_AUTOCOMMIT_TIMEOUT` | `timeout_seconds` | `120` seconds |''',
    "bearer token file configuration table",
)

replace_once(
    "README.md",
    '''Use `git autocommit --show-config` to inspect the resolved values. A configured bearer token appears only as `<redacted>`.

For an authenticated endpoint, provide the token in the process environment:

```sh
GIT_AUTOCOMMIT_BEARER_TOKEN="$(cat /secure/path/token)" git autocommit --dry-run
```''',
    '''Use `git autocommit --show-config` to inspect the resolved values. A configured bearer credential appears only as `<redacted>`; token-file paths are not printed.

For an authenticated endpoint, provide either the token directly:

```sh
GIT_AUTOCOMMIT_BEARER_TOKEN="$(cat /secure/path/token)" git autocommit --dry-run
```

or point to a mounted credential file:

```sh
GIT_AUTOCOMMIT_BEARER_TOKEN_FILE=/run/secrets/model-token git autocommit --dry-run
```

Do not set both variables. Token files may end in one LF or CRLF, but additional or embedded whitespace is rejected.''',
    "bearer token file usage",
)

replace_once(
    "README.md",
    '''| `local AI base_url must not include...` | `base_url` contains embedded credentials, a query string, or a fragment; configure only the endpoint origin/path and use `GIT_AUTOCOMMIT_BEARER_TOKEN` when bearer authentication is required. |
| `GIT_AUTOCOMMIT_BEARER_TOKEN must...` | The configured token is empty, contains whitespace or non-ASCII characters, is not UTF-8, or cannot form a valid HTTP header value. |
| `local AI endpoint returned HTTP redirect...` | `base_url` points to a redirecting URL; configure the final endpoint directly. |''',
    '''| `local AI base_url must not include...` | `base_url` contains embedded credentials, a query string, or a fragment; configure only the endpoint origin/path and use one bearer-token environment variable when authentication is required. |
| `GIT_AUTOCOMMIT_BEARER_TOKEN and GIT_AUTOCOMMIT_BEARER_TOKEN_FILE cannot...` | Both credential sources are configured; unset one of them. |
| `GIT_AUTOCOMMIT_BEARER_TOKEN must...` | The direct token is empty, contains whitespace or non-ASCII characters, is not UTF-8, or cannot form a valid HTTP header value. |
| `unable to read GIT_AUTOCOMMIT_BEARER_TOKEN_FILE` | The configured credential file is missing, unreadable, or not a regular readable secret source. |
| `GIT_AUTOCOMMIT_BEARER_TOKEN_FILE must...` | The file path is empty, or its content is empty, invalid UTF-8, contains unsupported whitespace or non-ASCII characters, or cannot form a valid HTTP header value. |
| `local AI endpoint returned HTTP redirect...` | `base_url` points to a redirecting URL; configure the final endpoint directly. |''',
    "bearer token file troubleshooting",
)

replace_once(
    "tests/bearer_token.rs",
    '''const TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";
const TEST_TOKEN: &str = "integration-secret-token";''',
    '''const TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";
const TOKEN_FILE_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN_FILE";
const TEST_TOKEN: &str = "integration-secret-token";''',
    "bearer token file test constant",
)

replace_once(
    "tests/bearer_token.rs",
    '''        .arg("--base-url")
        .arg(&base_url)
        .env(TOKEN_ENV, TEST_TOKEN);''',
    '''        .arg("--base-url")
        .arg(&base_url)
        .env(TOKEN_ENV, TEST_TOKEN)
        .env_remove(TOKEN_FILE_ENV);''',
    "direct token request test isolation",
)

replace_once(
    "tests/bearer_token.rs",
    '''        .arg("--base-url")
        .arg(&base_url)
        .env(TOKEN_ENV, TEST_TOKEN);
    show_config''',
    '''        .arg("--base-url")
        .arg(&base_url)
        .env(TOKEN_ENV, TEST_TOKEN)
        .env_remove(TOKEN_FILE_ENV);
    show_config''',
    "direct token config test isolation",
)

replace_once(
    "tests/bearer_token.rs",
    '''#[test]
fn invalid_bearer_token_fails_before_connecting_without_echoing_it() {''',
    '''#[test]
fn bearer_token_file_is_sent_and_show_config_redacts_source() {
    let repo = init_staged_repo();
    let secret_directory = tempdir().expect("temporary secret directory");
    let token_path = secret_directory.path().join("model-token");
    fs::write(&token_path, format!("{TEST_TOKEN}\\r\\n")).expect("write token file");
    let endpoint = TcpListener::bind("127.0.0.1:0").expect("bind endpoint");
    let endpoint_addr = endpoint.local_addr().expect("endpoint address");

    let endpoint_thread = thread::spawn(move || {
        let (mut stream, _) = endpoint.accept().expect("accept model request");
        let request = read_request(&mut stream);
        let request_text = String::from_utf8_lossy(&request);
        let authorization = request_text.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("authorization")
                .then(|| value.trim())
        });
        assert_eq!(authorization, Some("Bearer integration-secret-token"));

        let plan = r#"[{\"message\":\"test: commit staged change\",\"files\":[\"staged.txt\"]}]"#;
        let body = format!(
            r#"{{\"choices\":[{{\"message\":{{\"content\":{}}}}}]}}"#,
            serde_json::to_string(plan).unwrap()
        );
        let response = format!(
            "HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nContent-Length: {}\\r\\nConnection: close\\r\\n\\r\\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).expect("write endpoint response");
    });

    let base_url = format!("http://{endpoint_addr}/v1");
    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--dry-run")
        .arg("--base-url")
        .arg(&base_url)
        .env_remove(TOKEN_ENV)
        .env(TOKEN_FILE_ENV, &token_path);
    command
        .assert()
        .success()
        .stdout(predicate::str::contains("test: commit staged change"));
    endpoint_thread.join().expect("endpoint thread");

    let token_path_text = token_path.to_string_lossy().into_owned();
    let mut show_config = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    show_config
        .current_dir(repo.path())
        .arg("--show-config")
        .arg("--base-url")
        .arg(&base_url)
        .env_remove(TOKEN_ENV)
        .env(TOKEN_FILE_ENV, &token_path);
    show_config
        .assert()
        .success()
        .stdout(predicate::str::contains("\\\"bearer_token\\\": \\\"<redacted>\\\""))
        .stdout(predicate::str::contains(TEST_TOKEN).not())
        .stdout(predicate::str::contains(token_path_text).not());
}

#[test]
fn conflicting_bearer_token_sources_fail_before_connecting_without_echoing_values() {
    let repo = init_staged_repo();
    let secret_directory = tempdir().expect("temporary secret directory");
    let token_path = secret_directory.path().join("model-token");
    fs::write(&token_path, TEST_TOKEN).expect("write token file");
    let endpoint = TcpListener::bind("127.0.0.1:0").expect("bind endpoint");
    endpoint.set_nonblocking(true).expect("set endpoint nonblocking");
    let endpoint_addr = endpoint.local_addr().expect("endpoint address");
    let token_path_text = token_path.to_string_lossy().into_owned();

    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--dry-run")
        .arg("--base-url")
        .arg(format!("http://{endpoint_addr}/v1"))
        .env(TOKEN_ENV, TEST_TOKEN)
        .env(TOKEN_FILE_ENV, &token_path);
    command
        .assert()
        .failure()
        .stderr(predicate::str::contains(TOKEN_ENV))
        .stderr(predicate::str::contains(TOKEN_FILE_ENV))
        .stderr(predicate::str::contains(TEST_TOKEN).not())
        .stderr(predicate::str::contains(token_path_text).not());

    thread::sleep(Duration::from_millis(50));
    match endpoint.accept() {
        Err(error) if error.kind() == ErrorKind::WouldBlock => {}
        Ok(_) => panic!("endpoint unexpectedly received a request with conflicting token sources"),
        Err(error) => panic!("unexpected endpoint error: {error}"),
    }
}

#[test]
fn invalid_bearer_token_file_fails_before_connecting_without_echoing_content() {
    let repo = init_staged_repo();
    let secret_directory = tempdir().expect("temporary secret directory");
    let token_path = secret_directory.path().join("model-token");
    let invalid = "file integration secret\\n\\n";
    fs::write(&token_path, invalid).expect("write invalid token file");
    let endpoint = TcpListener::bind("127.0.0.1:0").expect("bind endpoint");
    endpoint.set_nonblocking(true).expect("set endpoint nonblocking");
    let endpoint_addr = endpoint.local_addr().expect("endpoint address");

    let mut command = Command::new(env!("CARGO_BIN_EXE_git-autocommit"));
    command
        .current_dir(repo.path())
        .arg("--dry-run")
        .arg("--base-url")
        .arg(format!("http://{endpoint_addr}/v1"))
        .env_remove(TOKEN_ENV)
        .env(TOKEN_FILE_ENV, &token_path);
    command
        .assert()
        .failure()
        .stderr(predicate::str::contains(TOKEN_FILE_ENV))
        .stderr(predicate::str::contains("file integration secret").not());

    thread::sleep(Duration::from_millis(50));
    match endpoint.accept() {
        Err(error) if error.kind() == ErrorKind::WouldBlock => {}
        Ok(_) => panic!("endpoint unexpectedly received a request with an invalid token file"),
        Err(error) => panic!("unexpected endpoint error: {error}"),
    }
}

#[test]
fn invalid_bearer_token_fails_before_connecting_without_echoing_it() {''',
    "bearer token file integration tests",
)

replace_once(
    "tests/bearer_token.rs",
    '''        .arg("--base-url")
        .arg(format!("http://{endpoint_addr}/v1"))
        .env(TOKEN_ENV, invalid);''',
    '''        .arg("--base-url")
        .arg(format!("http://{endpoint_addr}/v1"))
        .env(TOKEN_ENV, invalid)
        .env_remove(TOKEN_FILE_ENV);''',
    "invalid direct token test isolation",
)
