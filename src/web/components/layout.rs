use maud::{DOCTYPE, Markup, html};

use crate::APP_NAME;

pub fn base(
    username: &str,
    page_title: &str,
    current_path: &str,
    content: &Markup,
) -> Markup {
    let document_title = if page_title == APP_NAME {
        APP_NAME.to_owned()
    } else {
        format!("{page_title} · {APP_NAME}")
    };
    html! {
        (DOCTYPE)
        html lang="en" data-theme="light" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (document_title) }
                link rel="stylesheet" href="/pico.min.css";
                link rel="stylesheet" href="/pico.colors.min.css";
                script defer src="/fixi.js" {}
                script defer src="/fixi-config.js" {}
            }
            body {
                (navbar(username, current_path))
                main class="container" {
                    (content)
                }
            }
        }
    }
}

pub fn navbar(username: &str, current_path: &str) -> Markup {
    let profile_path = format!("/users/{username}");
    html! {
        nav class="container-fluid" aria-label="Primary" {
            ul {
                li {
                    strong {
                        a href="/" aria-current=[(current_path == "/").then_some("page")] { (APP_NAME) }
                    }
                }
            }
            ul {
                li {
                    a href="/about" aria-current=[(current_path == "/about").then_some("page")] { "About" }
                }
                @if username.is_empty() {
                    li {
                        a href="/sessions/new" aria-current=[(current_path == "/sessions/new").then_some("page")] { "Log in" }
                    }
                    li {
                        a href="/users/new" aria-current=[(current_path == "/users/new").then_some("page")] { "Sign up" }
                    }
                } @else {
                    li {
                        a href=(profile_path.as_str()) aria-current=[(current_path == profile_path).then_some("page")] { (username) }
                    }
                    li {
                        a href="/settings" aria-current=[(current_path == "/settings").then_some("page")] { "Settings" }
                    }
                    li {
                        a href="/" fx-action="/sessions" fx-method="delete" fx-swap="none" class="secondary" {
                            "Logout"
                        }
                    }
                }
            }
        }
    }
}
