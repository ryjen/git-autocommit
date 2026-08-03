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
}''',
    '''fn model_request_url(base_url: &str) -> Result<Url> {
    let mut url = Url::parse(base_url).context("invalid local AI base_url")?;
    if !url.username().is_empty() || url.password().is_some() {
        bail!("local AI base_url must not include embedded credentials");
    }
    if url.query().is_some() {
        bail!("local AI base_url must not include a query string");
    }
    if url.fragment().is_some() {
        bail!("local AI base_url must not include a fragment");
    }

    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("local AI base_url is missing a host"))?;
    match url.scheme() {
        "https" => {}
        "http" => {
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

    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| anyhow!("local AI base_url cannot be extended with a request path"))?;
        segments.pop_if_empty();
        segments.push("chat");
        segments.push("completions");
    }
    Ok(url)
}''',
    "base URL parsing and path construction",
)

replace_once(
    "src/app.rs",
    '''    #[test]
    fn rejects_plaintext_http_outside_loopback() {''',
    '''    #[test]
    fn appends_chat_completions_as_url_path_segments() {
        for (base_url, expected) in [
            (
                "https://example.com",
                "https://example.com/chat/completions",
            ),
            (
                "https://example.com/",
                "https://example.com/chat/completions",
            ),
            (
                "https://example.com/v1",
                "https://example.com/v1/chat/completions",
            ),
            (
                "https://example.com/v1/",
                "https://example.com/v1/chat/completions",
            ),
            (
                "https://example.com/api%2Fv1",
                "https://example.com/api%2Fv1/chat/completions",
            ),
            (
                "http://[::1]:8000/v1/",
                "http://[::1]:8000/v1/chat/completions",
            ),
        ] {
            assert_eq!(model_request_url(base_url).unwrap().as_str(), expected);
        }
    }

    #[test]
    fn rejects_credentials_queries_and_fragments() {
        for (base_url, expected) in [
            (
                "https://user@example.com/v1",
                "must not include embedded credentials",
            ),
            (
                "https://:secret@example.com/v1",
                "must not include embedded credentials",
            ),
            (
                "https://example.com/v1?model=test",
                "must not include a query string",
            ),
            (
                "https://example.com/v1?",
                "must not include a query string",
            ),
            (
                "https://example.com/v1#section",
                "must not include a fragment",
            ),
            (
                "https://example.com/v1#",
                "must not include a fragment",
            ),
        ] {
            let error = model_request_url(base_url).unwrap_err();
            assert!(
                error.to_string().contains(expected),
                "unexpected error for {base_url}: {error:#}"
            );
        }
    }

    #[test]
    fn rejects_plaintext_http_outside_loopback() {''',
    "base URL shape tests",
)

replace_once(
    "src/app.rs",
    '''    #[test]
    fn request_rejects_non_loopback_http_before_connecting() {''',
    '''    #[test]
    fn request_rejects_ambiguous_base_url_before_connecting() {
        let mut settings = settings_for(&[], FileConfig::default());
        settings.base_url = "http://127.0.0.1:9/v1?target=other".to_owned();
        settings.timeout_seconds = 0.1;
        let error = request_plan(&settings, "system", "user").unwrap_err();
        assert!(error.to_string().contains("must not include a query string"));
    }

    #[test]
    fn request_rejects_non_loopback_http_before_connecting() {''',
    "pre-connection ambiguous URL test",
)

replace_once(
    "README.md",
    '''HTTPS is required for every non-loopback model endpoint. Plaintext HTTP is accepted only for the exact hostname `localhost` or a literal loopback IP address such as `127.0.0.1` or `::1`; private-network addresses and alternate hostnames are rejected before a connection is attempted.

System and environment proxy settings are disabled for model requests.''',
    '''HTTPS is required for every non-loopback model endpoint. Plaintext HTTP is accepted only for the exact hostname `localhost` or a literal loopback IP address such as `127.0.0.1` or `::1`; private-network addresses and alternate hostnames are rejected before a connection is attempted.

`base_url` must contain only the endpoint origin and optional API path. Embedded credentials, query parameters, and fragments are rejected before connecting. The client appends `chat/completions` as URL path segments rather than through string concatenation.

System and environment proxy settings are disabled for model requests.''',
    "base URL shape documentation",
)

replace_once(
    "README.md",
    '''| `plaintext HTTP model endpoints are allowed only on loopback...` | A non-loopback `base_url` uses HTTP; configure HTTPS or use an exact loopback endpoint for local development. |
| `local AI endpoint returned HTTP redirect...` | `base_url` points to a redirecting URL; configure the final endpoint directly. |''',
    '''| `plaintext HTTP model endpoints are allowed only on loopback...` | A non-loopback `base_url` uses HTTP; configure HTTPS or use an exact loopback endpoint for local development. |
| `local AI base_url must not include...` | `base_url` contains embedded credentials, a query string, or a fragment; move authentication to the supported request mechanism and configure only the endpoint origin/path. |
| `local AI endpoint returned HTTP redirect...` | `base_url` points to a redirecting URL; configure the final endpoint directly. |''',
    "base URL troubleshooting",
)
