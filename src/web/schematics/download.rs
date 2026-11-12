//! A build's file to download: in the colours asked for, as one of the
//! parts the stacking procedure pastes, stacked, or with stacking markers.

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::Method,
    response::Response,
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    models::{
        schematic::{self, Build, DownloadKind},
        user::User,
        version,
    },
    recognize::palette::{self, Palette},
    schem::{stack, standardize},
    web::{app_state::AppState, error::AppError, settings::PaletteFields},
};

/// What a download of a build's file asks for: from the download dialog,
/// the stacking guide's links, and the 3D viewer.
#[derive(Deserialize)]
pub(in crate::web) struct Download {
    /// Set when the 3D viewer fetches the file, which is no download.
    preview: Option<String>,
    /// The direction, by name, to put glass at two opposite corners of
    /// the box `//rstack` stacks, for `WorldEdit` to select it by.
    markers: Option<String>,
    /// One of the files the in-game stacking procedure pastes, for the
    /// `direction` it stacks.
    part: Option<Part>,
    direction: Option<String>,
    /// Copies each way the build stacks.
    west: Option<usize>,
    up: Option<usize>,
    #[serde(rename = "west-up")]
    west_up: Option<usize>,
    /// The colours to show it in: the site's own, its author's, the
    /// viewer's theme, which is what shows when none is asked for, or a
    /// palette of the download's own, its fields sent beside these.
    pub(super) colours: Option<Colours>,
    /// The file's name, less `.schem`, when it is to be named otherwise
    /// than the build is.
    name: Option<String>,
}

/// The colours a build is shown and downloaded in.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Colours {
    /// The site's own: its roles in the site's colours and, for a
    /// recognized build, its building blocks white.
    Default,
    /// The colours its author gave it, as stored.
    Original,
    /// The viewer's theme.
    Theme,
    /// A palette made for one download, kept nowhere.
    Custom,
}

impl Colours {
    /// Its name in an address, such as `original`.
    pub fn name(self) -> &'static str {
        match self {
            Colours::Default => "default",
            Colours::Original => "original",
            Colours::Theme => "theme",
            Colours::Custom => "custom",
        }
    }
}

/// A file the in-game stacking procedure pastes.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Part {
    /// The build without the parts only its first copy keeps.
    Without,
    /// Those parts alone.
    First,
}

impl Download {
    /// The copies asked for each way, by direction name, leaving out one
    /// copy, which is the build itself.
    fn copies(&self) -> HashMap<String, usize> {
        [
            ("west", self.west),
            ("up", self.up),
            ("west-up", self.west_up),
        ]
        .into_iter()
        .filter_map(|(name, copies)| Some((name.to_owned(), copies?)))
        .filter(|&(_, copies)| copies != 1)
        .collect()
    }

    /// The palette to colour the file in, if any: for custom colours, the
    /// one `fields` make, refused unless every block is one a palette may
    /// use and its roles can be told apart; else, or with none given,
    /// `theme`, the viewer's.
    pub(in crate::web) fn palette(
        &self,
        fields: PaletteFields,
        theme: Option<Palette>,
    ) -> Result<Option<Palette>, AppError> {
        // Custom with no fields, as when its editor never came in, is
        // what that editor starts from.
        if self.colours != Some(Colours::Custom) || !fields.given() {
            return Ok(theme);
        }
        let custom = fields.palette();
        if !custom.known() {
            return Err(AppError::BadRequest(
                "A custom colour isn't a block a theme may use.".to_owned(),
            ));
        }
        Ok(Some(custom))
    }

    /// The name, less `.schem`, given in place of `filename`, the build's
    /// own: `None` when none is given, or it is the build's own, so a
    /// stack keeps the name it is given for its width.
    fn renamed(&self, filename: &str) -> Option<String> {
        let stem = schematic::file_stem(self.name.as_deref()?);
        let own = filename.trim_end_matches(".schem");
        (!stem.is_empty() && stem != own).then_some(stem)
    }
}

/// The stored `.schem` file, named by the naming scheme. The viewer fetches
/// the same URL.
pub async fn download(
    Path(id): Path<String>,
    State(state): State<AppState>,
    viewer: Option<User>,
    method: Method,
    Query(asked): Query<Download>,
    Query(fields): Query<PaletteFields>,
) -> Result<Response, AppError> {
    // The 3D viewer's fetch, and a HEAD request, is no download. Anything
    // else takes an account.
    let preview = asked.preview.is_some();
    if !preview && viewer.is_none() {
        return Err(AppError::SignedOut);
    }
    let counted = !preview && method == Method::GET;
    let theme = match &viewer {
        Some(user) => palette::of(&state.db, user.id).await?,
        None => None,
    };
    let palette = asked.palette(fields, theme)?;
    let (filename, data) =
        export(&state.db, &id, &asked, counted, palette.as_ref()).await?;
    if let (true, Some(user)) = (counted, &viewer) {
        let kind = if asked.part.is_some() {
            DownloadKind::Part
        } else {
            DownloadKind::File
        };
        schematic::log_download(&state.db, &id, user.id, None, kind).await?;
    }
    // Only readable files are stored, so the viewer's copy falls back to
    // the file as it is only if that changes.
    let data = if preview {
        standardize::for_viewer(&data).unwrap_or(data)
    } else {
        data
    };
    Ok(super::super::attachment(&filename, data))
}

