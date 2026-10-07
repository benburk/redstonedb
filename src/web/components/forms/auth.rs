//! Authentication form components (login, signup).

use maud::{Markup, html};

pub fn login_form(username: &str, error_message: &str) -> Markup {
    let has_error = !error_message.is_empty();
    html! {
        article id="login-form" {
            header { h1 { "Log in" } }
            form action="/sessions" method="post"
                fx-action="/sessions" fx-method="post" fx-target="#login-form" {
                fieldset {
                    legend { "Credentials" }
                    label {
                        "Username "
                        input name="username" type="text" placeholder="Username" required autofocus autocomplete="username"
                            value=[has_error.then_some(username)]
                            aria-invalid=[has_error.then_some("true")]
                            aria-describedby=[has_error.then_some("login-error")];
                    }
                    label {
                        "Password "
                        input name="password" type="password" placeholder="Password" required autocomplete="current-password"
                            aria-invalid=[has_error.then_some("true")]
                            aria-describedby=[has_error.then_some("login-error")];
                        @if has_error {
                            small id="login-error" role="alert" { (error_message) }
                        }
                    }
                }
                button type="submit" { "Log in" }
            }
            footer { "Don't have an account? " a href="/users/new" { "Sign up" } }
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
        article id="signup-form" {
            header { h1 { "Sign up" } }
            form action="/invite-codes/check" method="post" {
                fieldset {
                    legend { "Invitation" }
                    label {
                        "Invite code"
                        input name="invite_code" type="text" placeholder="XXXX-XXXX-XXXX-XXXX"
                            value=(invite_code) required autofocus[invite_issuer.is_empty()]
                            autocomplete="off" autocapitalize="characters" spellcheck="false"
                            aria-invalid=[(!invite_message.is_empty()).then_some("true")]
                            aria-describedby=[invite_description];
                        @if !invite_message.is_empty() {
                            small id="signup-invite-error" role="alert" { (invite_message) }
                        } @else if !invite_issuer.is_empty() {
                            small id="signup-invite-issuer" { "Issued by " (invite_issuer) "." }
                        } @else {
                            small id="signup-invite-help" { "Enter the 16-character code you received." }
                        }
                    }
                }
                button type="submit" { "Check invite" }
            }
            @if !invite_issuer.is_empty() {
                form action="/users" method="post"
                    fx-action="/users" fx-method="post" fx-target="#signup-form" {
                    input name="invite_code" type="hidden" value=(invite_code);
                    fieldset {
                        legend { "Credentials" }
                        label {
                            "Username"
                            input name="username" type="text" placeholder="Username" value=(username) required autofocus autocomplete="username"
                                aria-invalid=[(!username_message.is_empty()).then_some("true")]
                                aria-describedby=[(!username_message.is_empty()).then_some("signup-username-error")];
                            @if !username_message.is_empty() {
                                small id="signup-username-error" role="alert" { (username_message) }
                            }
                        }
                        label {
                            "Password"
                            input name="password" type="password" placeholder="Password" required autocomplete="new-password"
                                aria-invalid=[(!password_message.is_empty()).then_some("true")]
                                aria-describedby=[(!password_message.is_empty()).then_some("signup-password-error")];
                            @if !password_message.is_empty() {
                                small id="signup-password-error" role="alert" { (password_message) }
                            }
                        }
                    }
                    button type="submit" { "Sign up" }
                }
            }
            footer { "Already have an account? " a href="/sessions/new" { "Log in" } }
        }
    }
}
