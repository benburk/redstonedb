//! Which file an upload keeps: standardized when the site recognizes it as
//! the category it is given, with its gray blocks kept only when its copies
//! stack with them left out; as its builder made it, under a category off
//! the list, when it does not; and a new version that is recognized moving
//! a build off the list up.

use app::{
    details,
    import::{self, Kept, Upload},
    models::catalogue::{self, Category},
    schem::{self, Blocks},
    standardize::Gray,
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

/// The standard categories, as the migrations list them.
async fn categories() -> Vec<Category> {
    let pool: SqlitePool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .expect("an in-memory database");
    sqlx::migrate!().run(&pool).await.expect("migrations run");
    catalogue::categories(&pool).await.expect("categories")
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("tests/fixtures/{name}.schem"))
        .expect("fixture is readable")
}

/// Each block of `blocks` other than air, by position.
fn placed(blocks: &Blocks) -> Vec<(schem::Pos, String)> {
    blocks
        .iter()
        .filter(|(_, state)| *state != schem::AIR)
        .map(|(pos, state)| (pos, state.to_owned()))
        .collect()
}

fn gray(blocks: &Blocks) -> usize {
    blocks
        .iter()
        .filter(|(_, state)| schem::short(state).starts_with("gray_"))
        .count()
}

#[tokio::test]
async fn a_recognized_build_is_kept_standardized() {
    let categories = categories().await;
    let upload = Upload::read(&fixture("cca-16-raw")).unwrap();
    let standard = upload.standard.as_ref().unwrap();
    assert_ne!(
        placed(&upload.original.blocks),
        placed(&standard.blocks),
        "the raw build is not in the palette"
    );
    let Ok(Kept::Standard { file, category }) =
        import::kept(&categories, "adder", &upload).await.unwrap()
    else {
        panic!("the adder is not recognized");
    };
    assert_eq!(category, "adder");
    assert_eq!(placed(&file.blocks), placed(&standard.blocks));
    // Named for it off the list, it goes up as one all the same.
    let Ok(Kept::Standard { category, .. }) =
        import::kept(&categories, "16-bit adder", &upload)
            .await
            .unwrap()
    else {
        panic!("the adder is not recognized by its off-list name");
    };
    assert_eq!(category, "adder");
}

#[tokio::test]
async fn an_unrecognized_build_keeps_its_own_blocks_off_the_list() {
    let categories = categories().await;
    let upload = Upload::read(&fixture("cca-16-raw")).unwrap();
    let Ok(Kept::Original(Some(off))) =
        import::kept(&categories, "multiplier", &upload)
            .await
            .unwrap()
    else {
        panic!("an adder passed for a multiplier");
    };
    assert!(off.name.ends_with("multiplier"), "{}", off.name);
    let failing = Upload::read(&fixture("cca-8-dust-out")).unwrap();
    let Ok(Kept::Original(Some(off))) =
        import::kept(&categories, "adder", &failing).await.unwrap()
    else {
        panic!("a failing adder was recognized");
    };
    assert_eq!(off.name, "8-bit adder");
    assert!(
        off.why.contains("does not pass the adder test"),
        "{}",
        off.why
    );
}

#[tokio::test]
async fn a_category_no_test_recognizes_keeps_the_build_as_built() {
    let categories = categories().await;
    let upload = Upload::read(&fixture("cca-16-raw")).unwrap();
    assert!(matches!(
        import::kept(&categories, "piston door", &upload)
            .await
            .unwrap(),
        Ok(Kept::Original(None))
    ));
}

#[tokio::test]
async fn an_off_list_name_means_the_category_it_stands_in_for() {
    let categories = categories().await;
    let meant =
        |name| details::meant(&categories, name).map(|c| c.name.clone());
    assert_eq!(meant("8-bit adder").as_deref(), Some("adder"));
    assert_eq!(meant("32-bit_adder").as_deref(), Some("adder"));
    assert_eq!(meant("Adder").as_deref(), Some("adder"));
    assert_eq!(meant("piston door"), None);
    assert_eq!(meant("eight-bit adder"), None);
}

/// Gray stays only where the copies stack with it. This register's gray
/// driver sits inside the box its copies would stack, so it stacks no way,
/// and its gray is whitened.
#[tokio::test]
async fn gray_stays_only_where_the_copies_stack_with_it() {
    let standard = |name| {
        import::prepare(&fixture(name), Some(Gray::Keep), true, false, true)
            .unwrap()
    };
    let register = standard("register-1-gray-driver");
    assert!(gray(&register.blocks) > 0, "the fixture has gray blocks");
    let kept = import::gray_settled(&register, "register").await.unwrap();
    assert_eq!(gray(&kept.blocks), 0);

    // Taken for a multiplier, which never stacks, its gray means nothing.
    let whitened = import::gray_settled(&register, "multiplier").await.unwrap();
    assert_eq!(gray(&whitened.blocks), 0);
}

#[tokio::test]
async fn a_new_version_that_is_recognized_moves_a_build_up() {
    let categories = categories().await;
    let file = import::as_built(&fixture("cca-16-raw")).unwrap();
    let (moved, to) =
        import::moved_up(&categories, "16-bit_adder", &file, false)
            .await
            .unwrap()
            .expect("it moves up");
    assert_eq!(to.name, "adder");
    assert!(moved.conventional);
    assert!(
        import::moved_up(&categories, "16-bit_multiplier", &file, false)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        import::moved_up(&categories, "adder", &file, false)
            .await
            .unwrap()
            .is_none(),
        "a build on the list stays where it is"
    );
}

/// Every category the site recognizes builds of names its off-list stand-in
/// so it reads back, whatever its label holds, as XOR/XNOR Gate's slash.
#[tokio::test]
async fn an_off_list_name_reads_back_as_its_category() {
    let categories = categories().await;
    let evaluated = categories
        .iter()
        .filter(|c| app::category::behaviour(&c.name).evaluated);
    for category in evaluated {
        for width in [Some(8), None] {
            let name = details::off_list_name(category, width);
            let slug = app::models::tag::name(&name);
            assert!(app::models::tag::valid_name(&slug), "{slug}");
            assert_eq!(
                app::models::simulation::meant_by(&slug),
                Some(category.name.as_str()),
                "{slug}"
            );
            assert!(Category::find(&categories, &name).is_none(), "{name}");
        }
    }
}
