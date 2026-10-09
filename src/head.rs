//! A player's head, from their Minecraft skin: the 8×8 face with the hat
//! layer over it, as a PNG small enough for a page to scale up pixelated.

use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use tiny_skia::{Pixmap, PremultipliedColorU8};

const PROFILE: &str =
    "https://sessionserver.mojang.com/session/minecraft/profile";

/// Where skins are kept; a skin address anywhere else is not fetched.
const TEXTURES: &str = "textures.minecraft.net";

/// The PNG of the head of the player with `uuid`, or why there is none.
pub async fn fetch(uuid: &str) -> Result<Vec<u8>, String> {
    let failed =
        |err: reqwest::Error| format!("could not fetch the skin: {err}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(failed)?;
    let profile = client
        .get(format!("{PROFILE}/{uuid}"))
        .send()
        .await
        .map_err(failed)?;
    if !profile.status().is_success() {
        return Err(format!("Mojang has no player {uuid}"));
    }
    let profile: Profile = profile.json().await.map_err(failed)?;
    let url = skin_url(&profile).ok_or("the player has no skin")?;
    let skin = client.get(url).send().await.map_err(failed)?;
    let skin = skin.bytes().await.map_err(failed)?;
    face(&skin)
}

#[derive(Deserialize)]
struct Profile {
    properties: Vec<Property>,
}

#[derive(Deserialize)]
struct Property {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct Textures {
    textures: Skins,
}

#[derive(Deserialize)]
struct Skins {
    #[serde(rename = "SKIN")]
    skin: Option<Skin>,
}

#[derive(Deserialize)]
struct Skin {
    url: String,
}

/// The skin's address, over https, if it is where Mojang keeps skins. The
/// profile gives it base64-encoded JSON in its `textures` property.
fn skin_url(profile: &Profile) -> Option<String> {
    let encoded = &profile
        .properties
        .iter()
        .find(|p| p.name == "textures")?
        .value;
    let textures: Textures =
        serde_json::from_slice(&STANDARD.decode(encoded).ok()?).ok()?;
    let url = textures.textures.skin?.url;
    let path = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?
        .strip_prefix(TEXTURES)?
        .strip_prefix('/')?;
    Some(format!("https://{TEXTURES}/{path}"))
}

/// The face of `skin`, a skin PNG, with the hat layer drawn over it.
fn face(skin: &[u8]) -> Result<Vec<u8>, String> {
    let skin =
        Pixmap::decode_png(skin).map_err(|err| format!("not a skin: {err}"))?;
    if skin.width() < 64 || skin.height() < 32 {
        return Err("not a skin: too small".to_owned());
    }
    let mut head = Pixmap::new(8, 8).ok_or("could not make the head")?;
    let pixel = |x: u32, y: u32| skin.pixels()[(y * skin.width() + x) as usize];
    for y in 0..8 {
        for x in 0..8 {
            // The face is at (8, 8), its hat at (40, 8).
            let face = pixel(8 + x, 8 + y);
            let hat = pixel(40 + x, 8 + y);
            head.pixels_mut()[(y * 8 + x) as usize] = over(hat, face);
        }
    }
    head.encode_png()
        .map_err(|err| format!("could not encode the head: {err}"))
}

/// `top` drawn over `bottom`; both are premultiplied, so this is one sum.
fn over(
    top: PremultipliedColorU8,
    bottom: PremultipliedColorU8,
) -> PremultipliedColorU8 {
    let rest = 255 - u16::from(top.alpha());
    let mix = |t: u8, b: u8| {
        let value = u16::from(t) + (u16::from(b) * rest + 127) / 255;
        u8::try_from(value.min(255)).unwrap_or(u8::MAX)
    };
    PremultipliedColorU8::from_rgba(
        mix(top.red(), bottom.red()),
        mix(top.green(), bottom.green()),
        mix(top.blue(), bottom.blue()),
        mix(top.alpha(), bottom.alpha()),
    )
    .unwrap_or(bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(textures: &str) -> Profile {
        Profile {
            properties: vec![Property {
                name: "textures".to_owned(),
                value: STANDARD.encode(textures),
            }],
        }
    }

    #[test]
    fn the_skin_url_comes_from_mojangs_texture_server_only() {
        let ours = profile(
            r#"{"textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/abc"}}}"#,
        );
        assert_eq!(
            skin_url(&ours).as_deref(),
            Some("https://textures.minecraft.net/texture/abc")
        );
        let elsewhere = profile(
            r#"{"textures":{"SKIN":{"url":"http://example.com/texture/abc"}}}"#,
        );
        assert_eq!(skin_url(&elsewhere), None);
        assert_eq!(skin_url(&profile(r#"{"textures":{}}"#)), None);
    }

    #[test]
    fn the_hat_covers_the_face_where_it_is_drawn() {
        let mut skin = Pixmap::new(64, 64).unwrap();
        let red = PremultipliedColorU8::from_rgba(255, 0, 0, 255).unwrap();
        let blue = PremultipliedColorU8::from_rgba(0, 0, 255, 255).unwrap();
        let width = skin.width();
        for y in 8..16 {
            for x in 8..16 {
                skin.pixels_mut()[(y * width + x) as usize] = red;
            }
        }
        // A hat on the face's top-left pixel only.
        skin.pixels_mut()[(8 * width + 40) as usize] = blue;
        let head =
            Pixmap::decode_png(&face(&skin.encode_png().unwrap()).unwrap())
                .unwrap();
        assert_eq!((head.width(), head.height()), (8, 8));
        assert_eq!(head.pixels()[0], blue);
        assert_eq!(head.pixels()[1], red);
    }
}
