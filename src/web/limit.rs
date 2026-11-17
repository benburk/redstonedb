//! A limit on how fast one address may fetch pages with a query: filtered,
//! sorted, and searched lists, each of which reads every build. A person
//! choosing filters stays well inside it; a crawler walking every
//! combination of them is slowed to the rate it refills.

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{FromRequestParts, Request},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::web::extractors::client_ip::ClientIp;

/// How many such pages an address may fetch at once before it waits.
const BURST: f64 = 60.0;
/// How often it may fetch one more, once its burst is spent.
const EVERY: Duration = Duration::from_secs(1);
/// More addresses than this, and those whose buckets have refilled are
/// forgotten.
const REMEMBERED: usize = 10_000;

/// An address's allowance: what it has left, as of when.
#[derive(Clone, Copy)]
struct Bucket {
    left: f64,
    at: Instant,
}

/// Each address's allowance.
#[derive(Default)]
struct Buckets(HashMap<String, Bucket>);

impl Buckets {
    /// Takes one from `ip`'s allowance at `now`, or else says how long
    /// until it has one.
    fn take(&mut self, ip: &str, now: Instant) -> Result<(), Duration> {
        if self.0.len() >= REMEMBERED {
            self.0.retain(|_, b| refilled(*b, now) < BURST);
        }
        let bucket = self.0.entry(ip.to_owned()).or_insert(Bucket {
            left: BURST,
            at: now,
        });
        let left = refilled(*bucket, now);
        if left >= 1.0 {
            *bucket = Bucket {
                left: left - 1.0,
                at: now,
            };
            Ok(())
        } else {
            Err(EVERY.mul_f64(1.0 - left))
        }
    }
}

/// What `bucket` holds at `now`, refilled since it was last taken from.
fn refilled(bucket: Bucket, now: Instant) -> f64 {
    let since = now.saturating_duration_since(bucket.at);
    (bucket.left + since.as_secs_f64() / EVERY.as_secs_f64()).min(BURST)
}

static BUCKETS: LazyLock<Mutex<Buckets>> = LazyLock::new(Mutex::default);

/// Answers 429 to an address past its allowance of pages with a query.
/// Requests that came another way than through the proxy, with no
/// address, aren't limited.
pub(crate) async fn queries(request: Request, next: Next) -> Response {
    if request.uri().query().is_none_or(str::is_empty) {
        return next.run(request).await;
    }
    let (mut parts, body) = request.into_parts();
    let Ok(ClientIp(ip)) = ClientIp::from_request_parts(&mut parts, &()).await;
    let request = Request::from_parts(parts, body);
    let Some(ip) = ip else {
        return next.run(request).await;
    };
    let taken = BUCKETS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take(&ip, Instant::now());
    match taken {
        Ok(()) => next.run(request).await,
        Err(wait) => (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, wait.as_secs().max(1).to_string())],
            "Too many requests. Wait a moment and try again.",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An address spends its burst, then gets one more each `EVERY`; an
    /// address of its own is unaffected.
    #[test]
    fn an_address_waits_once_its_burst_is_spent() {
        let mut buckets = Buckets::default();
        let start = Instant::now();
        for _ in 0..60 {
            assert!(buckets.take("203.0.113.9", start).is_ok());
        }
        assert_eq!(buckets.take("203.0.113.9", start), Err(EVERY));
        assert!(buckets.take("198.51.100.1", start).is_ok());
        let later = start + EVERY;
        assert!(buckets.take("203.0.113.9", later).is_ok());
        assert!(buckets.take("203.0.113.9", later).is_err());
    }
}
