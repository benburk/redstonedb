use std::{collections::HashMap, io::Read};

use anyhow::{Context, Result, bail};
use fastnbt::Value;
use flate2::read::GzDecoder;

/// Bounding box of a Sponge schematic, in blocks along x, y, z.
pub struct Size {
    pub x: u16,
    pub y: u16,
    pub z: u16,
}

/// Reads the dimensions from a gzipped Sponge `.schem` file. Version 2 keeps
/// them at the root; version 3 nests them in a `Schematic` compound.
pub fn size(data: &[u8]) -> Result<Size> {
    let mut raw = Vec::new();
    GzDecoder::new(data)
        .read_to_end(&mut raw)
        .context("not a gzipped schematic")?;
    let Value::Compound(root) = fastnbt::from_bytes::<Value>(&raw)? else {
        bail!("schematic root is not an NBT compound");
    };
    if root.contains_key("Materials") {
        bail!("legacy MCEdit .schematic files are not supported");
    }
    let body = match root.get("Schematic") {
        Some(Value::Compound(nested)) => nested,
        _ => &root,
    };
    Ok(Size {
        x: dimension(body, "Width")?,
        y: dimension(body, "Height")?,
        z: dimension(body, "Length")?,
    })
}

// Sponge stores dimensions as unsigned shorts in NBT's signed short type.
#[allow(clippy::cast_sign_loss)]
fn dimension(body: &HashMap<String, Value>, key: &str) -> Result<u16> {
    match body.get(key) {
        Some(Value::Short(n)) => Ok(*n as u16),
        _ => bail!("schematic has no {key}"),
    }
}
