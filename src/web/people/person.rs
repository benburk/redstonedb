//! A person's page, the same for a player by their Minecraft name and for
//! an account by its username: who they are, the records they hold, and
//! their builds, in lists to choose between. An account whose Minecraft
//! name is confirmed is that player, so its address goes to the player's.

use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::{
    account::User,
    build::{self, Build},
    leaderboard::{self, Record},
    store::builds,
    web::{
        app_state::AppState,
        browse::list,
        error::AppError,
        href,
        people::{
            heads,
            person_view::{self, List, PersonPage},
        },
    },
};

/// Whose page it is.
pub(crate) enum Person<'a> {
    /// A player, by their Minecraft name as the site spells it, with the
    /// account that has confirmed the name, if they have signed up.
    Player {
        name: &'a str,
        account: Option<&'a User>,
    },
    /// An account with no confirmed Minecraft name: none given, or one an
    /// admin has yet to confirm.
    Account(&'a User),
}

impl<'a> Person<'a> {
    /// Their account, if they have signed up.
    pub(crate) fn account(&self) -> Option<&'a User> {
        match *self {
            Person::Player { account, .. } => account,
            Person::Account(account) => Some(account),
        }
    }

    /// The Minecraft name whose credits the page lists: a player's own, or
    /// the one an account gave, confirmed or not.
    pub(crate) fn credited_as(&self) -> Option<&'a str> {
        match *self {
            Person::Player { name, .. } => Some(name),
            Person::Account(account) => account.minecraft_name.as_deref(),
        }
    }

    /// The name at the top of the page.
    pub(crate) fn name(&self) -> &'a str {
        match *self {
            Person::Player { name, .. } => name,
            Person::Account(account) => account.name(),
        }
    }

    /// The page's address.
    pub(crate) fn path(&self) -> String {
        match *self {
            Person::Player { name, .. } => href::player(name),
            Person::Account(account) => href::user(&account.username),
        }
    }
}

/// Which of a person's lists of builds to show, by its name; the first
/// when none is named.
#[derive(Deserialize)]
pub(crate) struct Shown {
    pub show: Option<String>,
}

/// `path` showing the list `show` names, if it names one.
pub(crate) fn showing(path: &str, show: Option<&str>) -> String {
    let Some(show) = show else {
        return path.to_owned();
    };
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("show", show)
        .finish();
    format!("{path}?{query}")
}

impl Person<'_> {
    /// Whether `names`, in-game names a build credits, have theirs among
    /// them, whatever its case.
    fn among(&self, names: &[String]) -> bool {
        self.credited_as().is_some_and(|own| {
            names.iter().any(|name| name.eq_ignore_ascii_case(own))
        })
    }

    fn built(&self, s: &Build) -> bool {
        self.among(&s.credits.authors)
    }

    fn contributed(&self, s: &Build) -> bool {
        self.among(&s.credits.contributors)
    }

    fn uploaded(&self, s: &Build) -> bool {
        self.account().is_some_and(|account| account.uploaded(s))
    }

    /// Whether the description of `s` names them with `@name`.
    fn mentioned(&self, s: &Build) -> bool {
        let named = s.description.as_deref().map(build::mentions);
        self.among(&named.unwrap_or_default())
    }

    /// Their lists of builds among `all`, in the page's order: every build
    /// of theirs, those they built and contributed to, for a player those
    /// only naming them, and for an account those it uploaded. With
    /// `favourites`, the ids of the builds the account starred, as on its
    /// own page, those it keeps private and those it starred too.
    fn lists(&self, all: &[Build], favourites: Option<&[String]>) -> Vec<List> {
        let pick = |keep: &dyn Fn(&Build) -> bool| -> Vec<Build> {
            all.iter().filter(|s| keep(s)).cloned().collect()
        };
        let list = |name, label, builds| List {
            name,
            label,
            builds,
        };
        let theirs = |s: &Build| {
            self.built(s) || self.contributed(s) || self.uploaded(s)
        };
        let mut lists = vec![
            list("all", "All", pick(&theirs)),
            list("built", "Built", pick(&|s| self.built(s))),
            list(
                "contributed",
                "Contributed to",
                pick(&|s| self.contributed(s)),
            ),
        ];
        // A name not yet confirmed lists its credits, but is named
        // nowhere: that is the player's, on the player's page.
        if matches!(self, Person::Player { .. }) {
            // Those that credit them already list them.
            let mentioned = pick(&|s| {
                self.mentioned(s) && !self.built(s) && !self.contributed(s)
            });
            if !mentioned.is_empty() {
                lists.push(list("mentioned", "Mentioned in", mentioned));
            }
        }
        if self.account().is_some() {
            let uploaded = pick(&|s| self.uploaded(s));
            lists.push(list("uploaded", "Uploaded", uploaded));
        }
        if let Some(ids) = favourites {
            let private = pick(&|s| self.uploaded(s) && !s.entry.public);
            lists.push(list("private", "Private", private));
            // Most lately favourited first.
            let starred = ids
                .iter()
                .filter_map(|id| all.iter().find(|s| &s.id == id).cloned())
                .collect();
            lists.push(list("favourites", "Favourites", starred));
        }
        lists
    }
}

