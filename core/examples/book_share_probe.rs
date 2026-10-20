//! Synthetic LAN sender for native interoperability checks. Never uses the user's library.
use reader_core::{book_share, library::Library};
use serde_json::json;
use std::{fs, path::Path, thread, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    reader_core::sharing::set_debug_logging_enabled(true);
    if std::env::args().nth(1).as_deref() == Some("--decode") {
        let path = fs::canonicalize(std::env::args().nth(2).ok_or("Missing QR image")?)?;
        let root = path
            .parent()
            .and_then(Path::to_str)
            .ok_or("Invalid QR path")?;
        let decoded = book_share::dispatch(root, json!({"op":"decode_image","path":path}))?;
        if let Some(metadata) = std::env::args().nth(3) {
            let ticket =
                book_share::Ticket::parse(decoded["link"].as_str().ok_or("Missing ticket")?)?;
            if let book_share::Location::Lan { address, .. } = ticket.location {
                let socket: std::net::SocketAddr = address.parse()?;
                fs::write(
                    metadata,
                    serde_json::to_vec(&json!({"port":socket.port()}))?,
                )?;
            }
        }
        println!("Screenshot QR decoded and temporary ticket validated");
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--receive") {
        let path = fs::canonicalize(
            std::env::args()
                .nth(2)
                .ok_or("Missing sender QR screenshot")?,
        )?;
        let root = std::env::args()
            .nth(3)
            .ok_or("Missing receiver directory")?;
        fs::create_dir_all(&root)?;
        let root = fs::canonicalize(root)?;
        let root = root.to_str().ok_or("Invalid receiver directory")?;
        let decoded = book_share::dispatch(root, json!({"op":"decode_image","path":path}))?;
        let mut ticket =
            book_share::Ticket::parse(decoded["link"].as_str().ok_or("Missing decoded ticket")?)?;
        if let Some(host) = std::env::args().nth(4) {
            if let book_share::Location::Lan { address, .. } = &mut ticket.location {
                *address = format!(
                    "{}:{}",
                    host,
                    address.rsplit(':').next().ok_or("Missing port")?
                );
            }
        }
        let preview = book_share::dispatch(root, json!({"op":"receive","link":ticket.link()?}))?;
        let accepted = book_share::dispatch(root, json!({"op":"accept"}))?;
        let report = json!({"passed":true,"count":preview["preview"]["books"].as_array().map(Vec::len),"imported":accepted["imported"],"skipped":accepted["skipped"]});
        fs::write(
            Path::new(root).join("native-sender-verification.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        println!("Native sender QR decoded, transferred, verified, and imported");
        return Ok(());
    }
    let output = std::env::args()
        .nth(1)
        .ok_or("Pass a new probe directory")?;
    let root = Path::new(&output).join("sender");
    fs::create_dir_all(&root)?;
    let root = fs::canonicalize(root)?;
    let root = root.to_str().ok_or("Invalid path")?;
    let mut library = Library::read_snapshot(root)?;
    if library.books.is_empty() {
        let source = Path::new(root).join("synthetic.txt");
        fs::write(
            &source,
            "第一章 开始\n跨端临时分享测试。仅包含合成内容。\n第二章 继续\n校验内容不携带阅读进度。",
        )?;
        let epub =
            reader_core::txt::convert_txt_to_epub(&source, Path::new(root), &Default::default())?
                .epub_path;
        library
            .add_or_update(
                root,
                "跨端分享测试 Book".into(),
                epub.to_string_lossy().into(),
                0,
                None,
            )
            .unwrap();
        library = Library::read_snapshot(root)?;
    }
    let ids = library
        .books
        .iter()
        .map(|b| b.id.clone())
        .collect::<Vec<_>>();
    let response = book_share::dispatch(root, json!({"op":"start_lan","ids":ids}))?;
    // The ticket contains a temporary secret: keep it in the local verification folder.
    let outgoing = &response["outgoing"];
    let mut link = outgoing["link"]
        .as_str()
        .ok_or("Missing ticket")?
        .to_string();
    if let Some(host) = std::env::args().nth(2) {
        let mut ticket = book_share::Ticket::parse(&link)?;
        if let book_share::Location::Lan { address, .. } = &mut ticket.location {
            *address = format!(
                "{}:{}",
                host,
                address.rsplit(':').next().ok_or("Missing port")?
            );
        }
        ticket.validate()?;
        link = ticket.link()?;
    }
    fs::write(Path::new(&output).join("ticket.txt"), &link)?;
    let qr = qrcode::QrCode::new(link.as_bytes())?;
    let size = (qr.width() + 8) * 8;
    let image = image::GrayImage::from_fn(size as u32, size as u32, |x, y| {
        let x = x as usize / 8;
        let y = y as usize / 8;
        let dark = x >= 4
            && y >= 4
            && x < qr.width() + 4
            && y < qr.width() + 4
            && qr[(x - 4, y - 4)] == qrcode::Color::Dark;
        image::Luma([if dark { 0 } else { 255 }])
    });
    image.save(Path::new(&output).join("qr.png"))?;
    println!("Synthetic LAN sender ready");
    for _ in 0..620 {
        let state = book_share::state(root);
        let phase = state["progress"]["phase"].as_str().unwrap_or("");
        if matches!(phase, "completed" | "expired" | "error" | "cancelled") {
            fs::write(
                Path::new(&output).join("sender-result.json"),
                serde_json::to_vec_pretty(&json!({"phase":phase,"progress":state["progress"]}))?,
            )?;
            book_share::stop(root);
            return if phase == "completed" {
                Ok(())
            } else {
                Err(format!("Sender ended: {phase}").into())
            };
        }
        thread::sleep(Duration::from_secs(1));
    }
    book_share::stop(root);
    Err("Probe timed out".into())
}
