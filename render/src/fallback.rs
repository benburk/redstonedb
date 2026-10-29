//! What isn't in the game's models: the tints faces are multiplied by, the
//! blocks drawn as nothing, and the flat colour of a block whose model or
//! textures can't be found, so it still shows.

/// Whether block `name`, without `minecraft:`, is drawn as nothing: air,
/// and blocks that only the game's block entities draw, such as signs.
pub fn invisible(name: &str) -> bool {
    matches!(
        name,
        "air"
            | "cave_air"
            | "void_air"
            | "barrier"
            | "light"
            | "structure_void"
    ) || name.contains("sign")
}

/// What block `name`'s tinted faces are multiplied by: dust's red by its
/// `power`, as the game works it out, and plains' colours for grass,
/// leaves, and water.
pub fn tint(name: &str, power: Option<&str>) -> [f32; 3] {
    match name {
        "redstone_wire" => {
            let f =
                power.and_then(|p| p.parse::<f32>().ok()).unwrap_or(0.0) / 15.0;
            let r = if f == 0.0 { 0.3 } else { f * 0.6 + 0.4 };
            [
                r,
                (f * f * 0.7 - 0.5).max(0.0),
                (f * f * 0.6 - 0.7).max(0.0),
            ]
        }
        "water" | "bubble_column" => rgb([0x3F, 0x76, 0xE4]),
        "spruce_leaves" => rgb([0x61, 0x99, 0x61]),
        "birch_leaves" => rgb([0x80, 0xA7, 0x55]),
        "lily_pad" => rgb([0x20, 0x80, 0x30]),
        _ if name.contains("leaves") || name == "vine" => {
            rgb([0x77, 0xAB, 0x2F])
        }
        _ if name.contains("grass")
            || name.ends_with("fern")
            || name == "sugar_cane" =>
        {
            rgb([0x91, 0xBD, 0x59])
        }
        _ => [1.0; 3],
    }
}

fn rgb(bytes: [u8; 3]) -> [f32; 3] {
    bytes.map(|c| f32::from(c) / 255.0)
}

/// A block's flat colour, by its name without `minecraft:`: stained glass
/// see-through in its dye's colour, concrete in its own, and a few others.
pub fn color(name: &str, lit: bool) -> [u8; 4] {
    // A slab is drawn in its block's colour.
    let name = match name {
        "quartz_slab" | "smooth_quartz_slab" => "quartz_block",
        "smooth_stone_slab" => "smooth_stone",
        "stone_slab" => "stone",
        "stone_brick_slab" => "stone_bricks",
        "oak_slab" => "oak_planks",
        "spruce_slab" => "spruce_planks",
        "bamboo_mosaic_slab" => "bamboo_mosaic",
        name => name,
    };
    if let Some(dye) = name.strip_suffix("_stained_glass") {
        let [r, g, b, _] = dye_color(dye);
        return [r, g, b, 110];
    }
    if let Some(dye) = name.strip_suffix("_concrete") {
        return dye_color(dye);
    }
    // Wool is a little lighter than concrete, terracotta duller.
    if let Some(dye) = name.strip_suffix("_wool") {
        return shade(dye_color(dye), 1.15);
    }
    if let Some(dye) = name.strip_suffix("_terracotta") {
        let [r, g, b, a] = dye_color(dye);
        let mix = |c: u8| u8::midpoint(c, 150);
        return shade(
            [
                mix(r),
                mix(g).saturating_sub(15),
                mix(b).saturating_sub(30),
                a,
            ],
            0.85,
        );
    }
    match name {
        "redstone_lamp" if lit => [240, 200, 130, 255],
        "redstone_lamp" => [120, 76, 46, 255],
        "terracotta" => [152, 94, 67, 255],
        "oak_planks" => [162, 130, 78, 255],
        "spruce_planks" => [114, 84, 48, 255],
        "birch_planks" => [192, 175, 121, 255],
        "jungle_planks" => [160, 115, 80, 255],
        "acacia_planks" => [168, 90, 50, 255],
        "dark_oak_planks" => [66, 43, 20, 255],
        "mangrove_planks" => [117, 54, 48, 255],
        "cherry_planks" => [226, 178, 172, 255],
        "bamboo_planks" => [193, 173, 80, 255],
        "crimson_planks" => [101, 48, 70, 255],
        "warped_planks" => [43, 104, 99, 255],
        "cobblestone" => [118, 118, 118, 255],
        "polished_granite" => [154, 106, 89, 255],
        "deepslate_tiles" => [54, 54, 55, 255],
        "polished_blackstone" => [53, 48, 56, 255],
        "smooth_sandstone" => [223, 214, 170, 255],
        "bamboo_mosaic" => [190, 170, 78, 255],
        "glass" => [200, 222, 232, 90],
        "water" => [63, 118, 228, 150],
        "stone" => [125, 125, 125, 255],
        "smooth_stone" => [158, 158, 158, 255],
        "stone_bricks" => [122, 121, 122, 255],
        "smooth_quartz" | "quartz_block" => [235, 229, 222, 255],
        "white_wool" => [234, 236, 237, 255],
        "white_terracotta" => [210, 178, 161, 255],
        "polished_andesite" => [132, 134, 133, 255],
        "polished_diorite" => [192, 193, 194, 255],
        "redstone_block" => [175, 25, 5, 255],
        "target" => [226, 216, 200, 255],
        "observer" => [95, 95, 95, 255],
        "barrel" => [134, 98, 58, 255],
        "hopper" => [62, 62, 66, 255],
        "piston" | "sticky_piston" | "piston_head" => [160, 140, 100, 255],
        _ => [150, 150, 150, 255],
    }
}

/// Minecraft's concrete colours, by dye, such as `light_blue`.
pub fn dye_color(dye: &str) -> [u8; 4] {
    match dye {
        "white" => [207, 213, 214, 255],
        "orange" => [224, 97, 1, 255],
        "magenta" => [169, 48, 159, 255],
        "light_blue" => [36, 137, 199, 255],
        "yellow" => [241, 175, 21, 255],
        "lime" => [94, 169, 24, 255],
        "pink" => [214, 101, 143, 255],
        "gray" => [55, 58, 62, 255],
        "light_gray" => [125, 125, 115, 255],
        "cyan" => [21, 119, 136, 255],
        "purple" => [100, 32, 156, 255],
        "blue" => [45, 47, 143, 255],
        "brown" => [96, 60, 32, 255],
        "green" => [73, 91, 36, 255],
        "red" => [142, 33, 33, 255],
        "black" => [8, 10, 15, 255],
        _ => [150, 150, 150, 255],
    }
}

fn shade([r, g, b, a]: [u8; 4], light: f32) -> [u8; 4] {
    let scale = |c: u8| (f32::from(c) * light).round().clamp(0.0, 255.0) as u8;
    [scale(r), scale(g), scale(b), a]
}

// The values the tests compare are exact in binary.
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn dust_is_tinted_by_its_power_as_the_game_tints_it() {
        assert_eq!(tint("redstone_wire", Some("0")), [0.3, 0.0, 0.0]);
        let full = tint("redstone_wire", Some("15"));
        assert!((full[0] - 1.0).abs() < 1e-6);
        assert!((full[1] - 0.2).abs() < 1e-6);
        assert_eq!(full[2], 0.0);
        assert_eq!(tint("stone", None), [1.0; 3]);
    }
}
