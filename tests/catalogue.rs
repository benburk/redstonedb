//! The code's table of categories and the list of them the migrations seed
//! hold the same categories, by the same names and labels, so neither
//! drifts from the other unnoticed.

mod common;

use app::{category::Kind, models::catalogue};

#[tokio::test]
async fn the_table_and_the_seeded_list_agree() {
    let db = common::database().await;
    let mut seeded: Vec<(String, String)> = catalogue::categories(&db)
        .await
        .expect("categories")
        .into_iter()
        .map(|c| (c.name, c.label))
        .collect();
    let mut table: Vec<(String, String)> = Kind::all()
        .map(|kind| (kind.name().to_owned(), kind.label().to_owned()))
        .collect();
    seeded.sort();
    table.sort();
    assert_eq!(table, seeded);
}
