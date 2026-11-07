//! Every assistant request on desktop passes through here (docs/44, docs/62). The webview names a
//! URL. Rust decides where it may go, adds the local server's key itself, and streams bytes back.
//! So the webview never holds the local server's port or key, and its CSP has no outside address.
use crate::error::CnError;
use std::time::Duration;
use tokio::sync::Notify;
use url::Url;

/// The address the assistant is pointed at for the local model. `.invalid` is reserved, so it can
/// never resolve: if this layer were missing, the request would simply fail.
pub const LOCAL_ENDPOINT: &str = "https://this-computer.invalid/v1";
pub const LOCAL_HOST: &str = "this-computer.invalid";
const LOCAL_PATH: &str = "/v1/chat/completions";
pub const MAX_BODY: usize = 32 << 20;
const MAX_REPLY: u64 = 64 << 20;
pub const REDIRECT_REFUSED: &str = "CN-DSK-0018";
const BOUND_HEADERS: [&str; 5] = [
    "content-type",
    "authorization",
    "x-api-key",
    "anthropic-version",
    "anthropic-dangerous-direct-browser-access",
];

#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    /// The local model this shell supervises.
    Local,
    /// An origin a person confirmed.
    Bound(Url),
    /// Nothing is sent until a person confirms this origin.
    NeedsBinding(String, Url),
}

/// `scheme://host[:port]`, in ASCII form: a non-ASCII host shows as punycode. Refuses what could
/// disguise a destination. Plain http is for this computer only.
pub fn origin_of(raw: &str) -> Result<(String, Url), CnError> {
    let refused = |why: &str| CnError::new(REDIRECT_REFUSED, why);
    let mut url = Url::parse(raw).map_err(|_| refused("That is not a valid address."))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(refused("An address cannot carry a name or password."));
    }
    let host = url
        .host_str()
        .ok_or_else(|| refused("That address has no host."))?
        .to_ascii_lowercase();
    if host == "localhost" {
        url.set_host(Some("127.0.0.1"))
            .map_err(|_| refused("That address could not be read."))?;
    }
    let loopback = matches!(url.host_str(), Some("127.0.0.1") | Some("[::1]"));
    match url.scheme() {
        "https" => {}
        "http" if loopback => {}
        _ => {
            return Err(refused(
                "This address must use https. Plain http works only on this computer.",
            ));
        }
    }
    Ok((url.origin().ascii_serialization(), url))
}

pub fn classify(raw: &str, bindings: &[String]) -> Result<Route, CnError> {
    let (origin, url) = origin_of(raw)?;
    if url.host_str() == Some(LOCAL_HOST) {
        return if url.path() == LOCAL_PATH && url.query().is_none() {
            Ok(Route::Local)
        } else {
            Err(CnError::new(
                REDIRECT_REFUSED,
                "The local model only answers chat requests.",
            ))
        };
    }
    Ok(if bindings.contains(&origin) {
        Route::Bound(url)
    } else {
        Route::NeedsBinding(origin, url)
    })
}

/// Which request headers reach a bound provider. Anything else from the webview is dropped.
pub fn bound_headers(from_webview: &[(String, String)]) -> Vec<(String, String)> {
    from_webview
        .iter()
        .filter(|(k, _)| BOUND_HEADERS.contains(&k.to_ascii_lowercase().as_str()))
        .cloned()
        .collect()
}

/// For the local model: the key is the shell's, and any Authorization from the webview is ignored.
pub fn local_request(base: &str, key: &str) -> (String, Vec<(String, String)>) {
    (
        format!("{base}{LOCAL_PATH}"),
        vec![
            ("content-type".into(), "application/json".into()),
            ("authorization".into(), format!("Bearer {key}")),
        ],
    )
}

#[derive(Debug, PartialEq, Eq)]
pub enum Frame {
    Head {
        status: u16,
        content_type: String,
        retry_after: Option<String>,
    },
    Chunk(Vec<u8>),
}

