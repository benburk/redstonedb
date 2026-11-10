//! Who may reach which page, asked of the site as a browser asks it: a
//! visitor signed out is sent to log in, an account is refused another's
//! build, and a private build is no build at all to anyone but its
//! uploader and admins.

mod common;

use std::sync::Arc;

use app::{
    details::Details,
    models::{
        schematic::{self, Orientation},
        session::{Session, now_micros},
        user::{Role, User},
    },
    thumbnail::{Assets, Renderer},
    web::{self, app_state::AppState},
};
use reqwest::{StatusCode, header};
use sqlx::SqlitePool;
use uuid::Uuid;

/// The site, served on a free port of its own, and a client that leaves
/// redirects for the test to see.
struct Site {
    base: String,
    client: reqwest::Client,
    db: SqlitePool,
}

impl Site {
    async fn start() -> Self {
        let db = common::database().await;
        let http = reqwest::Client::new();
        let state = AppState {
            db: db.clone(),
            discord: None,
            renderer: Arc::new(Renderer::new(Assets::new(http.clone()))),
            http,
            thumbnails: Arc::default(),
            analyses: Arc::default(),
        };
        let app = web::router(state.clone()).with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a free port");
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        Site { base, client, db }
    }

    /// A session for `user`, as the cookie a browser would send.
    async fn session(&self, user: &User) -> String {
        let session = Session {
            id: Uuid::new_v4(),
            user_id: user.id,
            expires_at: now_micros() + 3_600_000_000,
        };
        Session::insert(&self.db, &session).await.unwrap();
        format!("session_id={}", session.id)
    }

    /// Sends `method` to `path`, as `cookie`'s account if one is given.
    async fn ask(
        &self,
        method: reqwest::Method,
        path: &str,
        cookie: Option<&str>,
    ) -> reqwest::Response {
        let mut request =
            self.client.request(method, format!("{}{path}", self.base));
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        request.send().await.expect("the site answers")
    }

    async fn get(&self, path: &str, cookie: Option<&str>) -> reqwest::Response {
        self.ask(reqwest::Method::GET, path, cookie).await
    }

    /// An adder uploaded by `by`.
    async fn adder(&self, by: &User) -> String {
        let given = Details {
            category: "adder".to_owned(),
            orientation: Orientation::Horizontal,
            ..Details::default()
        };
        common::store(&self.db, "alpha-rca", given, Some(by)).await
    }
}

/// Where a redirect sends the browser.
fn location(response: &reqwest::Response) -> &str {
    response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}

#[tokio::test]
async fn a_visitor_signed_out_is_sent_to_log_in_to_upload_or_edit() {
    let site = Site::start().await;
    let owner = common::sign_in(&site.db, "1", "steve").await;
    let id = site.adder(&owner).await;
    for path in [
        "/upload".to_owned(),
        "/settings".to_owned(),
        format!("/schematics/{id}/edit"),
        format!("/schematics/{id}/versions/new"),
        format!("/schematics/{id}/download"),
        format!("/schematics/{id}/download?colours=original"),
        format!("/schematics/{id}/versions/1/download"),
    ] {
        let response = site.get(&path, None).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER, "{path}");
        assert_eq!(location(&response), "/sessions/new", "{path}");
    }
}

