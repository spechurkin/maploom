//! Local font discovery and shared text layout for the scene, selection and exports.
use crate::document::*;
use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

pub const PADDING: f32 = 4.;
pub const LAYOUT_SCALE: f32 = 64.;

#[cfg(test)]
mod tests {
    #[test]
    fn windows_face_name_wins_over_stale_template_metadata() {
        let stale: Vec<u8> = "Arial".encode_utf16().flat_map(u16::to_be_bytes).collect();
        let actual: Vec<u8> = "My decorative font"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        let make = |platform_id, language_id, name| ttf_parser::name::Name {
            platform_id,
            encoding_id: 1,
            language_id,
            name_id: ttf_parser::name_id::FULL_NAME,
            name,
        };
        let names = [
            make(ttf_parser::PlatformId::Unicode, 0, &stale),
            make(ttf_parser::PlatformId::Windows, 0x0409, &actual),
        ];
        assert_eq!(
            super::font_name(names).as_deref(),
            Some("My decorative font")
        );
    }
}

#[derive(Clone)]
struct LocalFont {
    path: PathBuf,
    index: u32,
}
static CATALOG: OnceLock<BTreeMap<String, LocalFont>> = OnceLock::new();

fn font_name<'a>(names: impl IntoIterator<Item = ttf_parser::name::Name<'a>>) -> Option<String> {
    // Custom fonts often retain a template name in the platform-0 records.
    // Prefer Windows English metadata, then other Windows / Unicode names.
    names
        .into_iter()
        .filter(|n| n.name_id == ttf_parser::name_id::FULL_NAME)
        .filter_map(|n| {
            let rank = if n.platform_id == ttf_parser::PlatformId::Windows {
                if n.language_id == 0x0409 { 0 } else { 1 }
            } else {
                2
            };
            n.to_string()
                .filter(|name| !name.trim().is_empty())
                .map(|name| (rank, name))
        })
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, name)| name)
}

fn scan(directory: &Path, fonts: &mut BTreeMap<String, LocalFont>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Do not follow symlinks into unrelated directories or cycles.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            scan(&path, fonts);
            continue;
        }
        if !kind.is_file()
            || !path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["ttf", "otf", "ttc", "otc"].contains(&e.to_lowercase().as_str()))
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        for index in 0..ttf_parser::fonts_in_collection(&bytes).unwrap_or(1) {
            let Ok(face) = ttf_parser::Face::parse(&bytes, index) else {
                continue;
            };
            // egui uses outlines, including TrueType and OpenType CFF faces.
            if face.tables().glyf.is_none()
                && face.tables().cff.is_none()
                && face.tables().cff2.is_none()
            {
                continue;
            }
            let name = font_name(face.names());
            if let Some(name) = name {
                fonts.entry(name).or_insert(LocalFont {
                    path: path.clone(),
                    index,
                });
            }
        }
    }
}
fn catalog() -> &'static BTreeMap<String, LocalFont> {
    CATALOG.get_or_init(|| {
        let mut fonts = BTreeMap::new();
        let mut dirs = vec![];
        #[cfg(target_os = "windows")]
        {
            if let Some(root) = std::env::var_os("WINDIR") {
                dirs.push(PathBuf::from(root).join("Fonts"));
            }
            if let Some(root) = std::env::var_os("LOCALAPPDATA") {
                dirs.push(PathBuf::from(root).join("Microsoft/Windows/Fonts"));
            }
        }
        #[cfg(target_os = "macos")]
        {
            dirs.extend([
                PathBuf::from("/System/Library/Fonts"),
                PathBuf::from("/Library/Fonts"),
            ]);
        }
        #[cfg(target_os = "linux")]
        {
            dirs.extend([
                PathBuf::from("/usr/share/fonts"),
                PathBuf::from("/usr/local/share/fonts"),
            ]);
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            #[cfg(target_os = "macos")]
            dirs.push(home.join("Library/Fonts"));
            #[cfg(target_os = "linux")]
            {
                dirs.push(home.join(".fonts"));
                dirs.push(home.join(".local/share/fonts"));
            }
        }
        for dir in dirs {
            scan(&dir, &mut fonts);
        }
        fonts
    })
}
pub fn local_fonts() -> Vec<String> {
    catalog().keys().cloned().collect()
}

