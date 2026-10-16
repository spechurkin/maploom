use maploom::{
    document::*,
    formats,
    gpu_scene::{SceneOptions, SceneRenderer},
    render, text,
};
use std::collections::HashSet;

fn label(value: &str) -> Item {
    let mut item = Item::new(
        "Label",
        ItemKind::Text {
            position: Point::new(8., 8.),
            text: value.into(),
            size: 0.48,
            color: [90, 40, 10, 210],
        },
    );
    item.text_style = Some(TextStyle {
        background: true,
        border_width: 4.,
        ..Default::default()
    });
    item
}

#[test]
fn background_padding_is_four_native_pixels_at_every_zoom_and_resolution() {
    for value in [
        "One line",
        "Крепость\nСеверные ворота",
        "Line\n\nwith empty lines\n",
        "",
        "   ",
    ] {
        for ppg in [50., 100., 256.] {
            for scale in [12., 64., 180.] {
                let item = label(value);
                let ctx = egui::Context::default();
                let output = ctx.run(Default::default(), |ctx| {
                    text::paint(
                        &ctx.layer_painter(egui::LayerId::background()),
                        &item,
                        ppg,
                        egui::pos2(400., 400.),
                        scale,
                        1.,
                    )
                });
                let rect = output
                    .shapes
                    .iter()
                    .find_map(|s| {
                        if let egui::Shape::Rect(r) = &s.shape {
                            Some(r)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                // Empty text has a line box but no visible TextShape.
                if let Some(shape) = output.shapes.iter().find_map(|s| {
                    if let egui::Shape::Text(t) = &s.shape {
                        Some(t)
                    } else {
                        None
                    }
                }) {
                    let content = text::content_rect(&shape.galley).translate(shape.pos.to_vec2());
                    for gap in [
                        content.left() - rect.rect.left(),
                        content.top() - rect.rect.top(),
                        rect.rect.right() - content.right(),
                        rect.rect.bottom() - content.bottom(),
                    ] {
                        assert!(
                            (gap / scale * ppg - 4.).abs() < 0.002,
                            "{value:?}, ppg={ppg}, scale={scale}, gap={gap}"
                        );
                    }
                }
                assert_eq!(rect.stroke_kind, egui::StrokeKind::Outside);
            }
        }
    }
}

#[test]
fn content_and_font_changes_resize_bounds_and_move_preserves_size() {
    let mut item = label("Short");
    let a = text::bounds(&item, 100.);
    if let ItemKind::Text { text, .. } = &mut item.kind {
        *text = "A much longer label\nВторая строка".into();
    }
    let b = text::bounds(&item, 100.);
    assert!(b.right - b.x > a.right - a.x);
    assert!(b.bottom - b.y > a.bottom - a.y);
    item.translate(Point::new(3., -2.));
    let moved = text::bounds(&item, 100.);
    assert!((moved.x - b.x - 3.).abs() < 0.0001);
    assert!((moved.y - b.y + 2.).abs() < 0.0001);
    let fonts = text::local_fonts();
    assert!(fonts.windows(2).all(|p| p[0] < p[1]));
    if fonts.iter().any(|f| f == "Arial") && fonts.iter().any(|f| f == "Courier New") {
        item.text_style.as_mut().unwrap().font = "Arial".into();
        let proportional = text::bounds(&item, 100.);
        item.text_style.as_mut().unwrap().font = "Courier New".into();
        let monospace = text::bounds(&item, 100.);
        assert!(
            ((proportional.right - proportional.x) - (monospace.right - monospace.x)).abs() > 0.05
        );
    }
}

#[test]
fn native_save_keeps_local_face_and_box_and_legacy_labels_still_load() {
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    let mut item = label("Подземелье\nEntrance");
    item.text_style.as_mut().unwrap().font = "Missing face preserved".into();
    item.text_style.as_mut().unwrap().background_color = [35, 80, 120, 100];
    p.levels[0].layers[2].items.push(item.clone());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text.mloom");
    formats::save(&path, &p, &Default::default()).unwrap();
    assert_eq!(
        formats::load(&path).unwrap().project.levels[0].layers[2].items[0],
        item
    );
    let mut old = serde_json::to_value(&item).unwrap();
    old.as_object_mut().unwrap().remove("text_style");
    let old: Item = serde_json::from_value(old).unwrap();
    assert!(old.text_style.is_none());
    let fallback = text::bounds(&old, 100.);
    let missing = text::bounds(&item, 100.);
    assert!((missing.right - missing.x - (fallback.right - fallback.x) - 0.16).abs() < 0.001);
    item.text_style.as_mut().unwrap().border_width = -1.;
    p.levels[0].layers[2].items[0] = item;
    assert!(p.validate().is_err());
}

#[test]
fn gpu_bounds_and_svg_export_support_local_fonts_multiline_text_and_styled_box() {
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    let mut item = label("Крепость\nEntrance");
    item.text_style.as_mut().unwrap().font = text::local_fonts()
        .into_iter()
        .find(|f| f == "Arial")
        .unwrap_or_default();
    p.levels[0].layers[2].items.push(item.clone());
    let mut renderer = SceneRenderer::default();
    let scene = renderer
        .scene(
            &p,
            &Default::default(),
            SceneOptions {
                level: 0,
                grid: false,
                lights: false,
                revision: 1,
                known_meshes: HashSet::new(),
            },
            &HashSet::new(),
        )
        .unwrap();
    let b = &scene.text_bounds[&item.id];
    let expected = text::bounds(&item, p.pixels_per_grid as f32);
    assert_eq!(b.x, expected.x);
    assert_eq!(b.bottom, expected.bottom);
    assert!(!scene.textures.is_empty());
    assert!(scene.meshes.iter().any(|m| m.vertex_count > 8));
    let svg = render::svg(
        &p,
        &Default::default(),
        render::ExportOptions {
            level: 0,
            pixels_per_grid: 100,
            grid: false,
            lights: false,
            jpeg_quality: 90,
        },
    )
        .unwrap();
    assert!(svg.contains("stroke-width=\"0.04\""));
    assert!(svg.contains("<path fill=\"#5a280a\""));
    assert!(
        !svg.contains("<text "),
        "font outlines must remain portable"
    );
    assert!(svg.contains("rx=\"0.06\""));
}

#[test]
#[ignore = "Requires a hardware GPU; run explicitly for native text export verification"]
fn raster_export_uses_local_fonts_and_keeps_box_dimensions_after_content_changes() {
    let mut p = Project {
        width: 12,
        height: 8,
        background_asset: None,
        background: [20, 30, 40, 255],
        ..Default::default()
    };
    let mut item = label("Крепость\nNorth gate");
    if let ItemKind::Text {
        position, color, ..
    } = &mut item.kind
    {
        *position = Point::new(6., 4.);
        *color = [0, 0, 0, 255];
    }
    item.text_style.as_mut().unwrap().font = text::local_fonts()
        .into_iter()
        .find(|f| f == "Arial")
        .unwrap_or_default();
    item.text_style.as_mut().unwrap().border_color = [200, 10, 10, 255];
    item.text_style.as_mut().unwrap().shape = TextBoxShape::Square;
    p.levels[0].layers[2].items.push(item);
    let options = || render::ExportOptions {
        level: 0,
        pixels_per_grid: 100,
        grid: false,
        lights: false,
        jpeg_quality: 90,
    };
    let (image, adapter) = render::rasterize(&p, &Default::default(), options()).unwrap();
    assert!(
        image.pixels().any(|p| p[0] < 5 && p[1] < 5 && p[2] < 5),
        "text glyphs missing"
    );
    let extent = |im: &image::RgbaImage| {
        let pixels: Vec<_> = im
            .enumerate_pixels()
            .filter(|(_, _, p)| p[0] > 180 && (p[1] < 30 || p[1] > 240))
            .map(|(x, y, _)| (x, y))
            .collect();
        (
            pixels.iter().map(|p| p.0).max().unwrap() - pixels.iter().map(|p| p.0).min().unwrap(),
            pixels.iter().map(|p| p.1).max().unwrap() - pixels.iter().map(|p| p.1).min().unwrap(),
        )
    };
    let old = extent(&image);
    if let Some(path) = std::env::var_os("MAPLOOM_TEXT_PREVIEW") {
        image.save(path).unwrap();
    }
    if let ItemKind::Text { text, .. } = &mut p.levels[0].layers[2].items[0].kind {
        *text = "A much longer label\nВторая строка\nТретья строка".into();
    }
    let (image, _) = render::rasterize(&p, &Default::default(), options()).unwrap();
    let new = extent(&image);
    assert!(new.0 > old.0 && new.1 > old.1);
    println!("Native text export GPU: {adapter}");
}
