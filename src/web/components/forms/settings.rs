//! Settings form components (username, email, password).

use maud::{Markup, html};

pub fn username_form(
    new_username: &str,
    username_message: &str,
    username_is_success: bool,
) -> Markup {
    let aria_invalid = (!username_message.is_empty())
        .then_some(if username_is_success { "false" } else { "true" });
    html! {
        form id="username-form" fx-action="/settings" fx-method="patch" fx-target="#username-form" class="flex flex-col gap-4" {
            label for="new_username" class="field" {
                span { "New username" }
                input type="text" id="new_username" name="new_username" value=(new_username) required minlength="5" maxlength="20" autocomplete="username" class="input"
                    aria-invalid=[aria_invalid]
                    aria-describedby=[(!username_message.is_empty()).then_some("username-message")];
                @if !username_message.is_empty() {
                    small id="username-message" role=(if username_is_success { "status" } else { "alert" }) tabindex="-1" class="message" { (username_message) }
                }
            }
            button type="submit" class="btn btn-primary self-start" { "Update username" }
        }
    }
}

pub fn email_form(
    current_email: &str,
    message: &str,
    is_success: bool,
) -> Markup {
    let aria_invalid = (!message.is_empty()).then_some(if is_success {
        "false"
    } else {
        "true"
    });
    html! {
        form id="email-form" fx-action="/settings" fx-method="patch" fx-target="#email-form" class="flex flex-col gap-4" {
            label for="email" class="field" {
                span { "Email" }
                input type="email" id="email" name="email" value=(current_email) autocomplete="email" class="input"
                    aria-invalid=[aria_invalid]
                    aria-describedby=[(!message.is_empty()).then_some("email-message")];
                @if !message.is_empty() {
                    small id="email-message" role=(if is_success { "status" } else { "alert" }) tabindex="-1" class="message" { (message) }
                }
            }
            button type="submit" class="btn btn-primary self-start" { "Update email" }
        }
    }
}

pub fn password_form(
    current_password_message: &str,
    new_password_message: &str,
    current_password_is_success: bool,
    new_password_is_success: bool,
) -> Markup {
    let current_aria = (!current_password_message.is_empty()).then_some(
        if current_password_is_success {
            "false"
        } else {
            "true"
        },
    );
    let new_aria = (!new_password_message.is_empty()).then_some(
        if new_password_is_success {
            "false"
        } else {
            "true"
        },
    );
    html! {
        form id="password-form" fx-action="/settings" fx-method="patch" fx-target="#password-form" class="flex flex-col gap-4" {
            label for="current_password" class="field" {
                span { "Current password" }
                input type="password" id="current_password" name="current_password" required autocomplete="current-password" class="input"
                    aria-invalid=[current_aria]
                    aria-describedby=[(!current_password_message.is_empty()).then_some("current-password-message")];
                @if !current_password_message.is_empty() {
                    small id="current-password-message" role=(if current_password_is_success { "status" } else { "alert" }) tabindex="-1" class="message" { (current_password_message) }
                }
            }
            label for="new_password" class="field" {
                span { "New password" }
                input type="password" id="new_password" name="new_password" required minlength="8" maxlength="60" autocomplete="new-password" class="input"
                    aria-invalid=[new_aria]
                    aria-describedby=[(!new_password_message.is_empty()).then_some("new-password-message")];
                @if !new_password_message.is_empty() {
                    small id="new-password-message" role=(if new_password_is_success { "status" } else { "alert" }) tabindex="-1" class="message" { (new_password_message) }
                }
            }
            button type="submit" class="btn btn-primary self-start" { "Update password" }
        }
    }
}