#[derive(Default)]
pub struct FontBook {
    definitions: FontDefinitions,
    attempted: BTreeSet<String>,
}
impl FontBook {
    pub fn prepare<'a>(&mut self, ctx: &egui::Context, items: impl IntoIterator<Item = &'a Item>) {
        let mut changed = false;
        for item in items {
            if !matches!(item.kind, ItemKind::Text { .. }) {
                continue;
            }
            let Some(style) = &item.text_style else {
                continue;
            };
            if style.font.is_empty() || !self.attempted.insert(style.font.clone()) {
                continue;
            }
            let Some(local) = catalog().get(&style.font) else {
                continue;
            };
            let Ok(bytes) = std::fs::read(&local.path) else {
                continue;
            };
            let mut data = FontData::from_owned(bytes);
            data.index = local.index;
            let key = format!("local:{}", style.font);
            self.definitions
                .font_data
                .insert(key.clone(), Arc::new(data));
            let mut fallback = self.definitions.families[&FontFamily::Proportional].clone();
            fallback.insert(0, key.clone());
            self.definitions
                .families
                .insert(FontFamily::Name(key.into()), fallback);
            changed = true;
        }
        if changed {
            ctx.set_fonts(self.definitions.clone());
        }
    }
    pub fn font(&self, item: &Item, size: f32) -> FontId {
        let key = format!("local:{}", item.text_style.as_ref().map_or("", |s| &s.font));
        let family = FontFamily::Name(key.into());
        FontId::new(
            size,
            if self.definitions.families.contains_key(&family) {
                family
            } else {
                FontFamily::Proportional
            },
        )
    }
}

/// Font families are installed before the frame begins. Unknown faces use the bundled fallback.
pub fn font_id(ctx: &egui::Context, item: &Item, size: f32) -> FontId {
    let key = format!("local:{}", item.text_style.as_ref().map_or("", |s| &s.font));
    let family = FontFamily::Name(key.into());
    let available = ctx.fonts(|f| f.families().contains(&family));
    FontId::new(
        size,
        if available {
            family
        } else {
            FontFamily::Proportional
        },
    )
}

pub fn content_rect(galley: &egui::Galley) -> Rect {
    // Include line boxes (empty lines and spaces), plus italic overhangs and accents.
    if galley.mesh_bounds.is_finite() {
        galley.rect.union(galley.mesh_bounds)
    } else {
        galley.rect
    }
}

pub fn paint(
    painter: &egui::Painter,
    item: &Item,
    ppg: f32,
    position: Pos2,
    scale: f32,
    opacity: f32,
) {
    let ItemKind::Text {
        text, size, color, ..
    } = &item.kind
    else {
        return;
    };
    let tint = |mut c: Color| {
        c[3] = (c[3] as f32 * opacity) as u8;
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
    };
    let galley = painter.layout_no_wrap(
        text.clone(),
        font_id(painter.ctx(), item, size * scale),
        tint(*color),
    );
    let content = content_rect(&galley);
    let origin = position - content.center().to_vec2();
    if let Some(style) = item.text_style.as_ref().filter(|s| s.background) {
        let padding = PADDING / ppg * scale;
        let rect = content.translate(origin.to_vec2()).expand(padding);
        let radius = if style.shape == TextBoxShape::Rounded {
            (6. / ppg * scale).min(rect.height() / 2.)
        } else {
            0.
        };
        painter.rect(
            rect,
            radius,
            tint(style.background_color),
            egui::Stroke::new(style.border_width / ppg * scale, tint(style.border_color)),
            egui::StrokeKind::Outside,
        );
    }
    painter.galley(origin, galley, tint(*color));
}

