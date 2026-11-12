//! Sponge `.schem` files, versions 2 and 3, and the blocks in them: reading
//! and writing a build's blocks, its signs' text, and where `//paste` puts
//! it; what each block is to redstone; and changing a file's blocks,
//! turning it, and fitting its box. Nothing here knows what a build is
//! for: that begins with its inputs and outputs, in `recognize`.

pub mod block;
pub mod dust;
pub mod edit;
pub mod lines;
pub mod turn;

mod blocks;
mod direction;
mod entity;
mod nbt;

pub use blocks::{
    Blocks, Switch, as_placed, id, property, short, torch_block, unpowered,
    with_property,
};
pub use direction::{Direction, add, step};
pub use edit::{edit, edit_grown, edit_grown_moving, for_viewer, rebox};
pub use entity::{
    barrel_redstone, container_strength, entity_pos, plain_sign, slots,
    with_output_signal, with_pos,
};
pub use nbt::{Nbt, Size, decode, encode, paste_from_corner};

pub const AIR: &str = "minecraft:air";

/// A block position, as x, y, z.
pub type Pos = [i32; 3];
