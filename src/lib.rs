//! Maploom core, organized by responsibility.

pub mod editor;
pub mod platform;
pub mod rendering;
pub mod storage;
pub mod ui;

// Preserve the public module paths used by the CLI and integrations.
pub use editor::{caves, document, editing, geometry, openings, paths, portals, roofs};
pub use platform::{localization, preferences, resources};
pub use rendering::{gpu_scene, light, render, text, water};
pub use storage::{assets, catalog, formats};
#[cfg(feature = "desktop")]
pub use ui::desktop;
