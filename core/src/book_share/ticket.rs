use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    io::Cursor,
    net::{IpAddr, SocketAddr},
    path::Path,
};

const PREFIX: &str = "rustepubreader://share/v1/";
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Location {
    Lan {
        address: String,
        session: String,
        pin: String,
        expires: u64,
    },
    Github {
        owner: String,
        repository: String,
        asset: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        share: Option<String>,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ticket {
    pub location: Location,
    pub key: String,
}
// Intentionally no Debug: a ticket is a bearer capability, never diagnostic text.
pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
        && value != "."
        && value != ".."
}
pub fn local_address(address: &str) -> Result<SocketAddr, String> {
    let a: SocketAddr = address.parse().map_err(|_| "Invalid local share address")?;
    let local = match a.ip() {
        IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
    };
    if !local || a.port() == 0 {
        return Err("Share address must be on a local network".into());
    }
    Ok(a)
}
impl Ticket {
    pub fn secret(&self) -> Result<[u8; 32], String> {
        URL_SAFE_NO_PAD
            .decode(&self.key)
            .map_err(|_| "Invalid share key")?
            .try_into()
            .map_err(|_| "Invalid share key".into())
    }
    pub fn validate(&self) -> Result<(), String> {
        self.secret()?;
        match &self.location {
            Location::Lan {
                address,
                session,
                pin,
                expires,
            } => {
                local_address(address)?;
                uuid::Uuid::parse_str(session).map_err(|_| "Invalid share session")?;
                if pin.len() != 4 || !pin.bytes().all(|c| c.is_ascii_digit()) {
                    return Err("Invalid share PIN".into());
                }
                if *expires <= crate::now_secs() || *expires > crate::now_secs() + 900 {
                    return Err("Local share has expired".into());
                }
            }
            Location::Github {
                owner,
                repository,
                asset,
                share,
            } => {
                if share
                    .as_ref()
                    .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
                {
                    return Err("Invalid GitHub share identifier".into());
                }
                if !identifier(owner) || !identifier(repository) || *asset == 0 {
                    return Err("Invalid GitHub share address".into());
                }
            }
        }
        Ok(())
    }
    pub fn link(&self) -> Result<String, String> {
        Ok(format!(
            "{PREFIX}{}",
            URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(self).map_err(|_| "Cannot encode share link")?)
        ))
    }
    pub fn parse(input: &str) -> Result<Self, String> {
        let encoded = input
            .trim()
            .strip_prefix(PREFIX)
            .ok_or("Not an EPUB Reader share link")?;
        if encoded.len() > 2048 {
            return Err("Share link is too long".into());
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| "Invalid share link")?;
        let ticket: Self = serde_json::from_slice(&bytes).map_err(|_| "Invalid share link")?;
        ticket.validate()?;
        Ok(ticket)
    }
}
pub fn qr_png(link: &str) -> Result<Vec<u8>, String> {
    let code = qrcode::QrCode::with_error_correction_level(link.as_bytes(), qrcode::EcLevel::M)
        .map_err(|_| "Cannot generate share QR code")?;
    let modules = code.width();
    let scale = 8;
    let size = ((modules + 8) * scale) as u32;
    let mut image = image::GrayImage::from_pixel(size, size, image::Luma([255]));
    for y in 0..modules {
        for x in 0..modules {
            if code[(x, y)] == qrcode::Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        image.put_pixel(
                            ((x + 4) * scale + dx) as u32,
                            ((y + 4) * scale + dy) as u32,
                            image::Luma([0]),
                        );
                    }
                }
            }
        }
    }
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| "Cannot render share QR code")?;
    Ok(output.into_inner())
}
pub fn decode_image(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|_| "Cannot open QR image")?;
    if metadata.len() > 16 * 1024 * 1024 {
        return Err("QR image is too large".into());
    }
    let mut reader = image::ImageReader::open(path)
        .map_err(|_| "Cannot open QR image")?
        .with_guessed_format()
        .map_err(|_| "Unsupported QR image")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "Cannot decode QR image")?
        .thumbnail(2048, 2048)
        .to_luma8();
    let mut decoder = quircs::Quirc::default();
    for code in decoder
        .identify(image.width() as usize, image.height() as usize, &image)
        .flatten()
    {
        if let Ok(decoded) = code.decode() {
            if let Ok(link) = String::from_utf8(decoded.payload) {
                if Ticket::parse(&link).is_ok() {
                    return Ok(link);
                }
            }
        }
    }
    Err("No valid EPUB Reader share QR code found".into())
}