/// One request, streamed. No redirect is followed, since a request may carry a key (CN-DSK-0018).
/// Closing on `cancel` drops the connection, which is how a local server learns to stop.
pub async fn forward(
    client: &reqwest::Client,
    url: &str,
    headers: &[(String, String)],
    body: String,
    cancel: &Notify,
    mut sink: impl FnMut(Frame),
) -> Result<(), CnError> {
    let lost = || CnError::new("CN-AI-0005", "The connection failed.");
    let mut request = client
        .post(url)
        .body(body)
        .timeout(Duration::from_secs(3600));
    for (k, v) in headers {
        request = request.header(k, v);
    }
    let mut response = tokio::select! {
        r = request.send() => r.map_err(|_| lost())?,
        () = cancel.notified() => return Ok(()),
    };
    if response.status().is_redirection() {
        return Err(CnError::new(
            REDIRECT_REFUSED,
            "The provider tried to redirect this request, so it was stopped. Nothing else was sent.",
        ));
    }
    let text = |name: &str, r: &reqwest::Response| {
        r.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    sink(Frame::Head {
        status: response.status().as_u16(),
        content_type: text("content-type", &response).unwrap_or_default(),
        retry_after: text("retry-after", &response),
    });
    let mut total = 0;
    loop {
        let chunk = tokio::select! {
            c = response.chunk() => c.map_err(|_| lost())?,
            () = cancel.notified() => return Ok(()),
        };
        let Some(chunk) = chunk else { return Ok(()) };
        total += chunk.len() as u64;
        if total > MAX_REPLY {
            return Err(CnError::new(
                "CN-AI-0008",
                "The reply was larger than this app accepts.",
            ));
        }
        sink(Frame::Chunk(chunk.to_vec()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_goes_to_an_unconfirmed_origin() {
        let bound = vec!["https://api.openai.com".to_owned()];
        assert!(matches!(
            classify("https://api.openai.com/v1/chat/completions", &bound).unwrap(),
            Route::Bound(_)
        ));
        let Route::NeedsBinding(origin, _) =
            classify("https://api.openai.com.evil.example/v1", &bound).unwrap()
        else {
            panic!()
        };
        assert_eq!(origin, "https://api.openai.com.evil.example");
        assert!(matches!(
            classify("http://127.0.0.1:8080/v1", &bound).unwrap(),
            Route::NeedsBinding(..)
        ));
        assert!(matches!(
            classify("https://api.openai.com:8443/v1", &bound).unwrap(),
            Route::NeedsBinding(..)
        ));
    }

    #[test]
    fn what_a_confirmation_shows_cannot_be_dressed_up() {
        assert!(origin_of("http://evil.example/v1").is_err());
        assert!(origin_of("https://api.openai.com@evil.example/v1").is_err());
        assert!(origin_of("file:///etc/passwd").is_err());
        assert_eq!(
            origin_of("https://ｅxample.com/x").unwrap().0,
            "https://example.com"
        );
        assert_eq!(
            origin_of("https://bücher.example/x").unwrap().0,
            "https://xn--bcher-kva.example"
        );
        assert_eq!(
            origin_of("http://localhost:11434/v1").unwrap().0,
            "http://127.0.0.1:11434"
        );
    }

    #[test]
    fn the_local_route_is_chat_only_and_uses_the_shells_key() {
        let url = format!("{LOCAL_ENDPOINT}/chat/completions");
        assert_eq!(classify(&url, &[]).unwrap(), Route::Local);
        assert!(classify(&format!("{LOCAL_ENDPOINT}/models"), &[]).is_err());
        assert!(classify(&format!("{url}?x=1"), &[]).is_err());
        let (target, headers) = local_request("http://127.0.0.1:9", "shell-key");
        assert_eq!(target, "http://127.0.0.1:9/v1/chat/completions");
        assert!(headers.contains(&("authorization".to_owned(), "Bearer shell-key".to_owned())));
    }

    #[test]
    fn only_known_headers_reach_a_provider() {
        let kept = bound_headers(&[
            ("Authorization".into(), "Bearer k".into()),
            ("Host".into(), "evil".into()),
            ("cookie".into(), "c".into()),
            ("content-type".into(), "application/json".into()),
        ]);
        assert_eq!(kept.len(), 2);
    }
}