#[derive(Default)]
struct Measurer {
    ctx: egui::Context,
    fonts: FontBook,
}
static MEASURER: OnceLock<Mutex<Measurer>> = OnceLock::new();
pub fn layout(item: &Item) -> Arc<egui::Galley> {
    let mut m = MEASURER
        .get_or_init(|| Mutex::new(Measurer::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Measurer { ctx, fonts } = &mut *m;
    fonts.prepare(ctx, [item]);
    let mut result = None;
    let _ = ctx.run(Default::default(), |ctx| {
        if let ItemKind::Text { text, size, .. } = &item.kind {
            result = Some(
                ctx.layer_painter(egui::LayerId::background())
                    .layout_no_wrap(
                        text.clone(),
                        fonts.font(item, size * LAYOUT_SCALE),
                        Color32::WHITE,
                    ),
            );
        }
    });
    result.expect("text layout requires a text item")
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct TextBounds {
    pub x: f32,
    pub y: f32,
    pub right: f32,
    pub bottom: f32,
}
pub fn bounds(item: &Item, ppg: f32) -> TextBounds {
    let ItemKind::Text { position, .. } = item.kind else {
        panic!("text bounds require text");
    };
    let galley = layout(item);
    let content = content_rect(&galley);
    let margin = item
        .text_style
        .as_ref()
        .filter(|s| s.background)
        .map_or(0., |s| (PADDING + s.border_width) / ppg);
    let half = content.size() / LAYOUT_SCALE / 2. + egui::Vec2::splat(margin);
    TextBounds {
        x: position.x - half.x,
        y: position.y - half.y,
        right: position.x + half.x,
        bottom: position.y + half.y,
    }
}

/// Export glyph outlines so SVG retains the selected local font on other computers.
pub fn svg(item: &Item, ppg: f32) -> String {
    let ItemKind::Text {
        position, color, ..
    } = &item.kind
    else {
        return String::new();
    };
    let galley = layout(item);
    let content = content_rect(&galley);
    let origin = egui::vec2(position.x, position.y) - content.center().to_vec2() / LAYOUT_SCALE;
    let mut out = String::new();
    let rgba = |c: Color| {
        format!(
            "fill=\"#{:02x}{:02x}{:02x}\" fill-opacity=\"{}\"",
            c[0],
            c[1],
            c[2],
            c[3] as f32 / 255.
        )
    };
    if let Some(s) = item.text_style.as_ref().filter(|s| s.background) {
        let rect = Rect::from_min_max(
            content.min / LAYOUT_SCALE + origin,
            content.max / LAYOUT_SCALE + origin,
        )
        .expand(PADDING / ppg);
        let border = s.border_width / ppg;
        // SVG strokes are centered: offset half the border outward to preserve the 4 px inner gap.
        let rect = rect.expand(border / 2.);
        let radius = if s.shape == TextBoxShape::Rounded {
            (6. / ppg).min(rect.height() / 2.)
        } else {
            0.
        };
        out.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" {} stroke=\"#{:02x}{:02x}{:02x}\" stroke-opacity=\"{}\" stroke-width=\"{}\"/>",
                              rect.min.x, rect.min.y, rect.width(), rect.height(), radius, rgba(s.background_color), s.border_color[0], s.border_color[1], s.border_color[2], s.border_color[3] as f32 / 255., border));
    }
    let mut definitions = FontDefinitions::default();
    let mut keys = definitions.families[&FontFamily::Proportional].clone();
    if let Some(local) = item
        .text_style
        .as_ref()
        .and_then(|s| catalog().get(&s.font))
    {
        if let Ok(bytes) = std::fs::read(&local.path) {
            let mut data = FontData::from_owned(bytes);
            data.index = local.index;
            definitions
                .font_data
                .insert("selected".into(), Arc::new(data));
            keys.insert(0, "selected".into());
        }
    }
    struct Outline(String);
    impl ttf_parser::OutlineBuilder for Outline {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.push_str(&format!("M{x} {}", -y));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.push_str(&format!("L{x} {}", -y));
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.0.push_str(&format!("Q{x1} {} {x} {}", -y1, -y));
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.0
                .push_str(&format!("C{x1} {} {x2} {} {x} {}", -y1, -y2, -y));
        }
        fn close(&mut self) {
            self.0.push('Z');
        }
    }
    for row in &galley.rows {
        for glyph in &row.glyphs {
            for key in &keys {
                let data = &definitions.font_data[key];
                let Ok(face) = ttf_parser::Face::parse(&data.font, data.index) else {
                    continue;
                };
                let Some(id) = face.glyph_index(glyph.chr) else {
                    continue;
                };
                let mut path = Outline(String::new());
                if face.outline_glyph(id, &mut path).is_some() {
                    let factor = glyph.font_impl_height
                        / LAYOUT_SCALE
                        / (face.ascender() as f32 - face.descender() as f32);
                    let x = origin.x + (row.pos.x + glyph.pos.x) / LAYOUT_SCALE;
                    let y = origin.y + (row.pos.y + glyph.pos.y) / LAYOUT_SCALE;
                    out.push_str(&format!(
                        "<path {} transform=\"translate({x} {y}) scale({factor})\" d=\"{}\"/>",
                        rgba(*color),
                        path.0
                    ));
                }
                break;
            }
        }
    }
    out
}
