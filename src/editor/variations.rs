//! Variation scenes share map dimensions and assets, with independent content.
use crate::document::*;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Variation {
    pub id: String,
    pub name: String,
    pub levels: Vec<Level>,
    #[serde(default)]
    pub main_level: Option<String>,
    pub background: Color,
    pub ambient_light: Color,
    #[serde(default)]
    pub background_asset: Option<String>,
}

impl Project {
    /// Visit every independent scene once; the active cache is deliberately skipped.
    pub fn all_levels_mut(&mut self) -> impl Iterator<Item=&mut Level> {
        let active = self.active_variation.as_deref();
        self.levels.iter_mut().chain(
            self.variations
                .iter_mut()
                .filter(move |v| Some(v.id.as_str()) != active)
                .flat_map(|v| &mut v.levels),
        )
    }

    /// Rendering/validation copies a single scene, never the other variations.
    pub fn scene_copy(&self) -> Self {
        Self {
            format: self.format.clone(),
            version: self.version,
            id: self.id.clone(),
            name: self.name.clone(),
            width: self.width,
            height: self.height,
            pixels_per_grid: self.pixels_per_grid,
            background: self.background,
            ambient_light: self.ambient_light,
            background_asset: self.background_asset.clone(),
            use_default_assets: self.use_default_assets,
            levels: self.levels.clone(),
            main_level: self.main_level.clone(),
            active_variation: None,
            variations: vec![],
            assets: self.assets.clone(),
            packs: self.packs.clone(),
            asset_catalogs: self.asset_catalogs.clone(),
            external_assets: self.external_assets.clone(),
            extensions: self.extensions.clone(),
        }
    }

    pub fn variation_scene(&self, variation: &Variation) -> Self {
        let mut scene = self.scene_copy();
        if self.active_variation.as_ref() != Some(&variation.id) {
            scene.levels = variation.levels.clone();
            scene.main_level = variation.main_level.clone();
            scene.background = variation.background;
            scene.ambient_light = variation.ambient_light;
            scene.background_asset = variation.background_asset.clone();
        }
        scene
    }

    pub fn sync_active_variation(&mut self) {
        if let Some(variation) = self
            .variations
            .iter_mut()
            .find(|v| Some(&v.id) == self.active_variation.as_ref())
        {
            variation.levels = self.levels.clone();
            variation.main_level = self.main_level.clone();
            variation.background = self.background;
            variation.ambient_light = self.ambient_light;
            variation.background_asset = self.background_asset.clone();
        }
    }

    fn initialize_variations(&mut self) {
        if self.variations.is_empty() {
            self.active_variation = Some(self.id.clone());
            self.variations.push(Variation {
                id: self.id.clone(),
                name: "Variation 1".into(),
                levels: self.levels.clone(),
                main_level: self.main_level.clone(),
                background: self.background,
                ambient_light: self.ambient_light,
                background_asset: self.background_asset.clone(),
            });
        }
    }

    pub fn select_variation(&mut self, id: &str) -> Result<()> {
        ensure!(
            self.variations.iter().any(|v| v.id == id),
            "Invalid variation"
        );
        self.sync_active_variation();
        let variation = self.variations.iter().find(|v| v.id == id).unwrap().clone();
        let scene = self.variation_scene(&variation);
        self.levels = scene.levels;
        self.main_level = scene.main_level;
        self.background = scene.background;
        self.ambient_light = scene.ambient_light;
        self.background_asset = scene.background_asset;
        self.active_variation = Some(id.into());
        Ok(())
    }

    /// IDs are scoped to each scene, so copied portal references stay intact.
    pub fn add_variation(&mut self, name: String) -> Result<()> {
        ensure!(!name.trim().is_empty(), "Variation name is required");
        self.initialize_variations();
        self.sync_active_variation();
        let mut copy = self
            .variations
            .iter()
            .find(|v| Some(&v.id) == self.active_variation.as_ref())
            .context("Invalid variation")?
            .clone();
        copy.id = id();
        copy.name = name;
        self.active_variation = Some(copy.id.clone());
        self.variations.push(copy);
        Ok(())
    }

    pub fn remove_variation(&mut self) -> Result<()> {
        ensure!(
            self.variations.len() > 1,
            "The last variation cannot be deleted"
        );
        let index = self
            .variations
            .iter()
            .position(|v| Some(&v.id) == self.active_variation.as_ref())
            .context("Invalid variation")?;
        let next = self.variations[if index == 0 { 1 } else { index - 1 }]
            .id
            .clone();
        self.select_variation(&next)?;
        self.variations.remove(index);
        Ok(())
    }

    pub fn rename_variation(&mut self, name: String) -> Result<()> {
        ensure!(!name.trim().is_empty(), "Variation name is required");
        self.initialize_variations();
        self.variations
            .iter_mut()
            .find(|v| Some(&v.id) == self.active_variation.as_ref())
            .context("Invalid variation")?
            .name = name;
        Ok(())
    }

    pub fn validate_variations(&self) -> Result<()> {
        if self.variations.is_empty() {
            ensure!(self.active_variation.is_none(), "Invalid active variation");
            return Ok(());
        }
        ensure!(
            self.variations
                .iter()
                .any(|v| Some(&v.id) == self.active_variation.as_ref()),
            "Invalid active variation"
        );
        let mut ids = HashSet::new();
        for variation in &self.variations {
            ensure!(
                !variation.id.is_empty()
                    && ids.insert(&variation.id)
                    && !variation.name.trim().is_empty(),
                "Invalid variation"
            );
            self.variation_scene(variation).validate()?;
        }
        Ok(())
    }
}
