//! What a listing's address asks for: the category, the search, the tags,
//! the order, and the layout, read from its query string and written back.

use crate::{
    build::tag,
    screen::{self, Field, Picks},
};

/// How the home page lays out its schematics: cards, or `view=list` in its
/// address for a table.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum View {
    #[default]
    Cards,
    List,
}

/// A field the home page can order builds by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortField {
    AddedAt,
    Volume,
    Latency,
    Downloads,
}

impl SortField {
    const ALL: [SortField; 4] = [
        SortField::AddedAt,
        SortField::Volume,
        SortField::Latency,
        SortField::Downloads,
    ];

    /// As an address names it.
    fn name(self) -> &'static str {
        match self {
            SortField::AddedAt => "added_at",
            SortField::Volume => "volume",
            SortField::Latency => "latency",
            SortField::Downloads => "downloads",
        }
    }
}

/// The order the home page lists builds in: fields, each ascending or
/// descending, the first deciding and each after it breaking the ties
/// before. Its address writes it as `sort=-downloads,volume`, `-` for
/// descending. With no fields, or on a tie, builds go newest first.
#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub struct Sort(pub Vec<(SortField, bool)>);

impl Sort {
    /// `text` as an address gives it, leaving out any field it does not
    /// know.
    pub fn parse(text: &str) -> Self {
        Sort(
            text.split(',')
                .map(str::trim)
                .filter_map(|key| {
                    let (name, descending) = key
                        .strip_prefix('-')
                        .map_or((key, false), |name| (name, true));
                    let field = SortField::ALL
                        .into_iter()
                        .find(|field| field.name() == name)?;
                    Some((field, descending))
                })
                .collect(),
        )
    }

