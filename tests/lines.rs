//! A build's roles kept apart from its blocks: carried by a file while the
//! site works on it, moved with its blocks, kept with its version in the
//! database, and shown in a theme's colours, its blocks its author's.

mod common;

use app::{
    details::{self, Build, Details},
    import,
    models::{schematic::Orientation, version},
    recognize::{
        interface::{self, Kept},
        palette::{self, Palette},
    },
    schem::{
        Blocks, lines,
        lines::Role,
        orient::{self, Turn},
        standardize,
    },
};

/// Each line's name, role, and bit count, as the file in `data` reads.
fn lines_of(data: &[u8]) -> Vec<(String, Option<Role>, usize)> {
    let read = interface::read(&Blocks::read(data).unwrap());
    read.inputs
        .iter()
        .chain(&read.outputs)
        .map(|port| (port.name.clone(), port.role, port.bits.len()))
        .collect()
}

/// Turning a build, and fitting its box, take its roles where its blocks
/// go, so it reads the same lines; and taking a block out takes its role.
#[test]
fn roles_move_with_their_blocks() {
    let data = common::fixture("alpha-rca");
    let before = lines_of(&data);
    assert!(
        before
            .iter()
            .any(|(name, _, bits)| name == "A" && *bits == 8)
    );
    let turn = Turn {
        mirrored: true,
        quarters: 1,
    };
    assert_eq!(lines_of(&orient::turned(&data, turn).unwrap()), before);
    assert_eq!(lines_of(&standardize::tidy(&data).unwrap()), before);
    let blocks = Blocks::read(&data).unwrap();
    let roles = blocks.roles.clone().unwrap();
    let (&gone, _) = roles.0.iter().next().unwrap();
    let edited = standardize::edit(
        &data,
        &[(gone, app::schem::AIR.to_owned())].into_iter().collect(),
    )
    .unwrap();
    let left = Blocks::read(&edited).unwrap().roles.unwrap();
    assert_eq!(left.get(gone), None);
    assert_eq!(left.0.len(), roles.0.len() - 1);
}

/// A file is hashed and kept without the roles it carries, which are kept
/// beside it, so the same blocks are the same file whatever their roles.
#[test]
fn a_file_is_kept_without_its_roles() {
    let data = common::fixture("alpha-rca");
    let prepared = import::prepare_for(&data, common::IN_PLACE).unwrap();
    assert!(prepared.roles().is_some());
    let stored = prepared.stored().unwrap();
    assert_eq!(Blocks::read(&stored).unwrap().roles, None);
    let bare =
        import::prepare_for(&lines::without(&data).unwrap(), common::IN_PLACE)
            .unwrap();
    assert_eq!(bare.sha256, prepared.sha256);
}

/// The lines kept with a version read back as the same lines.
#[test]
fn kept_lines_read_back_the_same() {
    let data = common::fixture("alpha-cla");
    let blocks = Blocks::read(&data).unwrap();
    let kept = Kept::read(&blocks).unwrap();
    let carry = kept.outputs.iter().find(|l| l.name == "carry out").unwrap();
    assert_eq!(carry.role, Some(Role::Status));
    assert!(carry.signed);
    let json = serde_json::to_string(&kept).unwrap();
    let back: Kept = serde_json::from_str(&json).unwrap();
    let bare = lines::without(&data).unwrap();
    let again = lines::with(&bare, Some(&back.roles())).unwrap();
    assert_eq!(Kept::read(&Blocks::read(&again).unwrap()), Some(kept));
}

/// A stored build's roles go into the database with its version, and its
/// current file carries them again.
#[tokio::test]
async fn a_versions_roles_are_kept_with_it() {
    let db = common::database().await;
    let first = common::conventional("alpha-rca");
    let given = Details {
        category: "adder".to_owned(),
        orientation: Orientation::Horizontal,
        ..Details::default()
    };
    let ready = details::check(&db, given, Build::of(&first), false)
        .await
        .unwrap()
        .unwrap();
    let id = import::store(&db, &first, &ready, None, None, None, None)
        .await
        .unwrap()
        .unwrap();
    let stored = version::data(&db, &id, 1).await.unwrap().unwrap();
    assert_eq!(Blocks::read(&stored).unwrap().roles, None);
    let current = version::current_file(&db, &id).await.unwrap().unwrap();
    assert_eq!(lines_of(&current), lines_of(&first.data));
}

/// A theme colours each block with a role in its colour, as the roles the
/// file carries say, whatever its author made it: here every input and
/// output block gray wool.
#[test]
fn a_theme_colours_the_roles_not_the_blocks() {
    let data = common::fixture("alpha-rca");
    let blocks = Blocks::read(&data).unwrap();
    let roles = blocks.roles.clone().unwrap();
    let gray = roles
        .0
        .keys()
        .map(|&pos| (pos, "minecraft:gray_wool".to_owned()))
        .collect();
    let built = standardize::edit(&data, &gray).unwrap();
    let shown = palette::apply(&built, None, &Palette::default()).unwrap();
    let shown = Blocks::read(&shown).unwrap();
    assert_eq!(shown.roles, None, "a download carries no roles");
    for (&pos, &role) in &roles.0 {
        let colour = app::recognize::colour::shown(role);
        assert_eq!(
            shown.get(pos),
            format!("minecraft:{colour}_concrete"),
            "{role:?} at {pos:?}"
        );
    }
}

/// A stored build's roles, first found by probing, name A the operand its
/// old colours mark A where its test adds either way round: the port of an
/// 8-bit carry-cancel adder whose author put A in lime at x = 3 and B in
/// light blue at x = 0 reads A at x = 3, though x = 0 is further west. Its
/// colours whitened, nothing marks either, and A is the westernmost.
#[test]
fn interchangeable_operands_follow_the_authors_marking() {
    let raw = std::fs::read("tests/fixtures/cca-8-b-slower.schem").unwrap();
    let a_at = |data: &[u8]| {
        let found = import::with_roles(data, "adder").unwrap();
        let roles = lines::of(&found).unwrap().expect("its roles");
        let xs: Vec<i32> = roles
            .0
            .iter()
            .filter(|&(_, role)| *role == Role::Operand(0))
            .map(|(pos, _)| pos[0])
            .collect();
        assert_eq!(xs.len(), 8);
        xs[0]
    };
    assert_eq!(a_at(&raw), 3);
    let blocks = Blocks::read(&raw).unwrap();
    let white: std::collections::BTreeMap<_, _> = blocks
        .iter()
        .filter(|&(_, state)| {
            ["minecraft:lime_concrete", "minecraft:light_blue_concrete"]
                .contains(&state)
        })
        .map(|(pos, _)| (pos, "minecraft:white_concrete".to_owned()))
        .collect();
    assert_eq!(white.len(), 16);
    let whitened = standardize::edit(&raw, &white).unwrap();
    assert_eq!(a_at(&whitened), 0);
}
