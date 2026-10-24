//! Polish modes and translation target (FR-4, FR-8).

/// `Raw` never calls the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    Raw,
    Light,
    #[default]
    Polish,
    Formal,
}

/// v1 translation targets (FR-8). Default suggestion is English.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetLang {
    #[default]
    En,
    ZhTw,
    Ja,
}

#[derive(Debug, Clone, Default)]
pub struct PolishOptions {
    pub mode: Mode,
    /// `None` = no translation.
    pub translate_to: Option<TargetLang>,
    pub glossary: Vec<String>,
}

impl PolishOptions {
    /// Whether this request needs the API at all.
    pub fn needs_api(&self) -> bool {
        self.mode != Mode::Raw || self.translate_to.is_some()
    }
}
