//! What each of a build's lines is for, its role, by the blocks that
//! carry it: each input block, an operand's bit or a control line, and
//! each output block, a bit of an output bus or a status line. Probing
//! finds the roles by how a build behaves, and the site keeps them with
//! each version of its file, in the database, never in its blocks, which
//! stay as their author made them.
//!
//! A file being worked on carries its roles with it, in its metadata, so
//! every way of changing it keeps them where its blocks go: fitting its
//! box, turning it, and stacking copies move them, and a block taken out
//! takes its role with it. A file is stored, hashed, and downloaded
//! without them; see [`without`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Pos;

/// What a line is for.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Role {
    /// An operand, by its place: 0 for A, 1 for B, 2 for C, a fused
    /// multiply-add's addend, and on.
    Operand(u8),
    /// A control line, an input named on its own, such as a carry in.
    Control,
    /// An output bus, by its place among them, the lowest bits first.
    Output(u8),
    /// A status line, an output named on its own, such as a carry out.
    Status,
}

impl Role {
    /// The role as it is kept: `A`, `B`, and on for an operand, `control`,
    /// `output`, `output 2`, and on for an output bus, and `status`.
    pub fn key(self) -> String {
        match self {
            Role::Operand(n) => char::from(b'A'.saturating_add(n)).to_string(),
            Role::Control => "control".to_owned(),
            Role::Output(0) => "output".to_owned(),
            Role::Output(n) => format!("output {}", u16::from(n) + 1),
            Role::Status => "status".to_owned(),
        }
    }

    /// The role kept as `key`; see [`Role::key`].
    pub fn from_key(key: &str) -> Option<Role> {
        Some(match key {
            "control" => Role::Control,
            "output" => Role::Output(0),
            "status" => Role::Status,
            _ => {
                if let Some(n) = key.strip_prefix("output ") {
                    let n: u8 = n.parse().ok()?;
                    Role::Output(n.checked_sub(1)?)
                } else if let [letter @ b'A'..=b'Z'] = key.as_bytes() {
                    Role::Operand(letter - b'A')
                } else {
                    return None;
                }
            }
        })
    }

    /// Whether it is an input's role.
    pub fn input(self) -> bool {
        matches!(self, Role::Operand(_) | Role::Control)
    }

    /// Whether it is one of a bus's bits, an operand's or an output's,
    /// rather than a line of its own.
    pub fn bus(self) -> bool {
        matches!(self, Role::Operand(_) | Role::Output(_))
    }
}

impl Serialize for Role {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.key())
    }
}

impl<'de> Deserialize<'de> for Role {
    fn deserialize<D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<Self, D::Error> {
        let key = String::deserialize(d)?;
        Role::from_key(&key).ok_or_else(|| {
            serde::de::Error::custom(format!("no such role: {key}"))
        })
    }
}

/// The role of each block that has one, by position: an input's block, a
/// lever's or the block under a signal-strength input's comparator, or an
/// output's, the block under its repeater or comparator.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Vec<(Pos, Role)>", into = "Vec<(Pos, Role)>")]
pub struct Roles(pub BTreeMap<Pos, Role>);

impl From<Vec<(Pos, Role)>> for Roles {
    fn from(pairs: Vec<(Pos, Role)>) -> Self {
        Roles(pairs.into_iter().collect())
    }
}

impl From<Roles> for Vec<(Pos, Role)> {
    fn from(roles: Roles) -> Self {
        roles.0.into_iter().collect()
    }
}

impl Roles {
    /// The role of the block at `pos`, if it has one.
    pub fn get(&self, pos: Pos) -> Option<Role> {
        self.0.get(&pos).copied()
    }

    /// Gives each of `blocks` `role`, a later mark taking a block from an
    /// earlier one.
    pub fn mark<'p>(
        &mut self,
        role: Role,
        blocks: impl IntoIterator<Item = &'p Pos>,
    ) {
        self.0.extend(blocks.into_iter().map(|&pos| (pos, role)));
    }

    /// Gives each line of `lines` an output's role, the first the lowest
    /// bits.
    pub fn outputs<'p>(&mut self, lines: impl IntoIterator<Item = &'p [Pos]>) {
        for (bits, n) in lines.into_iter().zip(0..=u8::MAX) {
            self.mark(Role::Output(n), bits);
        }
    }

    /// The roles with each block moved by `to`, those it sends nowhere
    /// left out.
    #[must_use]
    pub fn moved(&self, to: impl Fn(Pos) -> Option<Pos>) -> Roles {
        Roles(
            self.0
                .iter()
                .filter_map(|(&pos, &role)| Some((to(pos)?, role)))
                .collect(),
        )
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The key under a file's `Metadata` its roles go by while it is worked
/// on.
pub(super) const KEY: &str = "RedstoneDBRoles";

/// The file in `data` carrying `roles`, or none with `None`, in place of
/// any it carried.
pub fn with(data: &[u8], roles: Option<&Roles>) -> anyhow::Result<Vec<u8>> {
    let mut nbt = super::decode(data)?;
    if nbt.roles().as_ref() == roles {
        return Ok(data.to_vec());
    }
    nbt.set_roles(roles);
    super::encode(nbt)
}

/// The file in `data` without the roles it carries, as it is stored,
/// hashed, and downloaded.
pub fn without(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    with(data, None)
}

/// The roles the file in `data` carries, if any.
pub fn of(data: &[u8]) -> anyhow::Result<Option<Roles>> {
    Ok(super::decode(data)?.roles())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_role_is_kept_by_a_key_it_is_read_back_from() {
        for role in [
            Role::Operand(0),
            Role::Operand(3),
            Role::Control,
            Role::Output(0),
            Role::Output(2),
            Role::Status,
        ] {
            assert_eq!(Role::from_key(&role.key()), Some(role));
        }
        assert_eq!(Role::Operand(1).key(), "B");
        assert_eq!(Role::Output(1).key(), "output 2");
        assert_eq!(Role::from_key("output 0"), None);
    }
}
