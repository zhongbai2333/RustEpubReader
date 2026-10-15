use crate::{app::ReaderApp, system_fonts::SystemFont};
use eframe::egui;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone, Deserialize)]
pub(crate) struct DownloadFont {
    id: String,
    name_zh: String,
    name_en: String,
    postscript: String,
    filename: String,
    url: String,
    sha256: String,
    bytes: u64,
    license: String,
    license_text: String,
    project_url: String,
    language: String,
    description_zh: String,
    description_en: String,
    license_url: String,
    preview_text: String,
}

fn preview_bytes(id: &str) -> Option<&'static [u8]> {
    macro_rules! masks { ($($name:literal),* $(,)?) => { match id {
        $($name => Some(include_bytes!(concat!("../../../resources/fonts/previews/", $name, ".png")).as_slice()),)*
        _ => None,
    } }; }
    masks!(
        "lxgw-wenkai",
        "source-han-serif-sc",
        "source-han-sans-sc",
        "lxgw-wenkai-screen",
        "source-han-serif-tc",
        "lxgw-wenkai-tc",
        "literata",
        "source-serif-4",
        "lora",
        "crimson-pro",
        "source-sans-3",
        "atkinson-hyperlegible",
        "lxgw-marker-gothic",
        "xiaolai",
        "yozai",
        "lxgw-zhenkai",
        "alegreya",
        "eb-garamond",
        "arvo",
        "andika"
    )
}
fn specimen(ui: &mut egui::Ui, font: &DownloadFont) {
    let width = ui.available_width().min(320.);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, width * 216. / 1200.),
        egui::Sense::hover(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &font.preview_text)
    });
    if !ui.is_rect_visible(rect) {
        return;
    }
    let key = egui::Id::new(("font_specimen", &font.id));
    let cached = ui
        .ctx()
        .data_mut(|d| d.get_temp::<egui::TextureHandle>(key));
    let texture = cached.or_else(|| {
        let pixels = image::load_from_memory(preview_bytes(&font.id)?)
            .ok()?
            .to_rgba8();
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [pixels.width() as usize, pixels.height() as usize],
            pixels.as_raw(),
        );
        let texture = ui.ctx().load_texture(
            format!("font-specimen-{}", font.id),
            image,
            egui::TextureOptions::LINEAR,
        );
        ui.ctx().data_mut(|d| d.insert_temp(key, texture.clone()));
        Some(texture)
    });
    if let Some(texture) = texture {
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
            ui.visuals().text_color(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_invalid_font_leaves_no_files() {
        let source = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let invalid = source.path().join("invalid.ttf");
        std::fs::write(&invalid, b"not a font").unwrap();
        assert!(import_font(&invalid, destination.path()).is_err());
        assert_eq!(std::fs::read_dir(destination.path()).unwrap().count(), 0);
    }
    #[test]
    fn catalog_artifacts_match_native_faces() {
        let Ok(directory) = std::env::var("READER_FONT_FIXTURES") else {
            eprintln!("Local font artifacts not supplied; metadata is checked by CI separately");
            return;
        };
        assert_eq!(catalog().len(), 20);
        for font in catalog() {
            let image =
                image::load_from_memory(preview_bytes(&font.id).expect("preview asset")).unwrap();
            assert_eq!((image.width(), image.height()), (1200, 216));
            let bytes = std::fs::read(Path::new(&directory).join(&font.filename)).unwrap();
            assert_eq!(bytes.len() as u64, font.bytes);
            assert_eq!(hex::encode(Sha256::digest(&bytes)), font.sha256);
            let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
            assert!(
                face.names()
                    .into_iter()
                    .any(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME
                        && n.to_string().as_deref() == Some(&font.postscript)),
                "{}",
                font.id
            );
            assert!(face.glyph_index('A').is_some());
            if font.language == "zh" {
                assert!(face.glyph_index('汉').is_some() || face.glyph_index('漢').is_some());
            }
            assert!(
                (350..=500).contains(&face.weight().to_number()),
                "{} is not a regular face",
                font.id
            );
        }
        let source = Path::new(&directory).join("atkinson-hyperlegible.ttf");
        let data = tempfile::tempdir().unwrap();
        let imports = data.path().join("fonts/imports");
        import_font(&source, &imports).unwrap();
        import_font(&source, &imports).unwrap();
        assert_eq!(std::fs::read_dir(&imports).unwrap().count(), 2);
        let mut names = Vec::new();
        let mut paths = HashMap::new();
        add_imported(data.path().to_str().unwrap(), &mut names, &mut paths);
        assert_eq!(names.len(), 1);
        assert!(paths[&names[0]].load().is_some());
    }
    #[test]
    fn failed_downloads_preserve_existing_font_and_remove_temporary_files() {
        for (body, expected, checksum) in [
            (b"bad".to_vec(), 3, "0".repeat(64)),
            (b"too long".to_vec(), 3, "0".repeat(64)),
            (
                b"not a font".to_vec(),
                10,
                hex::encode(Sha256::digest(b"not a font")),
            ),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/font", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let _ = stream.read(&mut [0u8; 2048]);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            });
            let root = tempfile::tempdir().unwrap();
            let mut font = catalog()[0].clone();
            font.url = url;
            font.bytes = expected;
            font.sha256 = checksum;
            let destination = root.path().join(&font.filename);
            std::fs::write(&destination, b"original font").unwrap();
            assert!(download(&font, root.path(), |_| {}).is_err());
            server.join().unwrap();
            assert_eq!(std::fs::read(destination).unwrap(), b"original font");
            assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
        }
    }
}
fn catalog() -> &'static [DownloadFont] {
    static CATALOG: OnceLock<Vec<DownloadFont>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../../../resources/fonts/catalog.json"))
            .expect("font catalog")
    })
}
pub(crate) fn add_installed(
    data: &str,
    names: &mut Vec<String>,
    paths: &mut HashMap<String, SystemFont>,
) {
    add_imported(data, names, paths);
    for font in catalog() {
        let path = Path::new(data).join("fonts").join(&font.filename);
        if path
            .metadata()
            .is_ok_and(|m| m.is_file() && m.len() == font.bytes)
        {
            paths.insert(
                font.name_zh.clone(),
                SystemFont {
                    path: path.to_string_lossy().into_owned(),
                    index: 0,
                },
            );
            if !names.contains(&font.name_zh) {
                names.push(font.name_zh.clone());
            }
        }
    }
}
#[derive(Clone, Deserialize)]
struct ExternalFont {
    name_zh: String,
    name_en: String,
    language: String,
    description_zh: String,
    description_en: String,
    project_url: String,
    license_url: String,
    checked_at: String,
}
fn external() -> &'static [ExternalFont] {
    static EXTERNAL: OnceLock<Vec<ExternalFont>> = OnceLock::new();
    EXTERNAL.get_or_init(|| {
        serde_json::from_str(include_str!("../../../resources/fonts/external.json"))
            .expect("external font catalog")
    })
}
#[derive(Serialize, Deserialize)]
struct ImportedFont {
    name: String,
    filename: String,
}
fn import_font(source: &Path, directory: &Path) -> anyhow::Result<()> {
    let ext = source
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    anyhow::ensure!(matches!(ext.as_str(), "ttf" | "otf"), "unsupported font");
    let mut bytes = Vec::new();
    std::fs::File::open(source)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        !bytes.is_empty() && bytes.len() <= 64 * 1024 * 1024,
        "font size limit"
    );
    let face = ttf_parser::Face::parse(&bytes, 0).map_err(|_| anyhow::anyhow!("invalid font"))?;
    let family = face
        .names()
        .into_iter()
        .find_map(|n| {
            (n.name_id == ttf_parser::name_id::FULL_NAME)
                .then(|| n.to_string())
                .flatten()
        })
        .unwrap_or_else(|| {
            source
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
    let hash = hex::encode(Sha256::digest(&bytes));
    let cjk = if face.glyph_index('汉').is_some() || face.glyph_index('漢').is_some() {
        "CJK:"
    } else {
        ""
    };
    let record = ImportedFont {
        name: format!("{} [{}{}]", family, cjk, &hash[..6]),
        filename: format!("{hash}.{ext}"),
    };
    std::fs::create_dir_all(directory)?;
    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
    temp.write_all(&bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(directory.join(&record.filename))?;
    let mut metadata = tempfile::NamedTempFile::new_in(directory)?;
    metadata.write_all(&serde_json::to_vec(&record)?)?;
    metadata.as_file().sync_all()?;
    metadata.persist(directory.join(format!("{hash}.json")))?;
    Ok(())
}
fn add_imported(data: &str, names: &mut Vec<String>, paths: &mut HashMap<String, SystemFont>) {
    let directory = Path::new(data).join("fonts/imports");
    for entry in std::fs::read_dir(&directory)
        .into_iter()
        .flatten()
        .flatten()
    {
        if entry.path().extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(record) = std::fs::read(entry.path())
            .ok()
            .and_then(|b| serde_json::from_slice::<ImportedFont>(&b).ok())
        else {
            continue;
        };
        if Path::new(&record.filename)
            .file_name()
            .is_none_or(|n| n != record.filename.as_str())
        {
            continue;
        }
        let file = directory.join(&record.filename);
        if !file.is_file() {
            continue;
        }
        if !names.contains(&record.name) {
            names.push(record.name.clone());
        }
        paths.insert(
            record.name,
            SystemFont {
                path: file.to_string_lossy().into_owned(),
                index: 0,
            },
        );
    }
}
#[derive(Clone, Default)]
struct LibraryView {
    open: bool,
    latin: bool,
}

#[derive(Clone)]
enum Job {
    Downloading(u64),
    Done,
    Failed,
}
type Jobs = Arc<Mutex<HashMap<String, Job>>>;

fn download(font: &DownloadFont, directory: &Path, progress: impl Fn(u64)) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(180))
        .user_agent("RustEpubReader-fonts")
        .build()?;
    let mut response = client.get(&font.url).send()?.error_for_status()?;
    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = response.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        anyhow::ensure!(count <= font.bytes, "font size mismatch");
        hash.update(&buffer[..n]);
        temp.write_all(&buffer[..n])?;
        progress(count);
    }
    anyhow::ensure!(
        count == font.bytes && hex::encode(hash.finalize()) == font.sha256,
        "font checksum mismatch"
    );
    let bytes = std::fs::read(temp.path())?;
    let face = ttf_parser::Face::parse(&bytes, 0).map_err(|_| anyhow::anyhow!("invalid font"))?;
    anyhow::ensure!(
        face.names()
            .into_iter()
            .any(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME
                && n.to_string().as_deref() == Some(&font.postscript)),
        "unexpected font face"
    );
    temp.as_file().sync_all()?;
    temp.persist(directory.join(&font.filename))?;
    std::fs::write(
        directory.join(format!("{}-LICENSE.txt", font.id)),
        &font.license_text,
    )?;
    Ok(())
}
impl ReaderApp {
    pub(crate) fn open_font_library(&mut self, ui: &mut egui::Ui, latin: bool) {
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::new("font_library_view"),
                LibraryView { open: true, latin },
            )
        });
    }
    pub(crate) fn import_reader_font(&mut self, ui: &mut egui::Ui) {
        let Some(source) = rfd::FileDialog::new()
            .add_filter("Font", &["ttf", "otf"])
            .pick_file()
        else {
            return;
        };
        let jobs: Jobs = ui.ctx().data_mut(|d| {
            d.get_temp_mut_or_default::<Jobs>(egui::Id::new("font_download_jobs"))
                .clone()
        });
        if matches!(
            jobs.lock().unwrap().get("import"),
            Some(Job::Downloading(_))
        ) {
            return;
        }
        jobs.lock()
            .unwrap()
            .insert("import".into(), Job::Downloading(0));
        let directory = PathBuf::from(&self.data_dir).join("fonts/imports");
        let ctx = ui.ctx().clone();
        std::thread::spawn(move || {
            let result = import_font(&source, &directory);
            jobs.lock().unwrap().insert(
                "import".into(),
                if result.is_ok() {
                    Job::Done
                } else {
                    Job::Failed
                },
            );
            ctx.request_repaint();
        });
    }
    pub(crate) fn render_font_library_window(&mut self, ui: &mut egui::Ui) {
        let id = egui::Id::new("font_library_view");
        let mut view = ui
            .ctx()
            .data_mut(|d| d.get_temp_mut_or_default::<LibraryView>(id).clone());
        // Consume completions even when the library window is closed.
        let jobs: Jobs = ui.ctx().data_mut(|d| {
            d.get_temp_mut_or_default::<Jobs>(egui::Id::new("font_download_jobs"))
                .clone()
        });
        let snapshot = jobs.lock().unwrap().clone();
        if snapshot.values().any(|j| matches!(j, Job::Done)) {
            add_installed(
                &self.data_dir,
                &mut self.system_font_names,
                &mut self.system_font_paths,
            );
            jobs.lock()
                .unwrap()
                .retain(|_, job| !matches!(job, Job::Done));
        }
        if !view.open {
            if matches!(snapshot.get("import"), Some(Job::Downloading(_))) {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(self.i18n.t("fonts.importing"));
                });
            }
            if matches!(snapshot.get("import"), Some(Job::Failed)) {
                ui.colored_label(ui.visuals().error_fg_color, self.i18n.t("fonts.invalid"));
            }
            return;
        }
        egui::Window::new(self.i18n.t("fonts.library").to_owned())
            .id(id)
            .open(&mut view.open)
            .order(egui::Order::Foreground)
            .default_pos(egui::pos2(220., 115.))
            .default_width(470.)
            .resizable(true)
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                ui.spacing_mut().item_spacing.y = 6.;
                ui.spacing_mut().interact_size.y = 28.;
                ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                ui.label(self.i18n.t(if view.latin {
                    "fonts.use_english"
                } else {
                    "fonts.use_chinese"
                }));
                ui.label(egui::RichText::new(self.i18n.t("fonts.description")).small());
                ui.label(
                    egui::RichText::new(self.i18n.t("fonts.preview_note"))
                        .small()
                        .weak(),
                );
                if ui.button(self.i18n.t("fonts.import")).clicked() {
                    self.import_reader_font(ui);
                }
                ui.label(
                    egui::RichText::new(self.i18n.t("fonts.import_local"))
                        .small()
                        .weak(),
                );
                if matches!(snapshot.get("import"), Some(Job::Downloading(_))) {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(self.i18n.t("fonts.importing"));
                    });
                }
                if matches!(snapshot.get("import"), Some(Job::Failed)) {
                    ui.colored_label(ui.visuals().error_fg_color, self.i18n.t("fonts.invalid"));
                }
                egui::ScrollArea::vertical()
                    .id_salt("font_library_scroll")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .max_height(500.)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(self.i18n.t("fonts.direct")).strong());
                        for font in catalog()
                            .iter()
                            .filter(|f| (f.language == "en") == view.latin)
                        {
                            ui.separator();
                            let title = if self.i18n.language().code() == "zh_cn" {
                                &font.name_zh
                            } else {
                                &font.name_en
                            };
                            ui.label(egui::RichText::new(title).strong());
                            ui.label(
                                egui::RichText::new(if self.i18n.language().code() == "zh_cn" {
                                    &font.description_zh
                                } else {
                                    &font.description_en
                                })
                                .small()
                                .weak(),
                            );
                            specimen(ui, font);
                            let path = PathBuf::from(&self.data_dir)
                                .join("fonts")
                                .join(&font.filename);
                            let installed = path.metadata().is_ok_and(|m| m.len() == font.bytes);
                            ui.horizontal(|ui| {
                                ui.label(if font.bytes < 1_000_000 {
                                    format!("{:.0} KB", font.bytes as f64 / 1_000.)
                                } else {
                                    format!("{:.1} MB", font.bytes as f64 / 1_000_000.)
                                });
                                if installed {
                                    let current = if view.latin {
                                        &self.reader_font_family
                                    } else {
                                        &self.reader_cjk_font_family
                                    };
                                    if current == &font.name_zh {
                                        let (mark, _) = ui.allocate_exact_size(
                                            egui::vec2(14., 14.),
                                            egui::Sense::hover(),
                                        );
                                        let stroke =
                                            egui::Stroke::new(1.5_f32, ui.visuals().text_color());
                                        ui.painter().line_segment(
                                            [
                                                mark.left_center(),
                                                mark.center_bottom() - egui::vec2(2., 2.),
                                            ],
                                            stroke,
                                        );
                                        ui.painter().line_segment(
                                            [
                                                mark.center_bottom() - egui::vec2(2., 2.),
                                                mark.right_top() + egui::vec2(0., 2.),
                                            ],
                                            stroke,
                                        );
                                        ui.label(self.i18n.t("fonts.selected"));
                                    } else {
                                        if ui
                                        .add_enabled(
                                            self.css_policy
                                                != reader_core::epub::native::CssPolicy::Publisher
                                                && current != &font.name_zh,
                                            egui::Button::new(self.i18n.t("fonts.use")),
                                        )
                                        .clicked()
                                    {
                                        add_installed(
                                            &self.data_dir,
                                            &mut self.system_font_names,
                                            &mut self.system_font_paths,
                                        );
                                        if view.latin {
                                            self.reader_font_family = font.name_zh.clone();
                                        } else {
                                            self.reader_cjk_font_family = font.name_zh.clone();
                                        }
                                        self.pages_dirty = true;
                                        self.embedded_fonts_registered = false;
                                        self.defer_custom_font_for_frame = true;
                                    }
                                    }
                                    ui.label(self.i18n.t("fonts.installed"));
                                } else if let Some(Job::Downloading(count)) = snapshot.get(&font.id)
                                {
                                    ui.add(
                                        egui::ProgressBar::new(*count as f32 / font.bytes as f32)
                                            .desired_width(100.),
                                    );
                                    ui.ctx().request_repaint_after(
                                        std::time::Duration::from_millis(100),
                                    );
                                } else {
                                    if matches!(snapshot.get(&font.id), Some(Job::Failed)) {
                                        ui.colored_label(
                                            ui.visuals().error_fg_color,
                                            self.i18n.t("fonts.failed"),
                                        );
                                    }
                                    if ui
                                        .button(self.i18n.t(if snapshot.contains_key(&font.id) {
                                            "fonts.retry"
                                        } else {
                                            "fonts.download"
                                        }))
                                        .clicked()
                                    {
                                        jobs.lock()
                                            .unwrap()
                                            .insert(font.id.clone(), Job::Downloading(0));
                                        let jobs = jobs.clone();
                                        let font = font.clone();
                                        let directory = PathBuf::from(&self.data_dir).join("fonts");
                                        let ctx = ui.ctx().clone();
                                        std::thread::spawn(move || {
                                            let result = download(&font, &directory, |n| {
                                                jobs.lock()
                                                    .unwrap()
                                                    .insert(font.id.clone(), Job::Downloading(n));
                                            });
                                            jobs.lock().unwrap().insert(
                                                font.id,
                                                if result.is_ok() {
                                                    Job::Done
                                                } else {
                                                    Job::Failed
                                                },
                                            );
                                            ctx.request_repaint();
                                        });
                                    }
                                }
                            });
                            ui.push_id(&font.id, |ui| {
                                ui.collapsing(
                                    format!("{} · {}", self.i18n.t("fonts.details"), font.license),
                                    |ui| {
                                        ui.label(self.i18n.t("fonts.open_note"));
                                        ui.hyperlink_to(
                                            self.i18n.t("fonts.official"),
                                            &font.project_url,
                                        );
                                        ui.hyperlink_to(
                                            self.i18n.t("fonts.official_terms"),
                                            &font.license_url,
                                        );
                                        ui.label(egui::RichText::new(&font.license_text).small());
                                    },
                                );
                            });
                        }
                        for font in external()
                            .iter()
                            .filter(|f| (f.language == "en") == view.latin)
                        {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(self.i18n.t("fonts.external"))
                                    .small()
                                    .weak(),
                            );
                            ui.label(
                                egui::RichText::new(if self.i18n.language().code() == "zh_cn" {
                                    &font.name_zh
                                } else {
                                    &font.name_en
                                })
                                .strong(),
                            );
                            ui.label(
                                egui::RichText::new(if self.i18n.language().code() == "zh_cn" {
                                    &font.description_zh
                                } else {
                                    &font.description_en
                                })
                                .small()
                                .weak(),
                            );
                            ui.label(
                                egui::RichText::new(self.i18n.t("fonts.personal_note")).small(),
                            );
                            ui.horizontal_wrapped(|ui| {
                                ui.hyperlink_to(self.i18n.t("fonts.official"), &font.project_url);
                                ui.hyperlink_to(
                                    self.i18n.t("fonts.official_terms"),
                                    &font.license_url,
                                );
                            });
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} · {}",
                                    self.i18n.t("fonts.checked"),
                                    font.checked_at
                                ))
                                .small()
                                .weak(),
                            );
                        }
                        if !view.latin {
                            ui.label(
                                egui::RichText::new(self.i18n.t("fonts.import_note"))
                                    .small()
                                    .weak(),
                            );
                        }
                    });
            });
        ui.ctx().data_mut(|d| d.insert_temp(id, view));
    }
}
