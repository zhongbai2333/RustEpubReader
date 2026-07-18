//! UI for initiating and managing local P2P book sharing.
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use eframe::egui;

use reader_core::sharing::{
    auto_sync_session, connect_to_peer, handle_client, resolve_broadcast_addr, start_broadcast,
    start_server, DiscoveredPeer, DiscoveryAnnouncement,
};

use crate::app::ReaderApp;

const FEEDBACK_GITHUB_URL: &str = "https://github.com/zhongbai233/RustEpubReader/issues/new/choose";

fn append_feedback_log(logs: &Arc<Mutex<Vec<String>>>, msg: impl AsRef<str>) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let line = format!("[{ts}] {}", msg.as_ref());
    if reader_core::sharing::is_debug_logging_enabled() {
        eprintln!("[FEEDBACK-LOG] {line}");
    }
    let mut guard = logs.lock().unwrap_or_else(|e| e.into_inner());
    guard.push(line);
    if guard.len() > 600 {
        let remove = guard.len().saturating_sub(600);
        guard.drain(0..remove);
    }
}

impl ReaderApp {
    pub fn render_sharing(&mut self, ctx: &egui::Context) {
        self.render_pairing_dialog(ctx);

        let mut open = self.show_sharing_panel;
        egui::Window::new(self.i18n.t("share.title"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(360.0)
            .frame(
                egui::Frame::window(&ctx.style())
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(ctx, |ui| {
                ui.checkbox(
                    &mut self.auto_start_sharing,
                    self.i18n.t("share.auto_start"),
                );
                ui.add_space(12.0);

                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(self.i18n.t("share.pin_label"))
                            .size(13.0)
                            .color(egui::Color32::from_gray(140)),
                    );
                    ui.add_space(4.0);
                    let pin_response = ui.add(
                        egui::TextEdit::singleline(&mut self.sharing_pin)
                            .font(egui::TextStyle::Heading)
                            .desired_width(120.0)
                            .horizontal_align(egui::Align::Center)
                            .hint_text("0000"),
                    );
                    self.sharing_pin = self
                        .sharing_pin
                        .chars()
                        .filter(|c| c.is_ascii_digit())
                        .take(4)
                        .collect();
                    if pin_response.changed() && self.sharing_server_running {
                        self.stop_sharing_server();
                        self.start_sharing_server();
                    }
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(self.i18n.t("share.pin_hint"))
                            .size(11.0)
                            .color(egui::Color32::from_gray(140)),
                    );
                });

                ui.add_space(12.0);

                ui.vertical_centered(|ui| {
                    if self.sharing_server_running {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("*")
                                    .color(egui::Color32::from_rgb(50, 180, 50))
                                    .size(14.0),
                            );
                            ui.label(self.i18n.t("share.server_running"));
                        });
                        ui.add_space(4.0);
                        if ui
                            .button(
                                egui::RichText::new(self.i18n.t("share.stop_server")).size(15.0),
                            )
                            .clicked()
                        {
                            self.stop_sharing_server();
                        }
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("*")
                                    .color(egui::Color32::from_gray(120))
                                    .size(14.0),
                            );
                            ui.label(self.i18n.t("share.server_stopped"));
                        });
                        ui.add_space(4.0);
                        let pin_valid = self.sharing_pin.len() == 4;
                        let btn = ui.add_enabled(
                            pin_valid,
                            egui::Button::new(
                                egui::RichText::new(self.i18n.t("share.start_server")).size(15.0),
                            ),
                        );
                        if btn.clicked() {
                            self.start_sharing_server();
                        }
                    }
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.label(
                    egui::RichText::new(self.i18n.t("share.discovered_devices"))
                        .strong()
                        .size(13.0),
                );
                ui.add_space(4.0);

                let peers: Vec<DiscoveredPeer> = self
                    .discovered_peers
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                let my_device_id = self
                    .peer_store
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .device_id
                    .clone();
                let visible_peers: Vec<&DiscoveredPeer> = peers
                    .iter()
                    .filter(|p| p.device_id != my_device_id)
                    .collect();

                if visible_peers.is_empty() {
                    ui.label(
                        egui::RichText::new(self.i18n.t("share.no_devices_nearby"))
                            .size(12.0)
                            .color(egui::Color32::from_gray(140)),
                    );
                } else {
                    let mut peer_to_pair: Option<DiscoveredPeer> = None;
                    for peer in &visible_peers {
                        egui::Frame::default()
                            .corner_radius(6.0)
                            .fill(ui.visuals().extreme_bg_color)
                            .inner_margin(egui::Margin::symmetric(8, 6))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(&peer.device_name)
                                                .size(13.0)
                                                .strong(),
                                        );
                                        ui.label(
                                            egui::RichText::new(&peer.addr)
                                                .size(10.0)
                                                .color(egui::Color32::from_gray(140)),
                                        );
                                        // Check if already paired
                                        let is_paired = self
                                            .peer_store
                                            .lock()
                                            .unwrap_or_else(|e| e.into_inner())
                                            .is_paired(&peer.device_id);
                                        if is_paired {
                                            ui.label(
                                                egui::RichText::new(self.i18n.t("share.paired"))
                                                    .size(10.0)
                                                    .color(egui::Color32::from_rgb(100, 160, 100)),
                                            );
                                        }
                                    });
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.button(self.i18n.t("share.connect")).clicked() {
                                                peer_to_pair = Some((*peer).clone());
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(4.0);
                    }
                    if let Some(peer) = peer_to_pair {
                        // Check if already paired 鈥?skip PIN dialog
                        let is_paired = self
                            .peer_store
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .is_paired(&peer.device_id);
                        if is_paired {
                            let addr = peer.addr.clone();
                            let device_id = peer.device_id.clone();
                            self.do_connect_and_sync(addr, String::new(), Some(device_id));
                        } else {
                            self.pairing_dialog_pin.clear();
                            self.pairing_dialog_peer = Some(peer);
                        }
                    }
                }

                let status = self
                    .sharing_status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                if !status.is_empty() {
                    ui.add_space(4.0);
                    let color = if status.contains('❌') {
                        egui::Color32::from_rgb(220, 60, 60)
                    } else if status.contains('✅') {
                        egui::Color32::from_rgb(60, 180, 60)
                    } else {
                        egui::Color32::from_rgb(60, 140, 220)
                    };
                    ui.label(egui::RichText::new(&status).size(12.0).color(color));
                }

                ui.add_space(8.0);
                if ui
                    .button(egui::RichText::new(self.i18n.t("feedback.export_logs")).size(13.0))
                    .clicked()
                {
                    match self.export_feedback_log() {
                        Ok(path) => {
                            self.last_exported_feedback_log = Some(path.clone());
                            self.show_feedback_github_prompt = true;
                            self.push_feedback_log(format!("export feedback log success: {path}"));
                            *self
                                .sharing_status
                                .lock()
                                .unwrap_or_else(|e| e.into_inner()) =
                                self.i18n.tf1("feedback.export_success", &path);
                        }
                        Err(e) => {
                            self.push_feedback_log(format!("export feedback log failed: {e}"));
                            *self
                                .sharing_status
                                .lock()
                                .unwrap_or_else(|er| er.into_inner()) =
                                self.i18n.tf1("feedback.export_failed", &e);
                        }
                    }
                }

                ui.add_space(8.0);

                if ui
                    .button(egui::RichText::new(self.i18n.t("share.refresh_library")).size(13.0))
                    .clicked()
                {
                    self.refresh_library_from_books_dir();
                }

                ui.add_space(8.0);

                let advanced_label = self.i18n.t("share.advanced").to_string();
                ui.collapsing(advanced_label, |ui| {
                    if self.sharing_server_running {
                        ui.label(self.i18n.tf1("share.address", &self.sharing_server_addr));
                    }
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(self.i18n.t("share.connect_to_peer"))
                            .strong()
                            .size(14.0),
                    );
                    ui.horizontal(|ui| {
                        ui.label(self.i18n.t("share.enter_address"));
                        ui.text_edit_singleline(&mut self.connect_addr_input);
                    });
                    ui.horizontal(|ui| {
                        ui.label("PIN:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.connect_pin_input)
                                .hint_text("PIN")
                                .desired_width(60.0),
                        );
                    });
                    if ui.button(self.i18n.t("share.manual_sync")).clicked() {
                        self.connect_and_sync();
                    }
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(self.i18n.t("share.paired_devices"))
                            .strong()
                            .size(14.0),
                    );
                    let paired_list: Vec<(String, String)> = {
                        let store = self.peer_store.lock().unwrap_or_else(|e| e.into_inner());
                        store
                            .paired
                            .iter()
                            .map(|d| (d.device_id.clone(), d.device_name.clone()))
                            .collect()
                    };
                    if paired_list.is_empty() {
                        ui.label(
                            egui::RichText::new(self.i18n.t("share.no_paired"))
                                .size(12.0)
                                .color(egui::Color32::from_gray(140)),
                        );
                    } else {
                        let mut to_remove: Option<String> = None;
                        for (dev_id, dev_name) in &paired_list {
                            ui.horizontal(|ui| {
                                ui.label(self.i18n.tf1("share.device_name", dev_name));
                                if ui
                                    .small_button("🗑")
                                    .on_hover_text("Remove pairing")
                                    .clicked()
                                {
                                    to_remove = Some(dev_id.clone());
                                }
                            });
                        }
                        if let Some(id) = to_remove {
                            let mut store =
                                self.peer_store.lock().unwrap_or_else(|e| e.into_inner());
                            store.remove_paired(&id);
                            store.save(&self.data_dir);
                        }
                    }
                });
            });
        self.show_sharing_panel = open;

        if self.show_feedback_github_prompt {
            let mut prompt_open = self.show_feedback_github_prompt;
            egui::Window::new(self.i18n.t("feedback.open_github"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(self.i18n.t("feedback.ask_open_github"));
                    if let Some(path) = &self.last_exported_feedback_log {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(path).small());
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(self.i18n.t("feedback.not_now")).clicked() {
                            prompt_open = false;
                        }
                        if ui.button(self.i18n.t("feedback.open_github")).clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(FEEDBACK_GITHUB_URL));
                            self.push_feedback_log("open github feedback page");
                            prompt_open = false;
                        }
                    });
                });
            self.show_feedback_github_prompt = prompt_open;
        }
    }

    fn render_pairing_dialog(&mut self, ctx: &egui::Context) {
        let peer = match &self.pairing_dialog_peer {
            Some(p) => p.clone(),
            None => return,
        };

        let mut open = true;
        egui::Window::new(self.i18n.t("share.pair_title"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .frame(
                egui::Frame::window(&ctx.style())
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(&peer.device_name).size(16.0).strong());
                    ui.label(
                        egui::RichText::new(&peer.addr)
                            .size(11.0)
                            .color(egui::Color32::from_gray(140)),
                    );
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(self.i18n.t("share.pair_enter_pin")).size(13.0));
                    ui.add_space(6.0);
                    let pin_edit = ui.add(
                        egui::TextEdit::singleline(&mut self.pairing_dialog_pin)
                            .font(egui::TextStyle::Heading)
                            .desired_width(120.0)
                            .horizontal_align(egui::Align::Center)
                            .hint_text("0000"),
                    );
                    self.pairing_dialog_pin = self
                        .pairing_dialog_pin
                        .chars()
                        .filter(|c| c.is_ascii_digit())
                        .take(4)
                        .collect();
                    ui.add_space(16.0);
                    let pin_ready = self.pairing_dialog_pin.len() == 4;
                    let confirm_pressed = pin_ready
                        && pin_edit.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button(self.i18n.t("share.cancel")).clicked() {
                            self.push_feedback_log("pairing dialog cancelled");
                            self.pairing_dialog_peer = None;
                            self.pairing_dialog_pin.clear();
                        }
                        let ok_btn = ui.add_enabled(
                            pin_ready,
                            egui::Button::new(
                                egui::RichText::new(self.i18n.t("share.pair_connect")).size(14.0),
                            ),
                        );
                        if ok_btn.clicked() || confirm_pressed {
                            self.push_feedback_log(format!(
                                "pairing confirmed for peer={} pin_len={}",
                                peer.addr,
                                self.pairing_dialog_pin.len()
                            ));
                            let addr = peer.addr.clone();
                            let pin = self.pairing_dialog_pin.clone();
                            let device_id = peer.device_id.clone();
                            self.do_connect_and_sync(addr, pin, Some(device_id));
                            self.pairing_dialog_peer = None;
                            self.pairing_dialog_pin.clear();
                        }
                    });
                });
            });

        if !open {
            self.push_feedback_log("pairing dialog dismissed");
            self.pairing_dialog_peer = None;
            self.pairing_dialog_pin.clear();
        }
    }

    pub fn start_sharing_server(&mut self) {
        if self.sharing_server_running {
            return;
        }
        self.push_feedback_log(format!(
            "start sharing server requested, pin={} ",
            self.sharing_pin
        ));
        self.server_stop_flag.store(false, Ordering::SeqCst);

        let data_dir = self.data_dir.clone();
        let books_dir = {
            let dir = std::path::PathBuf::from(&self.data_dir).join("books");
            let _ = std::fs::create_dir_all(&dir);
            dir.to_string_lossy().to_string()
        };
        let pin = self.sharing_pin.clone();
        let store = self.peer_store.clone();
        let stop_flag = self.server_stop_flag.clone();
        let status = self.sharing_status.clone();
        let shared_book_paths = self.shared_book_paths.clone();

        let (device_id, device_name) = {
            let s = store.lock().unwrap_or_else(|e| e.into_inner());
            (s.device_id.clone(), s.device_name.clone())
        };

        match start_server("0.0.0.0:0", &data_dir, &books_dir, &pin, store.clone()) {
            Ok((listener, addr)) => {
                let resolved = resolve_broadcast_addr(&addr);
                self.push_feedback_log(format!("sharing server started at {}", resolved));
                self.sharing_server_addr = resolved.clone();
                self.sharing_server_running = true;
                *status.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
                start_broadcast(
                    DiscoveryAnnouncement {
                        device_id,
                        device_name,
                        addr: resolved,
                    },
                    self.server_stop_flag.clone(),
                );
                listener.set_nonblocking(true).ok();
                std::thread::spawn(move || loop {
                    if stop_flag.load(Ordering::SeqCst) {
                        break;
                    }
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            stream
                                .set_read_timeout(Some(std::time::Duration::from_secs(60)))
                                .ok();
                            stream.set_nonblocking(false).ok();
                            let dd = data_dir.clone();
                            let bd = books_dir.clone();
                            let p = pin.clone();
                            let s = store.clone();
                            let st = status.clone();
                            let ebp = shared_book_paths
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .clone();
                            std::thread::spawn(move || {
                                if let Err(e) = handle_client(&mut stream, &dd, &bd, &p, s, &ebp) {
                                    *st.lock().unwrap_or_else(|e| e.into_inner()) =
                                        format!("Client error: {e}");
                                }
                            });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                        }
                        Err(_) => {
                            break;
                        }
                    }
                });
            }
            Err(e) => {
                self.push_feedback_log(format!("sharing server start failed: {e}"));
                *status.lock().unwrap_or_else(|e| e.into_inner()) = format!("Server error: {e}");
            }
        }
    }

    fn stop_sharing_server(&mut self) {
        self.server_stop_flag.store(true, Ordering::SeqCst);
        self.sharing_server_running = false;
        self.sharing_server_addr.clear();
        self.push_feedback_log("sharing server stopped");
    }

    fn do_connect_and_sync(&mut self, addr: String, pin: String, remote_device_id: Option<String>) {
        self.push_feedback_log(format!(
            "connect_and_sync start addr={} remote_id={:?} pin_len={}",
            addr,
            remote_device_id,
            pin.len()
        ));
        let store_arc = self.peer_store.clone();
        let status = self.sharing_status.clone();
        let feedback_logs = self.feedback_logs.clone();
        let pending_updates = self.pending_sync_updates.clone();
        let library_reload = self.pending_library_reload.clone();
        let data_dir = self.data_dir.clone();
        let books_dir = {
            let dir = std::path::PathBuf::from(&self.data_dir).join("books");
            let _ = std::fs::create_dir_all(&dir);
            dir.to_string_lossy().to_string()
        };
        let extra_book_paths: Vec<String> =
            self.library.books.iter().map(|b| b.path.clone()).collect();
        let connecting_msg = self.i18n.t("share.status_connecting").to_string();
        let connected_msg = self.i18n.t("share.status_connected").to_string();
        let sync_done_msg = self.i18n.t("share.status_sync_done").to_string();
        let connect_fail_tpl = self.i18n.t("share.status_connect_failed").to_string();
        let sync_fail_tpl = self.i18n.t("share.status_sync_failed").to_string();
        *status.lock().unwrap_or_else(|e| e.into_inner()) = connecting_msg;

        std::thread::spawn(move || {
            let mut store_snapshot = store_arc.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let pin_opt = if pin.is_empty() {
                None
            } else {
                Some(pin.as_str())
            };
            let device_id_ref = remote_device_id.as_deref();
            append_feedback_log(
                &feedback_logs,
                format!(
                    "connect_to_peer attempt addr={} remote_id={:?}",
                    addr, device_id_ref
                ),
            );
            match connect_to_peer(
                &addr,
                &mut store_snapshot,
                &data_dir,
                device_id_ref,
                pin_opt,
            ) {
                Ok((mut stream, aes_key, mut send_ctr, mut recv_ctr)) => {
                    append_feedback_log(&feedback_logs, "connect_to_peer success");
                    *status.lock().unwrap_or_else(|e| e.into_inner()) = connected_msg;
                    append_feedback_log(&feedback_logs, "auto_sync_session start");
                    match auto_sync_session(
                        &mut stream,
                        &aes_key,
                        &mut send_ctr,
                        &mut recv_ctr,
                        &mut store_snapshot,
                        &data_dir,
                        &books_dir,
                        &extra_book_paths,
                    ) {
                        Ok((changed_progress, remote_books)) => {
                            append_feedback_log(
                                &feedback_logs,
                                format!(
                                    "auto_sync_session success changed_progress={} remote_books={} merged_progress_total={}",
                                    changed_progress.len(),
                                    remote_books.len(),
                                    store_snapshot.progress.len()
                                ),
                            );
                            // Write back updated store
                            *store_arc.lock().unwrap_or_else(|e| e.into_inner()) =
                                store_snapshot.clone();
                            // Signal main thread to reload library from disk
                            // (auto_sync_session already saved downloaded books to library.json)
                            library_reload.store(true, std::sync::atomic::Ordering::SeqCst);
                            // Apply ALL synced progress (includes both changed and existing)
                            {
                                let mut updates =
                                    pending_updates.lock().unwrap_or_else(|e| e.into_inner());
                                updates.extend(store_snapshot.progress.iter().cloned());
                                append_feedback_log(
                                    &feedback_logs,
                                    format!(
                                        "queued pending progress updates count={}",
                                        updates.len()
                                    ),
                                );
                            }
                            *status.lock().unwrap_or_else(|e| e.into_inner()) = sync_done_msg;
                        }
                        Err(e) => {
                            append_feedback_log(
                                &feedback_logs,
                                format!("auto_sync_session failed: {e}"),
                            );
                            *store_arc.lock().unwrap_or_else(|e| e.into_inner()) = store_snapshot;
                            *status.lock().unwrap_or_else(|e| e.into_inner()) =
                                sync_fail_tpl.replacen("{}", &e, 1);
                        }
                    }
                }
                Err(e) => {
                    append_feedback_log(&feedback_logs, format!("connect_to_peer failed: {e}"));
                    // Store might have been updated during pairing
                    *store_arc.lock().unwrap_or_else(|e| e.into_inner()) = store_snapshot;
                    *status.lock().unwrap_or_else(|e| e.into_inner()) =
                        connect_fail_tpl.replacen("{}", &e, 1);
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(4));
            let mut st = status.lock().unwrap_or_else(|e| e.into_inner());
            if st.contains("✅") {
                st.clear();
            }
        });
    }

    fn connect_and_sync(&mut self) {
        let addr = self.connect_addr_input.trim().to_string();
        let pin = self.connect_pin_input.trim().to_string();
        self.push_feedback_log(format!(
            "manual connect requested addr={} pin_len={}",
            addr,
            pin.len()
        ));
        if addr.is_empty() {
            self.push_feedback_log("manual connect aborted: empty address");
            return;
        }
        self.do_connect_and_sync(addr, pin, None);
    }

    fn refresh_library_from_books_dir(&mut self) {
        let books_dir = std::path::PathBuf::from(&self.data_dir).join("books");
        let _ = std::fs::create_dir_all(&books_dir);
        self.push_feedback_log("refresh library from books dir start");

        let mut scanned_epub = 0usize;
        let mut newly_added = 0usize;
        // Scan for untracked epub files and register them
        match std::fs::read_dir(&books_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("epub") {
                        scanned_epub += 1;
                        let full_path = path.to_string_lossy().to_string();
                        if !self.library.books.iter().any(|b| b.path == full_path) {
                            let title = reader_core::epub::EpubBook::read_title(&path)
                                .unwrap_or_else(|| {
                                    path.file_stem()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string()
                                });
                            self.library
                                .add_or_update(&self.data_dir, title, full_path, 0, None);
                            newly_added += 1;
                        }
                    }
                }
            }
            Err(e) => {
                self.push_feedback_log(format!("refresh library read_dir failed: {e}"));
            }
        }

        let mut progress_updates_applied = 0usize;
        // Also apply any synced progress from PeerStore
        let progress = self
            .peer_store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .progress
            .clone();
        for pe in &progress {
            let mut matched_paths = Vec::new();
            for entry in &self.library.books {
                if let Ok(h) = reader_core::epub::EpubBook::file_hash(&entry.path) {
                    if h == pe.book_hash
                        && (entry.last_chapter != pe.chapter
                            || entry.last_block != pe.block
                            || entry.last_char_offset != pe.char_offset)
                    {
                        matched_paths.push(entry.path.clone());
                    }
                }
            }
            for p in matched_paths {
                self.library.update_position(
                    &self.data_dir,
                    &p,
                    pe.chapter,
                    pe.chapter_title.clone(),
                    pe.block,
                    pe.char_offset,
                );
                progress_updates_applied += 1;
            }
        }

        self.push_feedback_log(format!(
            "refresh library done scanned_epub={} newly_added={} progress_updates_applied={}",
            scanned_epub, newly_added, progress_updates_applied
        ));
    }
}