    /// As an address writes it, such as `-downloads,volume`.
    pub fn name(&self) -> String {
        self.0
            .iter()
            .map(|&(field, descending)| {
                if descending {
                    format!("-{}", field.name())
                } else {
                    field.name().to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// What the home page's address asks for, and where its links lead.
/// Separate parameters narrow together; within one, commas mean any of
/// them, as in `?category=adder,subtractor&width=8,16`; see
/// [`crate::screen`]. `tag` may repeat, each needing a match:
/// `?tag=alpha&tag=torchless` is both, and `?tag=alpha,torchless` either.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct HomeQuery {
    pub view: View,
    pub sort: Sort,
    /// A search such as `register vertical alpha -torchless`.
    pub q: String,
    /// What is picked of each field the builds are screened by, such as
    /// their category, width, and standing.
    pub picks: Picks,
    /// Tag filters, each needing a match; commas within one give choices.
    pub tags: Vec<String>,
}

impl HomeQuery {
    /// The query for the working builds of `category`.
    pub(crate) fn in_category(category: &str) -> Self {
        let mut query = HomeQuery::default();
        query.picks.set(Field::Category, category);
        query
    }

    /// The category picked, when it is one alone.
    pub(crate) fn category(&self) -> Option<&str> {
        match self.picks.of(Field::Category)[..] {
            [category] => Some(category),
            _ => None,
        }
    }

    /// The query with its tag filter `index` taken out.
    pub(crate) fn without_tag(&self, index: usize) -> Self {
        let mut rest = self.clone();
        rest.tags.remove(index);
        rest
    }

    /// The query with `picks` in place of its own.
    pub(crate) fn picking(&self, picks: Picks) -> Self {
        HomeQuery {
            picks,
            ..self.clone()
        }
    }

    /// The fields picked of with their picks as an address writes them,
    /// such as `("width", "8,16")`.
    pub(crate) fn picked(&self) -> Vec<(&'static str, String)> {
        self.picks
            .given()
            .map(|(field, picks)| (field.name(), picks.join(",")))
            .collect()
    }

    /// The query string `raw`, without its `?`. Asking for the unverified
    /// by their tag shows them, as `unverified=show` did before the status
    /// was picked, and `unaligned=hide` picks the aligned.
    pub fn parse(raw: &str) -> Self {
        let mut query = HomeQuery::default();
        let mut unverified = false;
        for (key, value) in form_urlencoded::parse(raw.as_bytes()) {
            let value = value.trim().to_owned();
            match key.as_ref() {
                "view" => {
                    query.view = if value == "list" {
                        View::List
                    } else {
                        View::Cards
                    };
                }
                "q" => query.q = value,
                "sort" => query.sort = Sort::parse(&value),
                "tag"
                    if [tag::FAILING, tag::UNTESTED]
                        .contains(&tag::name(&value).as_str()) =>
                {
                    unverified = true;
                    query.tags.push(value);
                }
                "tag" if !value.is_empty() => query.tags.push(value),
                "tag" => {}
                "unverified" => unverified |= value == "show",
                "unaligned" if value == "hide" => {
                    query.picks.set(Field::Buses, "aligned");
                }
                name => {
                    if let Some(field) = Field::named(name) {
                        query.picks.set(field, &value);
                    }
                }
            }
        }
        if unverified && query.picks.given().all(|(f, _)| f != Field::Status) {
            query.picks.set(Field::Status, screen::ALL);
        }
        query
    }

    /// Whether it searches or chooses anything. With nothing to narrow it,
    /// the home page shows its index, so every build has an address of its
    /// own, `/builds`.
    pub fn narrows(&self) -> bool {
        !self.q.is_empty() || !self.picks.is_empty() || !self.tags.is_empty()
    }

    /// Its address, such as `/?category=adder&q=8b`, as
    /// [`HomeQuery::parse`] reads it.
    pub fn href(&self) -> String {
        let mut params = form_urlencoded::Serializer::new(String::new());
        if self.view == View::List {
            params.append_pair("view", "list");
        }
        let sort = self.sort.name();
        if !sort.is_empty() {
            params.append_pair("sort", &sort);
        }
        // The category first, as addresses had it before the other
        // fields.
        let picked = self.picked();
        let category = Field::Category.name();
        for (name, picks) in picked.iter().filter(|(name, _)| *name == category)
        {
            params.append_pair(name, picks);
        }
        for tag in &self.tags {
            params.append_pair("tag", tag);
        }
        for (name, picks) in picked.iter().filter(|(name, _)| *name != category)
        {
            params.append_pair(name, picks);
        }
        if !self.q.is_empty() {
            params.append_pair("q", &self.q);
        }
        let path = if self.narrows() { "/" } else { "/builds" };
        let params = params.finish();
        if params.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{params}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An address reads back as it was written, but for a sort field the
    /// site does not know, which is left out.
    #[test]
    fn a_home_address_reads_back_as_written() {
        for href in [
            "/builds",
            "/builds?view=list&sort=volume",
            "/?category=adder",
            "/?view=list&sort=-downloads%2Cvolume&category=adder%2Csubtractor&tag=alpha%2Ctorchless&tag=synced&status=all&width=8%2C16&latency=..3&by=embizone&q=8b+adder",
        ] {
            let raw = href.split_once('?').map_or("", |(_, raw)| raw);
            assert_eq!(HomeQuery::parse(raw).href(), href);
        }
        // The switches the listing had before its status and buses were
        // picked lead where they did.
        let hidden = HomeQuery::parse("category=adder&unaligned=hide");
        assert_eq!(hidden.href(), "/?category=adder&buses=aligned");
        let shown = HomeQuery::parse("category=adder&unverified=show");
        assert_eq!(shown.href(), "/?category=adder&status=all");
        let failing = HomeQuery::parse("tag=failing");
        assert_eq!(failing.href(), "/?tag=failing&status=all");
        let query = HomeQuery::parse("q=add&sort=bogus,-latency");
        assert_eq!(query.href(), "/?sort=-latency&q=add");
        assert!(!HomeQuery::parse("view=list").narrows());
    }
}
