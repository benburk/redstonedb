use std::io::IsTerminal as _;
use std::net::SocketAddr;

use app::db::connect_to_database;
use app::external::discord;
use app::web;
use app::web::app_state::AppState;
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderValue, Request, header};
use tokio::net::TcpListener;
use tower_http::{
    set_header::SetResponseHeaderLayer,
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use tracing::field;
use tracing::info;
use tracing_subscriber::EnvFilter;

fn configure_logging() {
    tracing_subscriber::fmt()
        // Colour for a terminal only, not for the journal.
        .with_ansi(std::io::stdout().is_terminal())
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .or_else(|_| {
                    EnvFilter::try_new(
                        "app=info,redstonedb=info,tower_http=info",
                    )
                })
                .unwrap(),
        )
        .init();
}

/// Read the required server port from `PORT`.
fn port() -> u16 {
    let Ok(value) = std::env::var("PORT") else {
        eprintln!("no PORT. `just serve` picks a free one with free-port.");
        std::process::exit(2);
    };
    value.parse().unwrap_or_else(|_| {
        eprintln!("PORT is `{value}`, which is not a port number.");
        std::process::exit(2);
    })
}

#[tokio::main]
async fn main() {
    configure_logging();

    let db = connect_to_database().await;
    // Builds not yet compared with their category's others are, in the
    // background.
    app::models::similarity::refresh_soon(&db);
    // Every player's head the site shows, fetched ahead of the first view.
    app::web::fetch_heads_ahead(&db);

    let http = reqwest::Client::new();
    let state = AppState {
        db,
        discord: discord::Config::from_env(),
        renderer: std::sync::Arc::new(app::thumbnail::Renderer::new(
            app::thumbnail::Assets::new(http.clone()),
        )),
        http,
        thumbnails: std::sync::Arc::default(),
        analyses: std::sync::Arc::default(),
    };

    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|request: &Request<Body>| {
            let request_id = uuid::Uuid::new_v4();
            // The path only: a query can carry a one-time login code.
            tracing::info_span!(
                "request",
                %request_id,
                method = %request.method(),
                path = %request.uri().path(),
                user_id = field::Empty,
            )
        })
        .on_request(DefaultOnRequest::new().level(Level::DEBUG))
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    let app = Router::new()
        .merge(web::static_router())
        // Pages show live data, so browsers fetch them afresh, even on Back;
        // a handler that knows better, such as thumbnails, says so itself.
        .merge(
            web::router(state.clone())
                .layer(trace_layer)
                .layer(header_layer(header::CACHE_CONTROL, "no-store")),
        )
        // No framing by other sites, no guessing at file types, and no
        // page addresses sent to other sites.
        .layer(header_layer(header::X_FRAME_OPTIONS, "DENY"))
        .layer(header_layer(header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
        .layer(header_layer(header::REFERRER_POLICY, "same-origin"))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port()));
    let listener = TcpListener::bind(addr).await.expect("Failed to bind");

    info!("Serving on http://{addr}");

    axum::serve(listener, app).await.expect("Failed to serve");
}

fn header_layer(
    name: header::HeaderName,
    value: &'static str,
) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(
        name,
        HeaderValue::from_static(value),
    )
}
