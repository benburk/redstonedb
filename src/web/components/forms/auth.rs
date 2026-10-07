//! Authentication form components (login, signup).

use maud::{Markup, html};

const CARD: &str = "mx-auto mt-8 max-w-sm rounded-overlay border border-line bg-raised p-6 shadow-sm";
const TITLE: &str = "mb-6 text-page font-bold";
const FORM: &str = "flex flex-col gap-4";
const FOOTER: &str = "mt-6 text-muted";

pub fn login_form(username: &str, error_message: &str) -> Markup {
    let has_error = !error_message.is_empty();
    html! {
        section id="login-form" class=(CARD) {
            h1 class=(TITLE) { "Log in" }
            form action="/sessions" method="post"
                fx-action="/sessions" fx-method="post" fx-target="#login-form"
                class=(FORM) {
                    label class="field" {
                        span { "Username" }
                        input name="username" type="text" required autofocus autocomplete="username" class="input"
                            value=[has_error.then_some(username)]
                            aria-invalid=[has_error.then_some("true")]
                            aria-describedby=[has_error.then_some("login-error")];
                    }
                    label class="field" {
                        span { "Password" }
                        input name="password" type="password" required autocomplete="current-password" class="input"
                            aria-invalid=[has_error.then_some("true")]
                            aria-describedby=[has_error.then_some("login-error")];
                        @if has_error {
                            small id="login-error" role="alert" class="message" { (error_message) }
                        }
                    }
                button type="submit" class="btn btn-primary" { "Log in" }
            }
            p class=(FOOTER) { "Don't have an account? " a href="/users/new" { "Sign up" } }
        }
    }
}

pub fn signup_form(
    invite_code: &str,
    invite_issuer: &str,
    invite_message: &str,
    username: &str,
    username_message: &str,
    password_message: &str,
) -> Markup {
    let invite_description = if !invite_message.is_empty() {
        Some("signup-invite-error")
    } else if !invite_issuer.is_empty() {
        Some("signup-invite-issuer")
    } else {
        Some("signup-invite-help")
    };

    html! {
        section id="signup-form" class=(CARD) {
            h1 class=(TITLE) { "Sign up" }
            form action="/invite-codes/check" method="post" class=(FORM) {
                    label class="field" {
                        span { "Invite code" }
                        input name="invite_code" type="text" placeholder="XXXX-XXXX-XXXX-XXXX" class="input font-mono"
                            value=(invite_code) required autofocus[invite_issuer.is_empty()]
                            autocomplete="off" autocapitalize="characters" spellcheck="false"
                            aria-invalid=[(!invite_message.is_empty()).then_some("true")]
                            aria-describedby=[invite_description];
                        @if !invite_message.is_empty() {
                            small id="signup-invite-error" role="alert" class="message" { (invite_message) }
                        } @else if !invite_issuer.is_empty() {
                            small id="signup-invite-issuer" class="message" { "Issued by " (invite_issuer) "." }
                        } @else {
                            small id="signup-invite-help" class="message" { "Enter the 16-character code you received." }
                        }
                    }
                button type="submit" class={ "btn " (if invite_issuer.is_empty() { "btn-primary" } else { "btn-quiet" }) } { "Check invite" }
            }
            @if !invite_issuer.is_empty() {
                form action="/users" method="post"
                    fx-action="/users" fx-method="post" fx-target="#signup-form"
                    class={ (FORM) " mt-6 border-t border-line pt-6" } {
                    input name="invite_code" type="hidden" value=(invite_code);
                        label class="field" {
                            span { "Username" }
                            input name="username" type="text" value=(username) required autofocus autocomplete="username" class="input"
                                aria-invalid=[(!username_message.is_empty()).then_some("true")]
                                aria-describedby=[(!username_message.is_empty()).then_some("signup-username-error")];
                            @if !username_message.is_empty() {
                                small id="signup-username-error" role="alert" class="message" { (username_message) }
                            }
                        }
                        label class="field" {
                            span { "Password" }
                            input name="password" type="password" required autocomplete="new-password" class="input"
                                aria-invalid=[(!password_message.is_empty()).then_some("true")]
                                aria-describedby=[(!password_message.is_empty()).then_some("signup-password-error")];
                            @if !password_message.is_empty() {
                                small id="signup-password-error" role="alert" class="message" { (password_message) }
                            }
                        }
                    button type="submit" class="btn btn-primary" { "Sign up" }
                }
            }
            p class=(FOOTER) { "Already have an account? " a href="/sessions/new" { "Log in" } }
        }
    }
}
