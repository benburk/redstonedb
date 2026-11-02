use std::convert::Infallible;

use axum::{extract::FromRequestParts, http::request::Parts};

/// Whether fixi sent the request, as its `FX-Request` header says: it
/// swaps the answer into the page, so it wants a fragment, where a plain
/// form wants a page or a redirect.
pub struct Fixi(pub bool);

impl<S: Send + Sync> FromRequestParts<S> for Fixi {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Fixi(parts.headers.contains_key("fx-request")))
    }
}
