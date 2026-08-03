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
    "use reqwest::{StatusCode, blocking::Client, redirect::Policy};",
    "use reqwest::{StatusCode, Url, blocking::Client, redirect::Policy};",
    "reqwest URL import",
)

replace_once(
    "src/app.rs",
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
    let client = Client::builder()''',
    '''fn model_request_url(base_url: &str) -> Result<Url> {
    let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let url = Url::parse(&endpoint).context("invalid local AI base_url")?;
    match url.scheme() {
        "https" => {}
        "http" => {
            let host = url
                .host_str()
                .ok_or_else(|| anyhow!("local AI base_url is missing a host"))?;
            let address_host = host
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
                .unwrap_or(host);
            let is_loopback = host.eq_ignore_ascii_case("localhost")
                || address_host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback());
            if !is_loopback {
                bail!(
                    "plaintext HTTP model endpoints are allowed only on loopback; use HTTPS for non-loopback base_url"
                );
            }
        }
        scheme => bail!("local AI base_url must use http or https, not {scheme}"),
    }
    Ok(url)
}

fn reject_redirect(status: StatusCode) -> Result<()> {
    if status.is_redirection() {
        bail!(
            "local AI endpoint returned HTTP redirect {status}; redirects are disabled to prevent forwarding staged repository content"
        );
    }
    Ok(())
}

fn request_plan(settings: &Settings, system: &str, user: &str) -> Result<String> {
    validate_prompt_size(system, user, settings.max_prompt_bytes)?;
    let request_url = model_request_url(&settings.base_url)?;
    let client = Client::builder()''',
    "model endpoint transport validation",
)

replace_once(
    "src/app.rs",
    '''    let response = client
        .post(format!(
            "{}/chat/completions",
            settings.base_url.trim_end_matches('/')
        ))''',
    '''    let response = client
        .post(request_url)''',
    "validated request URL",
)

replace_once(
    "src/app.rs",
    '''    #[test]
    fn request_does_not_follow_http_redirects() {''',
    '''    #[test]
    fn allows_https_and_loopback_http_model_endpoints() {
        for base_url in [
            "https://example.com/v1",
            "http://localhost:8000/v1",
            "http://LOCALHOST:8000/v1",
            "http://127.0.0.1:8000/v1",
            "http://127.42.0.9:8000/v1",
            "http://[::1]:8000/v1",
        ] {
            model_request_url(base_url).unwrap();
        }
    }

    #[test]
    fn rejects_plaintext_http_outside_loopback() {
        for base_url in [
            "http://example.com/v1",
            "http://192.168.1.10:8000/v1",
            "http://0.0.0.0:8000/v1",
            "http://localhost.example:8000/v1",
            "http://localhost.:8000/v1",
        ] {
            let error = model_request_url(base_url).unwrap_err();
            assert!(error.to_string().contains("allowed only on loopback"));
        }
    }

    #[test]
    fn rejects_invalid_or_unsupported_model_endpoint_urls() {
        let invalid = model_request_url("not a URL").unwrap_err();
        assert!(invalid.to_string().contains("invalid local AI base_url"));

        let unsupported = model_request_url("ftp://127.0.0.1/v1").unwrap_err();
        assert!(unsupported.to_string().contains("must use http or https"));
    }

    #[test]
    fn request_rejects_non_loopback_http_before_connecting() {
        let mut settings = settings_for(&[], FileConfig::default());
        settings.base_url = "http://0.0.0.0:9/v1".to_owned();
        settings.timeout_seconds = 0.1;
        let error = request_plan(&settings, "system", "user").unwrap_err();
        assert!(error.to_string().contains("allowed only on loopback"));
    }

    #[test]
    fn request_does_not_follow_http_redirects() {''',
    "transport boundary tests",
)

replace_once(
    "README.md",
    '''### Response handling

System and environment proxy settings are disabled for model requests.''',
    '''### Response handling

HTTPS is required for every non-loopback model endpoint. Plaintext HTTP is accepted only for the exact hostname `localhost` or a literal loopback IP address such as `127.0.0.1` or `::1`; private-network addresses and alternate hostnames are rejected before a connection is attempted.

System and environment proxy settings are disabled for model requests.''',
    "HTTP transport documentation",
)

replace_once(
    "README.md",
    '''| `local AI returned an error` | The endpoint returned a non-success HTTP status. |
| `local AI endpoint returned HTTP redirect...` | `base_url` points to a redirecting URL; configure the final endpoint directly. |''',
    '''| `local AI returned an error` | The endpoint returned a non-success HTTP status. |
| `plaintext HTTP model endpoints are allowed only on loopback...` | A non-loopback `base_url` uses HTTP; configure HTTPS or use an exact loopback endpoint for local development. |
| `local AI endpoint returned HTTP redirect...` | `base_url` points to a redirecting URL; configure the final endpoint directly. |''',
    "HTTP transport troubleshooting",
)
