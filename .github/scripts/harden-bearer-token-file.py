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
    '''const BEARER_TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";
const BEARER_TOKEN_FILE_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN_FILE";
const DEFAULT_TIMEOUT_SECONDS: f64 = 120.0;''',
    '''const BEARER_TOKEN_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN";
const BEARER_TOKEN_FILE_ENV: &str = "GIT_AUTOCOMMIT_BEARER_TOKEN_FILE";
const MAX_BEARER_TOKEN_BYTES: usize = 16 * 1024;
const DEFAULT_TIMEOUT_SECONDS: f64 = 120.0;''',
    "bearer token size limit",
)

replace_once(
    "src/app.rs",
    '''        if value.is_empty() {
            bail!("{source} must not be empty");
        }
        if value.bytes().any(|byte| byte.is_ascii_whitespace()) {''',
    '''        if value.is_empty() {
            bail!("{source} must not be empty");
        }
        if value.len() > MAX_BEARER_TOKEN_BYTES {
            bail!("{source} exceeds the {MAX_BEARER_TOKEN_BYTES}-byte limit");
        }
        if value.bytes().any(|byte| byte.is_ascii_whitespace()) {''',
    "bearer token length validation",
)

replace_once(
    "src/app.rs",
    '''    fn from_file(path: &Path) -> Result<Self> {
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
    }''',
    '''    fn from_file(path: &Path) -> Result<Self> {
        let file = fs::File::open(path)
            .with_context(|| format!("unable to read {BEARER_TOKEN_FILE_ENV}"))?;
        let metadata = file
            .metadata()
            .with_context(|| format!("unable to inspect {BEARER_TOKEN_FILE_ENV}"))?;
        if !metadata.is_file() {
            bail!("{BEARER_TOKEN_FILE_ENV} must reference a regular file");
        }
        let read_limit = MAX_BEARER_TOKEN_BYTES + 3;
        let mut bytes = Vec::with_capacity(
            usize::try_from(metadata.len())
                .unwrap_or(read_limit)
                .min(read_limit),
        );
        file.take(read_limit as u64)
            .read_to_end(&mut bytes)
            .with_context(|| format!("unable to read {BEARER_TOKEN_FILE_ENV}"))?;
        if bytes.len() > MAX_BEARER_TOKEN_BYTES + 2 {
            bail!(
                "{BEARER_TOKEN_FILE_ENV} exceeds the {MAX_BEARER_TOKEN_BYTES}-byte token limit"
            );
        }
        let mut value = String::from_utf8(bytes)
            .map_err(|_| anyhow!("{BEARER_TOKEN_FILE_ENV} must contain valid UTF-8"))?;
        if value.ends_with("\\r\\n") {
            value.truncate(value.len() - 2);
        } else if value.ends_with('\\n') {
            value.pop();
        }
        Self::parse_named(value, BEARER_TOKEN_FILE_ENV)
    }''',
    "bounded regular-file token read",
)

replace_once(
    "src/app.rs",
    '''    #[test]
    fn bearer_token_sources_are_mutually_exclusive_without_echoing_values() {''',
    '''    #[test]
    fn bearer_token_file_rejects_non_regular_and_oversized_sources() {
        let directory = TempDir::new().unwrap();
        let non_regular = BearerToken::from_file(directory.path()).unwrap_err();
        assert!(non_regular.to_string().contains("regular file"));

        let path = directory.path().join("oversized-token");
        fs::write(&path, vec![b'a'; MAX_BEARER_TOKEN_BYTES + 1]).unwrap();
        let oversized = BearerToken::from_file(&path).unwrap_err();
        assert!(oversized.to_string().contains("byte limit"));
    }

    #[cfg(unix)]
    #[test]
    fn bearer_token_file_accepts_symlinked_secret_mounts() {
        use std::os::unix::fs::symlink;

        let directory = TempDir::new().unwrap();
        let target = directory.path().join("..data-token");
        let link = directory.path().join("token");
        fs::write(&target, "symlink-unit-secret\\n").unwrap();
        symlink(&target, &link).unwrap();
        let token = BearerToken::from_file(&link).unwrap();
        assert_eq!(token.header_value().to_str().unwrap(), "Bearer symlink-unit-secret");
    }

    #[test]
    fn bearer_token_sources_are_mutually_exclusive_without_echoing_values() {''',
    "bearer token file source hardening tests",
)

replace_once(
    "README.md",
    '''The variables are mutually exclusive. The file form supports mounted container secrets and accepts the token with no terminator, one LF, or one CRLF; no other whitespace is trimmed.''',
    '''The variables are mutually exclusive. The file form supports regular files and symlinked mounted secrets, accepts the token with no terminator, one LF, or one CRLF, and enforces a 16 KiB token limit; no other whitespace is trimmed.''',
    "bearer token file source documentation",
)

replace_once(
    "README.md",
    '''Do not set both variables. Token files may end in one LF or CRLF, but additional or embedded whitespace is rejected.''',
    '''Do not set both variables. Token files must resolve to regular files, may end in one LF or CRLF, and may contain at most 16 KiB before that optional terminator. Additional or embedded whitespace is rejected.''',
    "bearer token file format documentation",
)

replace_once(
    "README.md",
    '''| `GIT_AUTOCOMMIT_BEARER_TOKEN must...` | The direct token is empty, contains whitespace or non-ASCII characters, is not UTF-8, or cannot form a valid HTTP header value. |
| `unable to read GIT_AUTOCOMMIT_BEARER_TOKEN_FILE` | The configured credential file is missing, unreadable, or not a regular readable secret source. |
| `GIT_AUTOCOMMIT_BEARER_TOKEN_FILE must...` | The file path is empty, or its content is empty, invalid UTF-8, contains unsupported whitespace or non-ASCII characters, or cannot form a valid HTTP header value. |''',
    '''| `GIT_AUTOCOMMIT_BEARER_TOKEN must...` | The direct token is empty, exceeds 16 KiB, contains whitespace or non-ASCII characters, is not UTF-8, or cannot form a valid HTTP header value. |
| `unable to read GIT_AUTOCOMMIT_BEARER_TOKEN_FILE` | The configured credential file is missing or unreadable. |
| `GIT_AUTOCOMMIT_BEARER_TOKEN_FILE must...` | The file path is empty, does not resolve to a regular file, or its content is empty, exceeds 16 KiB, is invalid UTF-8, contains unsupported whitespace or non-ASCII characters, or cannot form a valid HTTP header value. |''',
    "bearer token file hardening troubleshooting",
)
