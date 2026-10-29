//! Web routes.
use std::fmt::Write as _;

use axum::{
    Router,
    extract::{OptionalFromRequestParts, Request, State},
    http::header,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};

use crate::{
    app_state::AppState,
    error::{AppError, ErrorPage},
    models::user::User,
};

mod about;
mod auth;
#[cfg(debug_assertions)]
mod dev;
mod form;
mod heads;
mod leaderboards;
mod pages;
mod players;
mod profile;
mod schemati;
mod schematics;
mod session;
mod settings;
mod similar;
mod simulation;
mod upload;
mod users;
mod versions;

use about::about;
use profile::profile;

/// A file under `static/`, built into the binary and served at `path`.
/// Pages link it at [`Asset::url`], which names its version, so browsers
/// may keep it for good.
#[derive(Clone, Copy)]
pub struct Asset {
    path: &'static str,
    content_type: &'static str,
    contents: &'static [u8],
    /// A hash of the contents, so a changed file gets a new address.
    version: u64,
}

impl Asset {
    const fn new(
        path: &'static str,
        content_type: &'static str,
        contents: &'static [u8],
    ) -> Self {
        // FNV-1a, which a constant can compute.
        let mut version: u64 = 0xcbf2_9ce4_8422_2325;
        let mut i = 0;
        while i < contents.len() {
            version ^= contents[i] as u64;
            version = version.wrapping_mul(0x0100_0000_01b3);
            i += 1;
        }
        Asset {
            path,
            content_type,
            contents,
            version,
        }
    }

    /// Where pages link it.
    pub fn url(self) -> String {
        format!("{}?v={:016x}", self.path, self.version)
    }
}

/// The stylesheet, built from `styles/app.css` by `just css`.
pub const CSS: Asset = Asset::new(
    "/app.css",
    "text/css",
    include_bytes!("../../static/app.css"),
);
/// fixi, which every page loads.
pub const FIXI_JS: Asset = Asset::new(
    "/fixi.js",
    "text/javascript",
    include_bytes!("../../static/fixi.js"),
);
/// fixi's policies for every page.
pub const APP_JS: Asset = Asset::new(
    "/app.js",
    "text/javascript",
    include_bytes!("../../static/app.js"),
);
/// The details forms' script.
pub const FORM_JS: Asset = Asset::new(
    "/form.js",
    "text/javascript",
    include_bytes!("../../static/form.js"),
);
/// Keeps the sidebar scrolled where it was from page to page.
pub const SIDEBAR_JS: Asset = Asset::new(
    "/sidebar.js",
    "text/javascript",
    include_bytes!("../../static/sidebar.js"),
);
/// Applies the theme chosen on the settings page before a page is drawn.
pub const THEME_JS: Asset = Asset::new(
    "/theme.js",
    "text/javascript",
    include_bytes!("../../static/theme.js"),
);
/// The 3D view of a build.
pub const VIEWER_JS: Asset = Asset::new(
    "/viewer.js",
    "text/javascript",
    include_bytes!("../../static/viewer.js"),
);

const ASSETS: [Asset; 7] = [
    CSS, FIXI_JS, APP_JS, FORM_JS, SIDEBAR_JS, THEME_JS, VIEWER_JS,
];

/// A Before you upload picture, drawn by `rsdb guide-images static/guide`.
macro_rules! guide_image {
    ($name:literal) => {
        Asset::new(
            concat!("/guide/", $name, ".webp"),
            "image/webp",
            include_bytes!(concat!("../../static/guide/", $name, ".webp")),
        )
    };
}

/// The Before you upload pictures, one for each of [`crate::guide::examples`].
const GUIDE: [Asset; 10] = [
    guide_image!("input"),
    guide_image!("input-three-dusts"),
    guide_image!("repeater"),
    guide_image!("output"),
    guide_image!("outputs-touching"),
    guide_image!("bus-straight"),
    guide_image!("bus-diagonal"),
    guide_image!("bus-alternating"),
    guide_image!("bus-uneven"),
    guide_image!("gray"),
];

/// Where pages link the Before you upload picture of example `name`.
pub fn guide_image(name: &str) -> Option<String> {
    let path = format!("/guide/{name}.webp");
    GUIDE
        .iter()
        .find(|asset| asset.path == path)
        .map(|asset| asset.url())
}

/// The static assets, served without request logging.
pub fn static_router() -> Router<AppState> {
    ASSETS
        .into_iter()
        .chain(GUIDE)
        .fold(Router::new(), |router, asset| {
            router.route(
                asset.path,
                get(move || async move {
                    (
                        [
                            (header::CONTENT_TYPE, asset.content_type),
                            // Its address names its version, so it never changes.
                            (
                                header::CACHE_CONTROL,
                                "public, max-age=31536000, immutable",
                            ),
                        ],
                        asset.contents,
                    )
                }),
            )
        })
}

