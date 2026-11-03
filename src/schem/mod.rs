//! Reading and writing Sponge `.schem` files, versions 2 and 3: a build's
//! blocks, its signs' text, and where `//paste` puts it. Its submodules
//! shape a build's blocks: turning it, fitting its box, stacking copies, and
//! dating the oldest release it works in.

pub mod block;
pub mod dust;
pub mod orient;
pub mod rstack;
pub mod stack;
pub mod standardize;
pub mod works_from;

mod blocks;
mod direction;
mod entity;
mod nbt;

pub use blocks::{
    Blocks, Switch, as_placed, id, property, short, torch_block, unpowered,
    with_property,
};
pub use direction::{Direction, add, step};
pub use entity::{
    barrel_redstone, container_strength, entity_pos, plain_sign, slots,
    with_output_signal, with_pos,
};
pub use nbt::{Nbt, Size, decode, encode, paste_from_corner};

pub const AIR: &str = "minecraft:air";

/// A block position, as x, y, z.
pub type Pos = [i32; 3];
