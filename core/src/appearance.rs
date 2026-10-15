//! Appearance inheritance is independent of reading colors, images and typography.
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutomaticSource {
    #[default]
    System,
    Ambient,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisplayChoice {
    Light,
    Dark,
    #[default]
    Automatic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceLinkage {
    pub version: u8,
    pub source: AutomaticSource,
    pub app_linked: bool,
    pub reading_linked: bool,
    pub app_manual: bool,
    pub reading_mode: DisplayChoice,
    pub reading_source: AutomaticSource,
}
impl Default for AppearanceLinkage {
    fn default() -> Self {
        Self {
            version: 1,
            source: AutomaticSource::System,
            app_linked: true,
            reading_linked: true,
            app_manual: false,
            reading_mode: DisplayChoice::Automatic,
            reading_source: AutomaticSource::System,
        }
    }
}
impl AutomaticSource {
    pub fn resolve(self, system: bool, ambient: Option<bool>) -> bool {
        match self {
            Self::System => system,
            Self::Ambient => ambient.unwrap_or(system),
        }
    }
}
impl AppearanceLinkage {
    pub fn app_dark(&self, system: bool, ambient: Option<bool>) -> bool {
        if self.app_linked {
            self.source.resolve(system, ambient)
        } else {
            self.app_manual
        }
    }
    pub fn reading_dark(&self, system: bool, ambient: Option<bool>) -> bool {
        if self.reading_linked {
            return self.app_dark(system, ambient);
        }
        match self.reading_mode {
            DisplayChoice::Dark => true,
            DisplayChoice::Light => false,
            DisplayChoice::Automatic => self.reading_source.resolve(system, ambient),
        }
    }
    pub fn uses_ambient(&self) -> bool {
        (self.app_linked && self.source == AutomaticSource::Ambient)
            || (!self.reading_linked
                && self.reading_mode == DisplayChoice::Automatic
                && self.reading_source == AutomaticSource::Ambient)
    }
    pub fn toggle_app_link(&mut self, system: bool, ambient: Option<bool>) {
        if self.app_linked {
            self.app_manual = self.app_dark(system, ambient);
        }
        self.app_linked = !self.app_linked;
    }
    pub fn toggle_reading_link(&mut self, system: bool, ambient: Option<bool>) {
        if self.reading_linked {
            self.reading_mode = if self.reading_dark(system, ambient) {
                DisplayChoice::Dark
            } else {
                DisplayChoice::Light
            };
        }
        self.reading_linked = !self.reading_linked;
    }
    pub fn set_app(&mut self, dark: bool) {
        self.app_manual = dark;
        self.app_linked = false;
    }
    pub fn toggle_reading(&mut self, system: bool, ambient: Option<bool>) {
        let dark = !self.reading_dark(system, ambient);
        if self.reading_linked {
            self.set_app(dark);
        } else {
            self.reading_mode = if dark {
                DisplayChoice::Dark
            } else {
                DisplayChoice::Light
            };
        }
    }
    pub fn migrate(app: bool, reading: bool) -> Self {
        Self {
            app_linked: false,
            reading_linked: app == reading,
            app_manual: app,
            reading_mode: if reading {
                DisplayChoice::Dark
            } else {
                DisplayChoice::Light
            },
            ..Self::default()
        }
    }
}