#[tokio::test]
async fn a_visitor_signed_out_sees_the_viewer_but_not_a_download() {
    let site = Site::start().await;
    let owner = common::sign_in(&site.db, "1", "steve").await;
    let id = site.adder(&owner).await;

    let preview = site
        .get(&format!("/schematics/{id}/download?preview=yes"), None)
        .await;
    assert_eq!(preview.status(), StatusCode::OK);
    let page = site.get(&format!("/schematics/{id}"), None).await;
    let page = page.text().await.unwrap();
    assert!(page.contains("Sign in to download"));
    assert!(!page.contains(r#"commandfor="download" command="show-modal""#));
    assert_eq!(downloads(&site.db, &id).await, Vec::new());
}

#[tokio::test]
async fn each_download_is_logged_with_who_took_it() {
    let site = Site::start().await;
    let owner = common::sign_in(&site.db, "1", "steve").await;
    let other = common::sign_in(&site.db, "2", "alex").await;
    let id = site.adder(&owner).await;
    let others = site.session(&other).await;

    for path in [
        format!("/schematics/{id}/download"),
        format!("/schematics/{id}/download?preview=yes"),
        format!("/schematics/{id}/versions/1/download"),
    ] {
        let response = site.get(&path, Some(&others)).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
    }
    let head = site
        .ask(
            reqwest::Method::HEAD,
            &format!("/schematics/{id}/download"),
            Some(&others),
        )
        .await;
    assert_eq!(head.status(), StatusCode::OK);

    // The viewer's fetch and a HEAD request are no download.
    let alex = Some(other.id);
    assert_eq!(
        downloads(&site.db, &id).await,
        [
            (alex, 1, "file".to_owned()),
            (alex, 1, "version".to_owned())
        ]
    );
    let s = schematic::get(&site.db, &id).await.unwrap().unwrap();
    assert_eq!(s.downloads, 1, "the counter counts the current file alone");
}

/// Build `id`'s logged downloads, oldest first: who, which version, what.
async fn downloads(
    db: &SqlitePool,
    id: &str,
) -> Vec<(Option<Uuid>, i64, String)> {
    sqlx::query_as(
        "SELECT user_id, version, kind FROM download WHERE schematic_id = ? ORDER BY id",
    )
    .bind(id)
    .fetch_all(db)
    .await
    .unwrap()
}

#[tokio::test]
async fn only_a_builds_uploader_may_edit_it() {
    let site = Site::start().await;
    let owner = common::sign_in(&site.db, "1", "steve").await;
    let other = common::sign_in(&site.db, "2", "alex").await;
    let id = site.adder(&owner).await;
    let path = format!("/schematics/{id}/edit");

    let owners = site.session(&owner).await;
    let response = site.get(&path, Some(&owners)).await;
    assert_eq!(response.status(), StatusCode::OK);

    let others = site.session(&other).await;
    let response = site.get(&path, Some(&others)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = site
        .ask(
            reqwest::Method::POST,
            &format!("/schematics/{id}/delete"),
            Some(&others),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(schematic::get(&site.db, &id).await.unwrap().is_some());
}

/// Only an admin sees the moderation log, its link in the sidebar, or
/// puts back an edit from it; a mod is refused as anyone else is.
#[tokio::test]
async fn only_an_admin_sees_the_moderation_log() {
    let site = Site::start().await;
    for (id, name, role, allowed) in [
        ("1", "steve", Role::User, false),
        ("2", "alex", Role::Mod, false),
        ("3", "herobrine", Role::Admin, true),
    ] {
        let user = common::sign_in(&site.db, id, name).await;
        User::set_role(&site.db, user.id, role).await.unwrap();
        let cookie = site.session(&user).await;

        let response = site.get("/log", Some(&cookie)).await;
        let wanted = if allowed {
            StatusCode::OK
        } else {
            StatusCode::FORBIDDEN
        };
        assert_eq!(response.status(), wanted, "{name}");
        let home = site.get("/", Some(&cookie)).await.text().await.unwrap();
        assert_eq!(home.contains("href=\"/log\""), allowed, "{name}");
        if !allowed {
            let response = site
                .ask(
                    reqwest::Method::POST,
                    "/log/edits/1/revert",
                    Some(&cookie),
                )
                .await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{name}");
        }
    }
}

#[tokio::test]
async fn a_private_build_is_not_found_by_anyone_else() {
    let site = Site::start().await;
    let owner = common::sign_in(&site.db, "1", "steve").await;
    let other = common::sign_in(&site.db, "2", "alex").await;
    let id = site.adder(&owner).await;
    schematic::set_public(&site.db, &id, false).await.unwrap();

    let others = site.session(&other).await;
    for path in [
        format!("/schematics/{id}"),
        format!("/schematics/{id}/edit"),
        format!("/schematics/{id}/download"),
    ] {
        let response = site.get(&path, Some(&others)).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
    for path in [
        format!("/schematics/{id}"),
        format!("/schematics/{id}/download?preview=yes"),
    ] {
        let response = site.get(&path, None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }

    let owners = site.session(&owner).await;
    let response = site
        .get(&format!("/schematics/{id}/edit"), Some(&owners))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// Gives build `id` a second version, one of its solid blocks glass, and
/// returns that block's position.
async fn second_version(db: &SqlitePool, id: &str) -> [i32; 3] {
    let old = app::models::version::data(db, id, 1)
        .await
        .unwrap()
        .unwrap();
    let blocks = app::schem::Blocks::read(&old).unwrap();
    let (pos, _) = blocks
        .iter()
        .find(|(_, state)| app::schem::block::solid(state))
        .unwrap();
    let changes = std::collections::BTreeMap::from([(
        pos,
        "minecraft:white_stained_glass".to_owned(),
    )]);
    let new = app::schem::standardize::edit(&old, &changes).unwrap();
    let prepared = app::import::prepare_for(&new, common::IN_PLACE).unwrap();
    let how = app::models::version::Replacing::NextVersion {
        added_by: None,
        added_from: None,
        note: Some("glass"),
        restores: None,
    };
    app::models::version::replace(db, id, &prepared, how)
        .await
        .unwrap();
    pos
}

#[tokio::test]
async fn anyone_may_see_a_diff_but_only_an_account_download_it() {
    let site = Site::start().await;
    let owner = common::sign_in(&site.db, "1", "steve").await;
    let id = site.adder(&owner).await;
    second_version(&site.db, &id).await;

    let page = site.get(&format!("/schematics/{id}/diff"), None).await;
    assert_eq!(page.status(), StatusCode::OK);
    let page = page.text().await.unwrap();
    assert!(page.contains("+1 −1 blocks"), "{page}");
    // Two versions, so neither a step back nor a step on.
    assert!(!page.contains(r#"rel="prev""#) && !page.contains(r#"rel="next""#));
    for side in ["before", "after"] {
        let preview =
            format!("/schematics/{id}/diff/{side}?from=1&to=2&preview=yes");
        let response = site.get(&preview, None).await;
        assert_eq!(response.status(), StatusCode::OK, "{side}");
    }
    let file = format!("/schematics/{id}/diff/after?from=1&to=2");
    let response = site.get(&file, None).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let owners = site.session(&owner).await;
    let response = site.get(&file, Some(&owners)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        downloads(&site.db, &id).await,
        [(Some(owner.id), 2, "diff".to_owned())]
    );
    let missing = site
        .get(&format!("/schematics/{id}/diff?from=1&to=9"), None)
        .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_uploader_restores_an_earlier_version_as_a_new_one() {
    let site = Site::start().await;
    let owner = common::sign_in(&site.db, "1", "steve").await;
    let other = common::sign_in(&site.db, "2", "alex").await;
    let id = site.adder(&owner).await;
    second_version(&site.db, &id).await;
    let restore = format!("/schematics/{id}/versions/1/restore");

    let others = site.session(&other).await;
    let refused = site
        .ask(reqwest::Method::POST, &restore, Some(&others))
        .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);

    let owners = site.session(&owner).await;
    let response = site
        .ask(reqwest::Method::POST, &restore, Some(&owners))
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let versions = app::models::version::list(&site.db, &id).await.unwrap();
    let (newest, first) = (&versions[0], &versions[2]);
    assert_eq!(newest.version, 3);
    assert_eq!(newest.restores, Some(1));
    assert_eq!(newest.note.as_deref(), Some("Restored version 1"));
    assert_eq!(newest.added_by.as_deref(), Some("steve"));
    let file = |n| app::models::version::data(&site.db, &id, n);
    assert_eq!(file(3).await.unwrap(), file(first.version).await.unwrap());

    // The current version is no version to restore.
    let current = format!("/schematics/{id}/versions/3/restore");
    let response = site
        .ask(reqwest::Method::POST, &current, Some(&owners))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_link_to_a_build_previews_its_name_authors_size_and_picture() {
    let site = Site::start().await;
    let owner = common::sign_in(&site.db, "1", "steve").await;
    let id = site.adder(&owner).await;
    let page = site.get(&format!("/schematics/{id}"), None).await;
    let page = page.text().await.unwrap();
    let s = schematic::get(&site.db, &id).await.unwrap().unwrap();
    let title = format!(r#"<meta property="og:title" content="{}"#, s.name());
    assert!(page.contains(&title), "{page}");
    assert!(page.contains(" H×W×D, "), "{page}");
    assert!(page.contains(&format!(
        r#"<meta property="og:image" content="https://redstonedb.com/schematics/{id}/thumbnail.webp?v="#
    )));
    assert!(page.contains(r#"<meta name="twitter:card" content="summary">"#));
}

#[tokio::test]
async fn a_new_account_keeps_when_it_joined() {
    let site = Site::start().await;
    let user = common::sign_in(&site.db, "1", "steve").await;
    let at = user.created_at.clone();
    assert_eq!(at.len(), "2026-11-04T00:00:00Z".len());
    // A later login keeps the first one's time.
    let again = common::sign_in(&site.db, "1", "steve").await;
    assert_eq!(again.created_at, at);
}

/// A mod merges a build uploaded on its own into the one it fixes: its file
/// becomes that build's next version, noted as merged from it, and it is
/// gone. Its uploader, no mod, is refused, and a build isn't merged into
/// itself.
#[tokio::test]
async fn a_mod_merges_a_build_into_the_one_it_fixes() {
    let site = Site::start().await;
    let owner = common::sign_in(&site.db, "1", "steve").await;
    let moderator = common::sign_in(&site.db, "2", "alex").await;
    User::set_role(&site.db, moderator.id, Role::Mod)
        .await
        .unwrap();
    let into = site.adder(&owner).await;
    // The same build, a block changed, uploaded on its own.
    let given = Details {
        category: "adder".to_owned(),
        orientation: Orientation::Horizontal,
        ..Details::default()
    };
    let from = common::store(&site.db, "alpha-cla", given, Some(&owner)).await;
    let first = app::models::version::data(&site.db, &into, 1)
        .await
        .unwrap()
        .unwrap();
    let blocks = app::schem::Blocks::read(&first).unwrap();
    // A solid block nothing is hung on, so glass breaks no convention.
    let bare = |[x, y, z]: [i32; 3]| {
        [
            [1, 0, 0],
            [-1, 0, 0],
            [0, 1, 0],
            [0, -1, 0],
            [0, 0, 1],
            [0, 0, -1],
        ]
        .iter()
        .all(|[dx, dy, dz]| {
            let near = blocks.get([x + dx, y + dy, z + dz]);
            !near.contains("lever") && !near.contains("button")
        })
    };
    let (pos, _) = blocks
        .iter()
        .find(|&(pos, state)| app::schem::block::solid(state) && bare(pos))
        .unwrap();
    let glass = std::collections::BTreeMap::from([(
        pos,
        "minecraft:white_stained_glass".to_owned(),
    )]);
    let fixed = app::schem::standardize::edit(&first, &glass).unwrap();
    let fixed = app::import::prepare_for(&fixed, common::IN_PLACE).unwrap();
    let how = app::models::version::Replacing::NextVersion {
        added_by: None,
        added_from: None,
        note: None,
        restores: None,
    };
    app::models::version::replace(&site.db, &from, &fixed, how)
        .await
        .unwrap();
    let merge = |cookie: String, target: String| {
        site.client
            .post(format!("{}/schematics/{from}/merge", site.base))
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(format!("into={target}&note="))
            .send()
    };
    let link = format!("https://redstonedb.com/schematics/{into}");

    let refused = merge(site.session(&owner).await, link.clone())
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    let mods = site.session(&moderator).await;
    let itself = merge(mods.clone(), from.clone()).await.unwrap();
    assert_eq!(itself.status(), StatusCode::BAD_REQUEST);

    let response = merge(mods, link).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&response), format!("/schematics/{into}"));
    let versions = app::models::version::list(&site.db, &into).await.unwrap();
    assert_eq!(versions[0].version, 2);
    assert_eq!(versions[0].added_by.as_deref(), Some("alex"));
    assert!(
        versions[0]
            .note
            .as_deref()
            .is_some_and(|n| n.starts_with("Merged from")),
        "{:?}",
        versions[0].note
    );
    assert!(schematic::get(&site.db, &from).await.unwrap().is_none());
}
