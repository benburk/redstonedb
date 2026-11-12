//! The files under `static/`, built into the binary, and their routes.

use axum::{Router, http::header, routing::get};

use crate::web::app_state::AppState;

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

pub(crate) const ASSETS: [Asset; 7] = [
    CSS, FIXI_JS, APP_JS, FORM_JS, SIDEBAR_JS, THEME_JS, VIEWER_JS,
];

/// A How builds are recognized picture, drawn by
/// `rsdb guide-images static/guide`.
macro_rules! guide_image {
    ($name:literal) => {
        Asset::new(
            concat!("/guide/", $name, ".webp"),
            "image/webp",
            include_bytes!(concat!("../../static/guide/", $name, ".webp")),
        )
    };
}

/// Where pages link the How builds are recognized picture of example
/// `name`.
pub fn guide_image(name: &str) -> Option<String> {
    let path = format!("/guide/{name}.webp");
    GUIDE
        .iter()
        .find(|asset| asset.path == path)
        .map(|asset| asset.url())
}

/// The How builds are recognized pictures, one for each of
/// [`crate::recognize::guide::examples`].
pub(crate) const GUIDE: [Asset; 11] = [
    guide_image!("input"),
    guide_image!("input-dust-below"),
    guide_image!("repeater"),
    guide_image!("data-dust-on-top"),
    guide_image!("strength-input"),
    guide_image!("output"),
    guide_image!("output-powering"),
    guide_image!("bus-straight"),
    guide_image!("bus-diagonal"),
    guide_image!("bus-alternating"),
    guide_image!("bus-uneven"),
];

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
                            // Its address names its version, so it never
                            // changes.
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
