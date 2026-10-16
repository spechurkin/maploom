use serde::{Deserialize, Serialize};
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub version: u32,
    pub dark: bool,
    pub theme: Option<String>,
    pub theme_variant: Option<String>,
    pub accent: [u8; 3],
    pub locale: String,
    pub default_assets: bool,
    pub asset_directories: Vec<AssetDirectory>,
    pub map_size_presets: Vec<MapSizePreset>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapSizePreset {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssetDirectory {
    pub id: String,
    pub name: String,
    pub path: String,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            dark: true,
            theme: None,
            theme_variant: None,
            accent: [137, 160, 109],
            locale: "en".into(),
            default_assets: true,
            asset_directories: vec![],
            map_size_presets: vec![],
        }
    }
}
pub fn path() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("MAPLOOM_SETTINGS_DIR") {
        return std::path::PathBuf::from(dir).join("settings.json");
    }
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Maploom/settings.json")
}
impl Preferences {
    pub fn load() -> Self {
        std::fs::read(path())
            .ok()
            .filter(|b| b.len() < 65536)
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .filter(|p| p.version == 1)
            .unwrap_or_default()
    }
    pub fn save(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.map_size_presets.len() <= 100,
            "Too many map size presets"
        );
        let mut ids = std::collections::HashSet::new();
        for preset in &self.map_size_presets {
            anyhow::ensure!(
                !preset.id.is_empty()
                    && ids.insert(&preset.id)
                    && !preset.name.trim().is_empty()
                    && preset.name.chars().count() <= 80
                    && (1..=512).contains(&preset.width)
                    && (1..=512).contains(&preset.height),
                "Invalid map size preset"
            );
        }
        let bytes = serde_json::to_vec_pretty(self)?;
        crate::assets::atomic_write(&path(), |f| {
            use std::io::Write;
            f.write_all(&bytes)?;
            Ok(())
        })
    }
}
