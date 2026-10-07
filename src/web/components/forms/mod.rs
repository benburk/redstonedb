//! Form components for fixi partial updates.
//!
//! These return HTML fragments, not full pages. Fixi replaces each submitted
//! form with the returned component.

mod auth;
mod settings;

pub use auth::{login_form, signup_form};
pub use settings::{email_form, password_form, username_form};
