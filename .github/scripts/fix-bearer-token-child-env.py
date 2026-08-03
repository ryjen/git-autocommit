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
    '''fn run_git_raw(
    root: Option<&Path>,
    args: &[&str],
    extra_env: Option<&[(&str, OsString)]>,
) -> Result<Output> {
    let mut command = Command::new("git");''',
    '''fn git_command() -> Command {
    let mut command = Command::new("git");
    command.env_remove(BEARER_TOKEN_ENV);
    command
}

fn run_git_raw(
    root: Option<&Path>,
    args: &[&str],
    extra_env: Option<&[(&str, OsString)]>,
) -> Result<Output> {
    let mut command = git_command();''',
    "bearer token subprocess scrubbing",
)

replace_once(
    "src/app.rs",
    '''    #[test]
    fn bearer_token_is_redacted_in_debug_and_json() {''',
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
    "subprocess environment unit test",
)

replace_once(
    "README.md",
    '''Authenticated endpoints can use `GIT_AUTOCOMMIT_BEARER_TOKEN`. The token is accepted only from the environment, sent as a sensitive `Authorization: Bearer` header, and rendered as `<redacted>` by `--show-config`. It is not accepted through CLI arguments, TOML, or URL user information. Avoid placing secrets directly in shell history.''',
    '''Authenticated endpoints can use `GIT_AUTOCOMMIT_BEARER_TOKEN`. The token is accepted only from the environment, sent as a sensitive `Authorization: Bearer` header, and rendered as `<redacted>` by `--show-config`. It is not accepted through CLI arguments, TOML, or URL user information, and it is removed from child Git and signing-process environments. Avoid placing secrets directly in shell history.''',
    "subprocess secret boundary documentation",
)
