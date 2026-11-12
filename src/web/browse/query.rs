//! What a listing's address asks for: the category, the search, the tags,
//! the order, and the layout, read from its query string and written back.

use crate::build::tag;

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
/// them, as in `?category=adder,subtractor`. `tag` may repeat, each needing
/// a match: `?tag=alpha&tag=torchless` is both, and `?tag=alpha,torchless`
/// either.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct HomeQuery {
    pub view: View,
    pub sort: Sort,
    /// A search such as `register vertical alpha -torchless`.
    pub q: String,
    /// The chosen category, such as `adder`; empty for all.
    pub category: String,
    /// Tag filters, each needing a match; commas within one give choices.
    pub tags: Vec<String>,
    /// Whether builds not verified as their category are listed too.
    pub unverified: bool,
    /// Whether unaligned builds, whose buses aren't laid as the
    /// conventions lay them, are left out; verified, they list as any
    /// other unless hidden.
    pub hide_unaligned: bool,
}

impl HomeQuery {
    /// The query with its tag filter `index` taken out.
    pub(crate) fn without_tag(&self, index: usize) -> Self {
        let mut rest = self.clone();
        rest.tags.remove(index);
        rest
    }

    /// The query string `raw`, without its `?`. Asking for the unverified
    /// by their tag shows them.
    pub fn parse(raw: &str) -> Self {
        let mut query = HomeQuery::default();
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
                "category" => query.category = value,
                "sort" => query.sort = Sort::parse(&value),
                "tag"
                    if [tag::FAILING, tag::UNTESTED]
                        .contains(&tag::name(&value).as_str()) =>
                {
                    query.unverified = true;
                    query.tags.push(value);
                }
                "tag" if !value.is_empty() => query.tags.push(value),
                "unverified" => query.unverified = value == "show",
                "unaligned" => query.hide_unaligned = value == "hide",
                _ => {}
            }
        }
        query
    }

    /// Whether it searches or chooses anything. With nothing to narrow it,
    /// the home page shows its index, so every build has an address of its
    /// own, `/builds`.
    pub fn narrows(&self) -> bool {
        !self.q.is_empty() || !self.category.is_empty() || !self.tags.is_empty()
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
        if !self.category.is_empty() {
            params.append_pair("category", &self.category);
        }
        for tag in &self.tags {
            params.append_pair("tag", tag);
        }
        if self.unverified {
            params.append_pair("unverified", "show");
        }
        if self.hide_unaligned {
            params.append_pair("unaligned", "hide");
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
            "/?view=list&sort=-downloads%2Cvolume&category=adder&tag=alpha%2Ctorchless&tag=synced&unverified=show&q=8b+adder",
        ] {
            let raw = href.split_once('?').map_or("", |(_, raw)| raw);
            assert_eq!(HomeQuery::parse(raw).href(), href);
        }
        // Unaligned builds list unless the switch hides them.
        let hidden = HomeQuery::parse("category=adder&unaligned=hide");
        assert!(hidden.hide_unaligned);
        assert_eq!(hidden.href(), "/?category=adder&unaligned=hide");
        assert!(!HomeQuery::parse("category=adder").hide_unaligned);
        let query = HomeQuery::parse("q=add&sort=bogus,-latency");
        assert_eq!(query.href(), "/?sort=-latency&q=add");
        assert!(!HomeQuery::parse("view=list").narrows());
    }
}
