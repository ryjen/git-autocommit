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
    '''        if value.bytes().any(|byte| byte.is_ascii_whitespace()) {
            bail!("{BEARER_TOKEN_ENV} must not contain whitespace");
        }
        let header_text = format!("Bearer {value}");''',
    '''        if value.bytes().any(|byte| byte.is_ascii_whitespace()) {
            bail!("{BEARER_TOKEN_ENV} must not contain whitespace");
        }
        if !value.is_ascii() {
            bail!("{BEARER_TOKEN_ENV} must contain only ASCII characters");
        }
        let header_text = format!("Bearer {value}");''',
    "ASCII bearer token validation",
)

replace_once(
    "README.md",
    '''| `GIT_AUTOCOMMIT_BEARER_TOKEN must...` | The configured token is empty, contains whitespace, is not UTF-8, or cannot form a valid HTTP header value. |''',
    '''| `GIT_AUTOCOMMIT_BEARER_TOKEN must...` | The configured token is empty, contains whitespace or non-ASCII characters, is not UTF-8, or cannot form a valid HTTP header value. |''',
    "ASCII token troubleshooting",
)
