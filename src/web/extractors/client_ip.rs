use std::convert::Infallible;

use axum::{extract::FromRequestParts, http::request::Parts};

/// The address the request came from, as the proxy in front of the site
/// saw it: the last entry of `X-Forwarded-For`, the one the proxy adds,
/// as a client can send any entries before it. `None` when the request
/// came another way, as one to the site's own port does.
pub struct ClientIp(pub Option<String>);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        let ip = parts
            .headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .next_back()
            .and_then(|ip| ip.trim().parse::<std::net::IpAddr>().ok())
            .map(|ip| ip.to_string());
        Ok(ClientIp(ip))
    }
}

#[cfg(test)]
mod tests {
    use axum::http::Request;

    use super::*;

    async fn ip(forwarded: &[&str]) -> Option<String> {
        let mut request = Request::builder();
        for value in forwarded {
            request = request.header("x-forwarded-for", *value);
        }
        let (mut parts, ()) = request.body(()).unwrap().into_parts();
        let ClientIp(ip) =
            ClientIp::from_request_parts(&mut parts, &()).await.unwrap();
        ip
    }

    /// The proxy's entry is the last; any a client sent come before it.
    #[tokio::test]
    async fn takes_the_entry_the_proxy_adds() {
        assert_eq!(ip(&["203.0.113.9"]).await.as_deref(), Some("203.0.113.9"));
        assert_eq!(
            ip(&["1.2.3.4, 203.0.113.9"]).await.as_deref(),
            Some("203.0.113.9")
        );
        assert_eq!(
            ip(&["1.2.3.4", "2001:db8::1"]).await.as_deref(),
            Some("2001:db8::1")
        );
        assert_eq!(ip(&["not an address"]).await, None);
        assert_eq!(ip(&[]).await, None);
    }
}