/// `text` for an `FX-Notice` header, which `static/app.js` shows: percent-
/// encoded, since a header value is only ASCII.
fn notice(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b" .,;:!?'()-_/".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            // Writing to a String cannot fail.
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

/// A file to download, saved as `filename`, which needs no quoting.
fn attachment(filename: &str, data: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        data,
    )
        .into_response()
}

/// Puts the page into a not-found or forbidden answer, with the header for
/// whoever sent the request; see [`ErrorPage`]. Only such answers look up
/// the viewer, so every other request is spared it.
async fn error_pages(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers().clone();
    let response = next.run(request).await;
    let Some(&page) = response.extensions().get::<ErrorPage>() else {
        return response;
    };
    let (mut parts, ()) = Request::new(()).into_parts();
    parts.headers = headers;
    let viewer = <User as OptionalFromRequestParts<_>>::from_request_parts(
        &mut parts, &state,
    )
    .await
    .ok()
    .flatten();
    let body = match page {
        ErrorPage::NotFound => pages::not_found(viewer.as_ref()),
        ErrorPage::Forbidden => pages::forbidden(viewer.as_ref()),
    };
    (response.status(), body).into_response()
}

async fn fallback() -> AppError {
    AppError::NotFound
}

/// Dynamic routes - with request logging. `state` is for the layer that
/// draws the not-found and forbidden pages.
pub fn router(state: AppState) -> Router<AppState> {
    let router = Router::new()
        .route("/", get(schematics::home))
        .route("/builds", get(schematics::list))
        .route("/schematics/{id}", get(schematics::detail))
        .route("/leaderboards", get(leaderboards::people))
        .route("/leaderboards/{category}", get(leaderboards::show))
        .route("/schematics/{id}/download", get(schematics::download))
        .route(
            "/schematics/{id}/thumbnail.webp",
            get(schematics::thumbnail),
        )
        .route("/schematics/{id}/thumbnail.png", get(schematics::thumbnail))
        .route(
            "/schematics/{id}/edit",
            get(schematics::edit).post(schematics::save),
        )
        .route("/schematics/{id}/delete", post(schematics::delete))
        .route("/schematics/{id}/schemati", post(schemati::send))
        .route("/schematics/{id}/versions/new", get(versions::form))
        .route(
            "/schematics/{id}/versions/{version}/download",
            get(versions::download),
        )
        .route("/upload", get(upload::form).post(upload::publish))
        .route("/similar", get(similar::form).post(similar::search))
        .route("/upload/preview", post(upload::preview))
        .route("/upload/simulation", post(simulation::upload))
        .route(
            "/schematics/{id}/simulation",
            get(simulation::poll).post(simulation::stored),
        )
        .route("/settings", get(settings::show))
        .route("/settings/minecraft", post(settings::set_minecraft_name))
        .route("/settings/palette", post(settings::set_palette))
        .route("/welcome", get(settings::welcome))
        .route("/schemati", get(schemati::show))
        .route("/schemati/session", post(schemati::connect))
        .route("/schemati/session/delete", post(schemati::disconnect))
        .route("/schemati/files", post(schemati::add))
        .route("/schemati/files/{name}", get(schemati::download))
        .route("/schemati/files/{name}/rename", post(schemati::rename))
        .route("/schemati/files/{name}/delete", post(schemati::delete))
        .route("/about", get(about))
        .route("/about/categories", get(about::categories))
        .route("/about/conventions", get(about::conventions))
        .route("/about/uploading", get(about::before_upload))
        .route("/about/simulation", get(about::simulation))
        .route("/heads/{uuid}", get(heads::show))
        .route("/players/{name}", get(players::show))
        .route("/players/{name}/head.png", get(players::head))
        .route("/users", get(users::list))
        .route("/users/{username}", get(profile))
        .route("/users/{username}/role", post(users::set_role))
        .route(
            "/users/{username}/minecraft/verify",
            post(users::verify_minecraft),
        )
        .route(
            "/users/{username}/minecraft/name",
            post(users::name_minecraft),
        )
        .route("/sessions/new", get(auth::login))
        .route("/logout", post(session::delete))
        .route("/auth/discord", get(auth::start))
        .route("/auth/discord/callback", get(auth::callback));
    // Signing in without Discord, for a local server only.
    #[cfg(debug_assertions)]
    let router = router.route("/dev/login", get(dev::login));
    router
        .fallback(fallback)
        .layer(middleware::from_fn_with_state(state, error_pages))
}
