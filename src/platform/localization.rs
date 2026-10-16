use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Serialize, Deserialize)]
pub struct Locale {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    pub strings: BTreeMap<String, String>,
}
impl Locale {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        ensure!(bytes.len() <= 1024 * 1024, "Locale file exceeds 1 MiB");
        let locale: Self = serde_json::from_slice(bytes)?;
        ensure!(
            locale.format == "maploom/locale" && locale.version == 1,
            "Unsupported locale format"
        );
        ensure!(
            !locale.id.is_empty()
                && locale.id.len() <= 32
                && locale
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "Invalid locale ID"
        );
        ensure!(
            !locale.name.trim().is_empty(),
            "Locale display name is empty"
        );
        ensure!(
            locale.strings.len() <= 10000
                && locale
                    .strings
                    .iter()
                    .all(|(k, v)| !k.is_empty() && k.len() <= 8192 && v.len() <= 16384),
            "Invalid locale strings"
        );
        Ok(locale)
    }
}
pub struct Locales {
    pub catalogs: BTreeMap<String, Locale>,
    pub active: String,
    pub warnings: Vec<String>,
}
impl Default for Locales {
    fn default() -> Self {
        let en =
            Locale::parse(include_bytes!("../../locales/en.json")).expect("Bundled English locale");
        let ru =
            Locale::parse(include_bytes!("../../locales/ru.json")).expect("Bundled Russian locale");
        Self {
            catalogs: BTreeMap::from([("en".into(), en), ("ru".into(), ru)]),
            active: "en".into(),
            warnings: vec![],
        }
    }
}
impl Locales {
    // English source strings are stable lookup keys; an empty translation falls back to English.
    pub fn tr(&self, _r: &str, key: &str) -> String {
        self.catalogs
            .get(&self.active)
            .and_then(|l| l.strings.get(key))
            .filter(|s| !s.trim().is_empty())
            .cloned()
            .or_else(|| {
                self.catalogs
                    .get("en")
                    .and_then(|l| l.strings.get(key))
                    .cloned()
            })
            .unwrap_or_else(|| key.into())
    }
    pub fn scan(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path
                .extension()
                .is_none_or(|e| !e.eq_ignore_ascii_case("json"))
            {
                continue;
            }
            let result = (|| -> Result<Locale> {
                ensure!(entry.metadata()?.len() <= 1024 * 1024, "Locale too large");
                Locale::parse(&std::fs::read(&path)?)
            })();
            match result {
                Ok(locale) => {
                    self.catalogs.insert(locale.id.clone(), locale);
                }
                Err(e) => self.warnings.push(format!("{}: {e:#}", path.display())),
            }
        }
    }
    pub fn startup() -> Self {
        let mut locales = Self::default();
        for directory in crate::resources::directories() {
            locales.scan(&directory.join("locales"));
        }
        locales
    }
}
