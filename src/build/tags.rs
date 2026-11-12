//! A build's tags as its page shows them: those stored with it, and those
//! worked out from its design, orientation, flags, stacking, and timing.

use super::{Build, classes};
use crate::{
    category::Flag,
    models::tag::{self, Tag},
};

impl Build {
    pub(super) fn tags_in(
        &self,
        kind: tag::Kind,
    ) -> impl Iterator<Item = &Tag> {
        self.tags.iter().filter(move |tag| tag.kind == kind)
    }

    /// Its tags saying how far apart its copies add words on its shared
    /// data bus, as `3-wide_word_stackable` west or
    /// `4-high_word_stackable` up, each of `kind`, where the simulation
    /// found it: copies across its bits, with no pitch, as a stacking its
    /// author gave has none either.
    fn word_tags(&self, kind: tag::Kind) -> Vec<Tag> {
        let decided = self.conventional
            && crate::category::behaviour(&self.category).stacking
                == crate::category::Stacking::Inferred;
        self.stacking
            .iter()
            .filter(|way| {
                decided
                    && way.pitch.is_none()
                    && !way.adds_bits(self.specs.orientation)
            })
            .filter_map(|way| {
                let shape = way.pitch_shape()?;
                Some(Tag {
                    name: tag::word_stackable(&format!(
                        "{}-{shape}",
                        way.west + way.up
                    )),
                    token: None,
                    kind,
                })
            })
            .collect()
    }

    /// Its tags with its design, orientation, and flags among them, as its page
    /// shows them and a list is narrowed by: `failing` first, then
    /// its design, its orientation, its flags in its category's order,
    /// `stackable` when its copies stack, `synced` when its outputs settle
    /// together or `unsynced` when they don't, and its other tags. One the site
    /// works out from a conventional build is found, as its category's test or
    /// file sets it; one a person gives is of kind other.
    pub fn every_tag(&self) -> Vec<Tag> {
        let behaviour = crate::category::behaviour(&self.category);
        let found = |kind, by_site: bool| {
            if self.conventional && self.unverified.is_none() && by_site {
                kind
            } else {
                tag::Kind::Other
            }
        };
        let design = self.specs.design.as_ref().map(|design| Tag {
            name: design.clone(),
            token: self
                .listed_design()
                .and_then(|listed| listed.abbreviation)
                .map(str::to_owned),
            kind: found(tag::Kind::Design, behaviour.evaluated),
        });
        let orientation = Tag {
            name: self.specs.orientation.label().to_lowercase(),
            token: Some(self.specs.orientation.name().to_owned()),
            kind: found(
                tag::Kind::Orientation,
                behaviour.evaluated
                    || behaviour.reads != crate::category::Reads::Nothing,
            ),
        };
        // An inverted carry in says only how one line is read, as the
        // inputs table shows.
        let flags = self
            .flags
            .iter()
            .filter(|flag| flag.name != Flag::INVERTED_CARRY_IN)
            .map(|flag| Tag {
                name: flag.name.to_owned(),
                token: None,
                kind: found(
                    tag::Kind::Feature,
                    behaviour.evaluated || behaviour.flags_from_file,
                ),
            });
        let stackable = (!self.stacking.is_empty()).then(|| Tag {
            name: tag::STACKABLE.to_owned(),
            token: None,
            kind: found(
                tag::Kind::Feature,
                behaviour.stacking == crate::category::Stacking::Inferred,
            ),
        });
        // The pitch class of copies that add bits, the blocks a bit apart
        // they stack, as `2-wide_stackable` west or `2-high_stackable` up,
        // and each looser class, which a tighter pitch counts in too; or
        // the pitch of copies end to end, as `7-wide_stackable`, alone.
        let mut pitches: Vec<Tag> = Vec::new();
        for way in &self.stacking {
            let (Some(pitch), Some(shape)) = (way.pitch, way.pitch_shape())
            else {
                continue;
            };
            for class in classes(pitch) {
                let name = tag::pitch_stackable(&format!("{class}-{shape}"));
                if pitches.iter().any(|tag| tag.name == name) {
                    continue;
                }
                pitches.push(Tag {
                    name,
                    token: None,
                    kind: found(
                        tag::Kind::Feature,
                        behaviour.stacking
                            == crate::category::Stacking::Inferred,
                    ),
                });
            }
        }
        let unaligned = self.unaligned.then(|| Tag {
            name: tag::UNALIGNED.to_owned(),
            token: None,
            kind: found(tag::Kind::Feature, true),
        });
        let interleaved = self.interleaved_inputs().then(|| Tag {
            name: tag::INTERLEAVED_INPUTS.to_owned(),
            token: None,
            kind: found(tag::Kind::Feature, true),
        });
        // Whether its outputs settle together, once measured, either way.
        let synced = self.specs.synchronized.map(|synced| Tag {
            name: if synced { tag::SYNCED } else { tag::UNSYNCED }.to_owned(),
            token: None,
            kind: found(tag::Kind::Feature, behaviour.evaluated),
        });
        let (failing, rest): (Vec<&Tag>, Vec<&Tag>) =
            self.tags.iter().partition(|tag| tag.is_unverified());
        failing
            .into_iter()
            .cloned()
            .chain(design)
            .chain([orientation])
            .chain(flags)
            .chain(stackable)
            .chain(pitches)
            .chain(self.word_tags(found(tag::Kind::Feature, true)))
            .chain(interleaved)
            .chain(unaligned)
            .chain(synced)
            .chain(rest.into_iter().cloned())
            .collect()
    }
}
