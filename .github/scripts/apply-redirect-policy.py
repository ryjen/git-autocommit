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
    "use reqwest::blocking::Client;",
    "use reqwest::{StatusCode, blocking::Client, redirect::Policy};",
    "reqwest imports",
)

replace_once(
    "src/app.rs",
    '''fn request_plan(settings: &Settings, system: &str, user: &str) -> Result<String> {
    validate_prompt_size(system, user, settings.max_prompt_bytes)?;
    let client = Client::builder()
        .timeout(Duration::from_secs_f64(settings.timeout_seconds))
        .build()?;''',
    '''fn reject_redirect(status: StatusCode) -> Result<()> {
    if status.is_redirection() {
        bail!(
            "local AI endpoint returned HTTP redirect {status}; redirects are disabled to prevent forwarding staged repository content"
        );
    }
    Ok(())
}

fn request_plan(settings: &Settings, system: &str, user: &str) -> Result<String> {
    validate_prompt_size(system, user, settings.max_prompt_bytes)?;
    let client = Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs_f64(settings.timeout_seconds))
        .build()?;''',
    "redirect policy and validation",
)

replace_once(
    "src/app.rs",
    '''        .send()
        .context("local AI unavailable")?
        .error_for_status()
        .context("local AI returned an error")?;
    validate_response_content_length(response.content_length(), MAX_AI_RESPONSE_BYTES)?;''',
    '''        .send()
        .context("local AI unavailable")?;
    reject_redirect(response.status())?;
    let response = response
        .error_for_status()
        .context("local AI returned an error")?;
    validate_response_content_length(response.content_length(), MAX_AI_RESPONSE_BYTES)?;''',
    "redirect response rejection",
)

replace_once(
    "src/app.rs",
    '''    #[test]
    fn index_lock_allows_validation_blocks_writes_and_cleans_up() {''',
    '''    #[test]
    fn request_does_not_follow_http_redirects() {
        use std::io::ErrorKind;
        use std::net::TcpListener;
        use std::sync::mpsc;
        use std::thread;

        let target = TcpListener::bind("127.0.0.1:0").unwrap();
        target.set_nonblocking(true).unwrap();
        let target_addr = target.local_addr().unwrap();
        let redirect = TcpListener::bind("127.0.0.1:0").unwrap();
        let redirect_addr = redirect.local_addr().unwrap();
        let (served_tx, served_rx) = mpsc::channel();

        let server = thread::spawn(move || {
            let (mut stream, _) = redirect.accept().unwrap();
            let mut request = [0_u8; 4_096];
            std::io::Read::read(&mut stream, &mut request).unwrap();
            let response = format!(
                "HTTP/1.1 307 Temporary Redirect\\r\\nLocation: http://{target_addr}/capture\\r\\nContent-Length: 0\\r\\nConnection: close\\r\\n\\r\\n"
            );
            std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
            served_tx.send(()).unwrap();
        });

        let mut settings = settings_for(&[], FileConfig::default());
        settings.base_url = format!("http://{redirect_addr}");
        settings.timeout_seconds = 2.0;
        let error = request_plan(&settings, "system", "user").unwrap_err();
        assert!(error.to_string().contains("redirects are disabled"));
        served_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        server.join().unwrap();
        thread::sleep(Duration::from_millis(50));

        match target.accept() {
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Ok(_) => panic!("redirect target unexpectedly received the staged prompt request"),
            Err(error) => panic!("unexpected redirect target error: {error}"),
        }
    }

    #[test]
    fn index_lock_allows_validation_blocks_writes_and_cleans_up() {''',
    "redirect integration test",
)

replace_once(
    "README.md",
    '''### Response handling

The HTTP response body is capped at 256 KiB before JSON deserialization. A declared `Content-Length` above the limit is rejected before reading the body. Responses without a trustworthy length, including chunked responses, are streamed only through the limit plus one byte and rejected if oversized.''',
    '''### Response handling

HTTP redirects are disabled. Any 3xx response is rejected rather than forwarding staged repository content to another URL; configure `base_url` to the final endpoint directly.

The HTTP response body is capped at 256 KiB before JSON deserialization. A declared `Content-Length` above the limit is rejected before reading the body. Responses without a trustworthy length, including chunked responses, are streamed only through the limit plus one byte and rejected if oversized.''',
    "redirect response documentation",
)

replace_once(
    "README.md",
    '''| `local AI returned an error` | The endpoint returned a non-success HTTP status. |
| `rendered prompt is ... exceeding the ...-byte limit` | Expanded prompt text, including metadata and custom prompts, exceeds `max_prompt_bytes`. |''',
    '''| `local AI returned an error` | The endpoint returned a non-success HTTP status. |
| `local AI endpoint returned HTTP redirect...` | `base_url` points to a redirecting URL; configure the final endpoint directly. |
| `rendered prompt is ... exceeding the ...-byte limit` | Expanded prompt text, including metadata and custom prompts, exceeds `max_prompt_bytes`. |''',
    "redirect troubleshooting",
)
