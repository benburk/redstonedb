// `sqlx::migrate!` embeds the migrations at compile time, so a new or changed
// one must rebuild the crate.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
