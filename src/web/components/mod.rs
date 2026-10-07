mod forms;
mod layout;

pub use forms::{
    email_form, login_form, password_form, signup_form, username_form,
};
pub use layout::base;

/// Display struct for rendering session info in templates.
pub struct SessionDisplay {
    pub id: String,
    pub ip_address: String,
    pub user_agent: String,
    pub created_at: String,
    pub expires_at: String,
    pub is_current: bool,
}
