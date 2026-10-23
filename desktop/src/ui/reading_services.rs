use crate::app::ReaderApp;
use eframe::egui;
pub struct ServicePanel {
    target: String,
    kind: &'static str,
    text: String,
    result: Option<Result<String, String>>,
    pending: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
}
impl ReaderApp {
    pub fn start_reading_service(&mut self, kind: &'static str, text: String, ctx: &egui::Context) {
        self.start_reading_service_target(kind, text, "zh".into(), ctx);
    }
    fn start_reading_service_target(
        &mut self,
        kind: &'static str,
        text: String,
        target: String,
        ctx: &egui::Context,
    ) {
        let (endpoint, key) = if kind == "dictionary" {
            (
                self.dictionary_api_url.clone(),
                self.dictionary_api_key.clone(),
            )
        } else {
            (
                self.translate_api_url.clone(),
                self.translate_api_key.clone(),
            )
        };
        let (model, provider) = if kind == "dictionary" {
            (
                self.dictionary_api_model.clone(),
                self.dictionary_api_provider.clone(),
            )
        } else {
            (
                self.translate_api_model.clone(),
                self.translate_api_provider.clone(),
            )
        };
        let mut panel = ServicePanel {
            target: target.clone(),
            kind,
            text: text.clone(),
            result: None,
            pending: None,
        };
        if !endpoint.trim().is_empty() {
            let (tx, rx) = std::sync::mpsc::channel();
            panel.pending = Some(rx);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(reader_core::reading_services::query(
                    &reader_core::reading_services::ServiceRequest {
                        endpoint,
                        key,
                        text,
                        kind: kind.into(),
                        target,
                        model,
                        provider,
                    },
                ));
                ctx.request_repaint();
            });
        }
        self.reading_service = Some(panel);
    }
    pub fn render_reading_service(&mut self, ctx: &egui::Context) {
        let Some(panel) = self.reading_service.as_mut() else {
            return;
        };
        if let Some(result) = panel.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            panel.pending = None;
            panel.result = Some(result);
        }
        let mut open = true;
        let mut retry = false;
        let mut settings = false;
        egui::Window::new(self.i18n.t(if panel.kind=="dictionary"{"context.dictionary"}else{"context.translate"})).id(egui::Id::new("native-reading-service")).open(&mut open).default_width(440.).show(ctx,|ui|{
   egui::ScrollArea::vertical().max_height(420.).show(ui,|ui|{
    ui.label(egui::RichText::new(&panel.text).weak());ui.separator();
    if panel.kind=="translate"{ui.add_enabled_ui(panel.pending.is_none(),|ui|{ui.label(self.i18n.t("services.target"));retry|=ui.selectable_value(&mut panel.target,"zh".into(),self.i18n.t("fonts.chinese")).changed();retry|=ui.selectable_value(&mut panel.target,"en".into(),self.i18n.t("fonts.english")).changed();});}
    if panel.pending.is_some(){ui.horizontal(|ui|{ui.spinner();ui.label(self.i18n.t("services.loading"));});}
    match &panel.result {
     Some(Ok(value))=>{ui.add(egui::Label::new(value).selectable(true).wrap());},
     Some(Err(error))=>{ui.colored_label(egui::Color32::RED,error);retry=ui.button(self.i18n.t("services.retry")).clicked();},
     None if panel.pending.is_none()=>{
      ui.label(self.i18n.t("services.missing"));settings=ui.button(self.i18n.t("services.title")).clicked();
      if ui.button(self.i18n.t(if panel.kind=="dictionary"{"services.online_dictionary"}else{"services.online_translation"})).clicked(){let text=urlencoding::encode(&panel.text);let url=if panel.kind=="dictionary"{format!("https://www.zdic.net/hans/{text}")}else{format!("https://translate.google.com/?sl=auto&tl=zh-CN&text={text}")};let _=open::that(url);}
     },_=>{}
    }
   });
  });
        if settings {
            self.show_settings = true;
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("focus-reading-services"), true));
            self.reading_service = None;
            return;
        }
        if retry {
            let (kind, text, target) = (panel.kind, panel.text.clone(), panel.target.clone());
            self.start_reading_service_target(kind, text, target, ctx);
        }
        if !open {
            self.reading_service = None;
        }
    }
}

static SERVICE_KEYS: std::sync::Mutex<Option<[String; 2]>> = std::sync::Mutex::new(None);
const KEY_SERVICE: &str = "com.epub.reader.reading-services";
pub(crate) fn load_keys() -> Result<[String; 2], String> {
    let mut cached = SERVICE_KEYS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(keys) = cached.as_ref() {
        return Ok(keys.clone());
    }
    let entry = keyring::Entry::new(KEY_SERVICE, "api-keys")
        .map_err(|_| "Cannot read reading service credentials")?;
    let keys = match entry.get_password() {
        Ok(value) => {
            serde_json::from_str(&value).map_err(|_| "Invalid reading service credentials")?
        }
        Err(keyring::Error::NoEntry) => [String::new(), String::new()],
        Err(_) => return Err("Cannot read reading service credentials".into()),
    };
    *cached = Some(keys.clone());
    Ok(keys)
}
pub(crate) fn save_keys(keys: [String; 2]) -> Result<(), String> {
    let mut cached = SERVICE_KEYS.lock().unwrap_or_else(|e| e.into_inner());
    if cached.as_ref() == Some(&keys) || (cached.is_none() && keys.iter().all(String::is_empty)) {
        return Ok(());
    }
    let entry = keyring::Entry::new(KEY_SERVICE, "api-keys")
        .map_err(|_| "Cannot save reading service credentials")?;
    if keys.iter().all(String::is_empty) {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => return Err("Cannot remove reading service credentials".into()),
        }
    } else {
        entry
            .set_password(
                &serde_json::to_string(&keys)
                    .map_err(|_| "Cannot encode reading service credentials")?,
            )
            .map_err(|_| "Cannot save reading service credentials")?;
    }
    *cached = Some(keys);
    Ok(())
}
