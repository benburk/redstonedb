//! Textures as faces show them: an animated one's first frame, and the
//! part a face shows cut out, tinted and shaded, and shrunk to the size it
//! is drawn at.

use tiny_skia::{Pixmap, PremultipliedColorU8};

/// A block texture, and whether every texel of it is opaque.
pub struct Texture {
    pub pixmap: Pixmap,
    pub opaque: bool,
}

impl Texture {
    /// The texture in `pixmap`, its first frame if its frames stand one
    /// above the next, as an animated texture's do.
    pub fn new(pixmap: Pixmap) -> Option<Texture> {
        let side = pixmap.width();
        let pixmap = if pixmap.height() > side {
            let frame = side as usize * side as usize;
            let mut first = Pixmap::new(side, side)?;
            first
                .pixels_mut()
                .copy_from_slice(&pixmap.pixels()[..frame]);
            first
        } else {
            pixmap
        };
        let opaque = pixmap.pixels().iter().all(|p| p.alpha() == 255);
        Some(Texture { pixmap, opaque })
    }

    /// A texture of one colour, for a block drawn without its own.
    pub fn solid([r, g, b, a]: [u8; 4]) -> Texture {
        let mut pixmap = Pixmap::new(1, 1).expect("one pixel");
        pixmap.pixels_mut()[0] =
            tiny_skia::ColorU8::from_rgba(r, g, b, a).premultiply();
        Texture {
            pixmap,
            opaque: a == 255,
        }
    }
}

/// The part `uv` of `texture`, in sixteenths, left, top, right, bottom,
/// one pixel to a texel, each multiplied by `color` and by the light of
/// the face's corners `light`, top left, top right, bottom right, and
/// bottom left, blended across it.
pub fn cut(
    texture: &Texture,
    uv: [f32; 4],
    color: [f32; 3],
    light: [f32; 4],
) -> Option<Pixmap> {
    let source = &texture.pixmap;
    let (tw, th) = (source.width(), source.height());
    let scale = tw as f32 / 16.0;
    let [u1, v1, u2, v2] = uv;
    let width = ((u2 - u1).abs() * scale).round().max(1.0) as u32;
    let height = ((v2 - v1).abs() * scale).round().max(1.0) as u32;
    let mut out = Pixmap::new(width, height)?;
    let plain = color.iter().chain(&light).all(|&k| k >= 1.0);
    let [tl, tr, br, bl] = light;
    for j in 0..height {
        let t = (j as f32 + 0.5) / height as f32;
        let row = ((v1 + t * (v2 - v1)) * scale)
            .floor()
            .clamp(0.0, th as f32 - 1.0) as usize;
        for i in 0..width {
            let s = (i as f32 + 0.5) / width as f32;
            let column = ((u1 + s * (u2 - u1)) * scale)
                .floor()
                .clamp(0.0, tw as f32 - 1.0) as usize;
            let texel = source.pixels()[row * tw as usize + column];
            out.pixels_mut()[(j * width + i) as usize] = if plain {
                texel
            } else {
                let lit =
                    (tl + (tr - tl) * s) * (1.0 - t) + (bl + (br - bl) * s) * t;
                let alpha = texel.alpha();
                let channel = |c: u8, k: f32| {
                    ((f32::from(c) * k * lit).round() as u8).min(alpha)
                };
                PremultipliedColorU8::from_rgba(
                    channel(texel.red(), color[0]),
                    channel(texel.green(), color[1]),
                    channel(texel.blue(), color[2]),
                    alpha,
                )?
            };
        }
    }
    Some(out)
}

/// `pixmap` shrunk to `width` by `height`, each pixel the average of the
/// pixels it covers.
pub fn shrink(pixmap: &Pixmap, width: u32, height: u32) -> Pixmap {
    let (sw, sh) = (pixmap.width() as usize, pixmap.height() as usize);
    let (width, height) =
        (width.clamp(1, sw as u32), height.clamp(1, sh as u32));
    let mut out =
        Pixmap::new(width, height).expect("a shrunk pixmap has pixels");
    let span = |i: usize, to: usize, from: usize| {
        (i * from / to, ((i + 1) * from / to).max(i * from / to + 1))
    };
    let source = pixmap.pixels();
    for j in 0..height as usize {
        let (y0, y1) = span(j, height as usize, sh);
        for i in 0..width as usize {
            let (x0, x1) = span(i, width as usize, sw);
            let mut sum = [0u32; 4];
            for y in y0..y1 {
                for p in &source[y * sw + x0..y * sw + x1] {
                    sum[0] += u32::from(p.red());
                    sum[1] += u32::from(p.green());
                    sum[2] += u32::from(p.blue());
                    sum[3] += u32::from(p.alpha());
                }
            }
            let count = ((y1 - y0) * (x1 - x0)) as u32;
            let [r, g, b, a] = sum.map(|c| ((c + count / 2) / count) as u8);
            if let Some(p) =
                PremultipliedColorU8::from_rgba(r.min(a), g.min(a), b.min(a), a)
            {
                out.pixels_mut()[j * width as usize + i] = p;
            }
        }
    }
    out
}