/// Build `id`'s file as `asked`, with its name, counted as a download if
/// `counted`, in `palette` unless another is asked for, without the roles
/// it carries; a 404 for no build, or a deleted one.
pub(in crate::web) async fn export(
    db: &SqlitePool,
    id: &str,
    asked: &Download,
    counted: bool,
    palette: Option<&Palette>,
) -> Result<(String, Vec<u8>), AppError> {
    let s = schematic::get(db, id).await?.ok_or(AppError::NotFound)?;
    let data = version::current_file(db, id)
        .await?
        .ok_or(AppError::NotFound)?;
    // In its author's colours, as stored, with any kept from before the
    // site kept them, which go on its blocks before any stacking; and so
    // always for a build the site can't read, its file carrying no roles.
    if asked.colours == Some(Colours::Original)
        || !crate::models::simulation::carries_roles(&data)
    {
        let data = match version::colours(db, id).await? {
            Some(colours) => {
                crate::recognize::colouring::apply(&data, &colours)?
            }
            None => data,
        };
        let (filename, data) = shaped(db, id, s, data, asked, counted).await?;
        return Ok((filename, crate::schem::lines::without(&data)?));
    }
    // The site's own palette, unless the viewer's theme or a custom one is
    // asked for.
    let palette = match asked.colours {
        Some(Colours::Default) => None,
        None | Some(Colours::Theme | Colours::Custom) => palette.cloned(),
        Some(Colours::Original) => unreachable!("downloaded above"),
    }
    .unwrap_or_default();
    // Its building blocks before any stacking, by position; its roles'
    // colours after, as stacked copies carry their roles.
    let needs = palette::needs(db, id).await?;
    let data = match &needs {
        Some(needs) => palette::blocks_in(&data, needs, &palette)?,
        None => data,
    };
    let (filename, data) = shaped(db, id, s, data, asked, counted).await?;
    let data = palette::colour(&data, &palette)?;
    Ok((filename, crate::schem::lines::without(&data)?))
}

/// Build `s`'s file `data` as `asked`: a part, copies stacked, or as it is,
/// with stacking markers if asked; counted as a download if `counted`.
async fn shaped(
    db: &SqlitePool,
    id: &str,
    s: Build,
    data: Vec<u8>,
    asked: &Download,
    counted: bool,
) -> Result<(String, Vec<u8>), AppError> {
    let renamed = asked.renamed(&s.filename());
    if let Some(part) = asked.part {
        let direction = s
            .stacking
            .iter()
            .find(|d| asked.direction.as_deref() == Some(d.name().as_str()))
            .ok_or_else(|| AppError::BadRequest("no such part".to_owned()))?;
        let first = part == Part::First;
        let data = stack::part(&data, *direction, first)?;
        let suffix = if first {
            "_first_copy_parts"
        } else {
            "_without_first_copy_parts"
        };
        let stem = renamed.unwrap_or_else(|| {
            s.filename().trim_end_matches(".schem").to_owned()
        });
        return Ok((format!("{stem}{suffix}.schem"), data));
    }
    let copies = asked.copies();
    let markers = match asked.markers.as_deref() {
        None | Some("") => None,
        Some(name) => Some(
            s.stacking
                .iter()
                .copied()
                .find(|d| d.name() == name)
                .ok_or_else(|| {
                    AppError::BadRequest("no such direction".to_owned())
                })?,
        ),
    };
    if markers.is_some() && !copies.is_empty() {
        return Err(AppError::BadRequest(
            "Stacking markers go on a single copy.".to_owned(),
        ));
    }
    let (data, s) = if copies.is_empty() {
        (data, s)
    } else {
        let stacked = stack::plan(&s.stacking, &copies).and_then(|plan| {
            let buses = stack::Buses::of(s.kind(), s.specs.orientation);
            let (data, size) = stack::stack(&data, &plan, buses)?;
            Ok((data, s.stacked(&plan, size)))
        });
        stacked.map_err(|err| AppError::BadRequest(err.to_string()))?
    };
    if counted {
        schematic::count_download(db, id).await?;
    }
    let data = match markers {
        Some(direction) => {
            stack::with_markers(&data, direction)?.ok_or_else(|| {
                AppError::BadRequest(
                "Every pair of opposite corners of its box holds a block, so \
                 there's nowhere to put the glass."
                    .to_owned(),
            )
            })?
        }
        None => data,
    };
    let filename =
        renamed.map_or_else(|| s.filename(), |stem| format!("{stem}.schem"));
    Ok((filename, data))
}