/// The page of `who`, among `all` the builds `viewer` may see, with the
/// list `show` names chosen; see [`Person::lists`].
pub(crate) async fn page(
    state: &AppState,
    viewer: Option<&User>,
    who: &Person<'_>,
    show: Option<&str>,
    mut all: Vec<Build>,
) -> Result<Response, AppError> {
    // Newest first, as on the home page.
    all.sort_by(|a, b| b.entry.added_at.cmp(&a.entry.added_at));
    let account = who.account();
    let own = account
        .filter(|account| viewer.is_some_and(|viewer| viewer.id == account.id));
    let favourites = match own {
        Some(account) => Some(builds::favorites(&state.db, account.id).await?),
        None => None,
    };
    let lists = who.lists(&all, favourites.as_deref());
    // One only named in descriptions opens on those.
    let first = if lists[0].builds.is_empty() {
        lists.iter().position(|list| list.name == "mentioned")
    } else {
        None
    };
    let shown = show
        .and_then(|show| lists.iter().position(|list| list.name == show))
        .or(first)
        .unwrap_or(0);
    // A player holds records, among the builds anyone may see: private
    // ones compete with no one. A name not yet confirmed holds none.
    let player = matches!(who, Person::Player { .. });
    let ranked: Vec<&Build> =
        all.iter().filter(|s| player && s.entry.public).collect();
    let stacks = leaderboard::stacked(ranked.iter().copied());
    let ranked: Vec<&Build> = ranked.into_iter().chain(&stacks).collect();
    let records = leaderboard::records(&ranked);
    let held: Vec<&Record> = records
        .iter()
        .filter(|record| record.holders.iter().any(|s| who.built(s)))
        .collect();
    let uuid = match (account.and_then(|a| a.minecraft_uuid.clone()), who) {
        (Some(uuid), _) => Some(uuid),
        (None, Person::Player { name, .. }) => {
            heads::player(&state.db, name).await.map(|found| found.uuid)
        }
        (None, Person::Account(_)) => None,
    };
    Ok(person_view::person(
        viewer,
        &PersonPage {
            who,
            uuid: uuid.as_deref(),
            records: &held,
            lists: &lists,
            shown,
            viewing: &list::viewing(&state.db, viewer).await?,
        },
    )
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::Orientation;

    fn build(id: &str, authors: &[&str], description: Option<&str>) -> Build {
        let mut s = crate::search::tests::schematic(
            "adder",
            Orientation::Horizontal,
            &[],
        );
        s.id = id.to_owned();
        s.credits.authors = authors.iter().map(|&a| a.to_owned()).collect();
        s.description = description.map(str::to_owned);
        s
    }

    fn shown(lists: &[List]) -> Vec<(&str, Vec<&str>)> {
        lists
            .iter()
            .map(|list| {
                let ids = list.builds.iter().map(|s| s.id.as_str()).collect();
                (list.name, ids)
            })
            .collect()
    }

    /// A player with no account has the builds crediting them, whatever
    /// the case of their name, and those only naming them; with nothing
    /// uploaded, private, or favourited to list.
    #[test]
    fn a_player_lists_their_credits_and_mentions() {
        let all = [
            build("a", &["Ada"], None),
            build("b", &["Bob"], Some("after @ada's design")),
            build("c", &["ada", "Bob"], Some("by @Ada")),
            build("d", &["Bob"], None),
        ];
        let who = Person::Player {
            name: "Ada",
            account: None,
        };
        assert_eq!(
            shown(&who.lists(&all, None)),
            [
                ("all", vec!["a", "c"]),
                ("built", vec!["a", "c"]),
                ("contributed", vec![]),
                ("mentioned", vec!["b"]),
            ]
        );
    }

    /// The list an address names stays named when the address moves.
    #[test]
    fn a_list_follows_a_page_to_its_new_address() {
        assert_eq!(showing("/players/Ada", None), "/players/Ada");
        assert_eq!(
            showing("/players/Ada", Some("built")),
            "/players/Ada?show=built"
        );
    }
}
