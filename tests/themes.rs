use maploom::platform::themes;
use std::fs;

fn theme(root: &std::path::Path, name: &str) -> std::path::PathBuf {
    let folder = root.join(name);
    fs::create_dir_all(folder.join("nested")).unwrap();
    fs::write(folder.join("theme.json"), r#"{"format":"maploom/theme","api_version":1,"name":"Тема 测试 ✨","stylesheet":"style.css","entry":"nested/main.tsx"}"#).unwrap();
    fs::write(folder.join("style.css"), "/* user bytes */").unwrap();
    fs::write(
        folder.join("nested/main.tsx"),
        "export default api => api.setTokens({});",
    )
    .unwrap();
    folder
}

#[test]
fn unicode_directories_css_tsx_and_unknown_resources_are_external_and_read_only() {
    let root = tempfile::tempdir().unwrap();
    let folder = theme(root.path(), "Моя тема [夜] ✨");
    fs::write(folder.join("nested/font.woff2"), b"a user font").unwrap();
    fs::write(folder.join("arbitrary.bin"), b"arbitrary bytes").unwrap();
    let catalog = themes::scan(root.path());
    assert!(catalog.warnings.is_empty());
    assert_eq!(catalog.themes.len(), 1);
    let id = &catalog.themes[0].id;
    let package = themes::package(root.path(), id).unwrap();
    assert!(package.modules.contains_key("nested/main.tsx"));
    assert_eq!(
        themes::preview_stylesheet(root.path(), id).unwrap(),
        "/* user bytes */"
    );
    assert!(!package.modules.contains_key("nested/font.woff2"));
    assert_eq!(
        themes::resource(root.path(), id, "nested/font.woff2").unwrap(),
        (b"a user font".to_vec(), "font/woff2")
    );
    assert_eq!(
        themes::resource(root.path(), id, "arbitrary.bin")
            .unwrap()
            .0,
        b"arbitrary bytes"
    );
    themes::scan(root.path());
    assert_eq!(
        fs::read(folder.join("style.css")).unwrap(),
        b"/* user bytes */"
    );
}

#[test]
fn unsupported_broken_and_missing_themes_do_not_prevent_other_themes_loading() {
    let root = tempfile::tempdir().unwrap();
    theme(root.path(), "good");
    let bad = theme(root.path(), "future");
    fs::write(
        bad.join("theme.json"),
        r#"{"format":"maploom/theme","api_version":99,"name":"future","stylesheet":"style.css"}"#,
    )
    .unwrap();
    fs::create_dir(root.path().join("broken")).unwrap();
    let catalog = themes::scan(root.path());
    assert_eq!(catalog.themes.len(), 1);
    assert_eq!(catalog.warnings.len(), 2);
    assert!(themes::package(root.path(), "missing").is_err());
}

#[test]
fn theme_types_are_validated_and_legacy_themes_remain_adaptive() {
    let root = tempfile::tempdir().unwrap();
    let folder = theme(root.path(), "custom");
    let original = fs::read_to_string(folder.join("theme.json")).unwrap();
    assert!(themes::read_theme(root.path(), "custom")
        .unwrap()
        .manifest
        .mode
        .is_none());
    for (value, expected) in [
        (serde_json::json!("dark"), Some(themes::ThemeMode::Dark)),
        (serde_json::json!("light"), Some(themes::ThemeMode::Light)),
        (serde_json::Value::Null, None),
    ] {
        let mut manifest: serde_json::Value = serde_json::from_str(&original).unwrap();
        manifest["mode"] = value.clone();
        fs::write(folder.join("theme.json"), manifest.to_string()).unwrap();
        let package = themes::package(root.path(), "custom").unwrap();
        assert_eq!(package.theme.manifest.mode, expected);
        assert_eq!(
            serde_json::to_value(package.theme.manifest).unwrap()["mode"],
            value
        );
    }
    for invalid in [serde_json::json!("sepia"), serde_json::json!(true)] {
        let mut manifest: serde_json::Value = serde_json::from_str(&original).unwrap();
        manifest["mode"] = invalid;
        fs::write(folder.join("theme.json"), manifest.to_string()).unwrap();
        assert!(themes::package(root.path(), "custom").is_err());
    }
}

#[test]
fn variants_share_one_folder_and_resolve_their_own_names_and_modes() {
    let root = tempfile::tempdir().unwrap();
    let id = "Моя тема [夜] ✨";
    let folder = theme(root.path(), id);
    let mut manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(folder.join("theme.json")).unwrap()
    ).unwrap();
    manifest["variants"] = serde_json::json!([
        {"id": "night", "name": "Ночь", "mode": "dark"},
        {"id": "paper", "name": "Бумага", "mode": "light"}
    ]);
    fs::write(folder.join("theme.json"), manifest.to_string()).unwrap();
    let catalog = themes::scan(root.path());
    assert!(catalog.warnings.is_empty());
    assert_eq!(catalog.themes.len(), 2);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    assert!(catalog.themes.iter().all(|theme| theme.id == id));
    assert_eq!(catalog.themes[0].manifest.name, "Ночь");
    assert_eq!(catalog.themes[1].manifest.name, "Бумага");
    let dark = themes::package_variant(root.path(), id, Some("night")).unwrap();
    let light = themes::package_variant(root.path(), id, Some("paper")).unwrap();
    assert_eq!(dark.theme.variant.as_deref(), Some("night"));
    assert_eq!(light.theme.variant.as_deref(), Some("paper"));
    assert_eq!(dark.theme.manifest.mode, Some(themes::ThemeMode::Dark));
    assert_eq!(light.theme.manifest.mode, Some(themes::ThemeMode::Light));
    assert_eq!(dark.modules, light.modules);
    assert_eq!(dark.theme.manifest.stylesheet, light.theme.manifest.stylesheet);
    assert_eq!(themes::package(root.path(), id).unwrap().theme.variant.as_deref(), Some("night"));
    assert!(themes::package_variant(root.path(), id, Some("missing")).is_err());
    let saved = serde_json::json!({"version":1, "theme":id, "theme_variant":"paper", "dark":true});
    let preferences: maploom::preferences::Preferences = serde_json::from_value(saved).unwrap();
    let restored: maploom::preferences::Preferences = serde_json::from_slice(
        &serde_json::to_vec(&preferences).unwrap()
    ).unwrap();
    let selected = themes::read_variant(root.path(), restored.theme.as_deref().unwrap(), restored.theme_variant.as_deref()).unwrap();
    assert_eq!(selected.variant.as_deref(), Some("paper"));
    assert_eq!(selected.manifest.mode, Some(themes::ThemeMode::Light));
    // Removing a variant cannot silently load a different one.
    manifest["variants"].as_array_mut().unwrap().pop();
    fs::write(folder.join("theme.json"), manifest.to_string()).unwrap();
    assert!(themes::read_variant(root.path(), id, Some("paper")).is_err());
}

#[test]
fn malformed_variants_are_rejected_without_affecting_other_packages() {
    let root = tempfile::tempdir().unwrap();
    theme(root.path(), "good");
    let folder = theme(root.path(), "bad");
    let mut manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(folder.join("theme.json")).unwrap()
    ).unwrap();
    for variants in [
        serde_json::json!([{"id":"same", "name":"One", "mode":"dark"}, {"id":"same", "name":"Two", "mode":"light"}]),
        serde_json::json!([{"id":"", "name":"One", "mode":"dark"}]),
        serde_json::json!([{"id":"one", "name":" ", "mode":"light"}]),
        serde_json::json!([{"id":"one", "name":"One", "mode":"sepia"}]),
        serde_json::json!([{"id":"one", "name":"One"}]),
    ] {
        manifest["variants"] = variants;
        fs::write(folder.join("theme.json"), manifest.to_string()).unwrap();
        let catalog = themes::scan(root.path());
        assert_eq!(catalog.themes.len(), 1);
        assert_eq!(catalog.themes[0].id, "good");
        assert_eq!(catalog.warnings.len(), 1);
    }
    assert!(themes::package_variant(root.path(), "good", Some("unknown")).is_err());
}

#[test]
fn paths_cannot_read_a_different_theme_or_application_file() {
    let root = tempfile::tempdir().unwrap();
    let folder = theme(root.path(), "one");
    theme(root.path(), "two");
    for id in ["", ".", "..", "../one", "one/two", "C:\\one", "/one"] {
        assert!(themes::package(root.path(), id).is_err(), "{id}");
    }
    for path in [
        "../two/style.css",
        "nested/../../two/style.css",
        "/style.css",
        "C:/style.css",
        "nested\\main.tsx",
        "./style.css",
        "nested//main.tsx",
    ] {
        assert!(themes::resolve(&folder, path).is_err(), "{path}");
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.path().join("two/style.css"), folder.join("escape.css"))
            .unwrap();
        assert!(themes::resolve(&folder, "escape.css").is_err());
        std::os::unix::fs::symlink(root.path().parent().unwrap(), root.path().join("outside"))
            .unwrap();
        assert!(themes::theme_root(root.path(), "outside").is_err());
    }
}

#[test]
fn oversized_sources_are_rejected_and_example_creation_never_replaces_user_files() {
    let root = tempfile::tempdir().unwrap();
    let first = themes::create_example(root.path()).unwrap();
    assert_eq!(first, ["Monokai", "GitHub", "Atlas", "Contour"]);
    for id in &first {
        let package = themes::package(root.path(), id).unwrap();
        assert_eq!(package.theme.manifest.name, if id == "Atlas" { "Atlas Dark" } else { id });
        assert_eq!(
            package.theme.manifest.mode,
            match id.as_str() {
                "Monokai" | "Atlas" => Some(themes::ThemeMode::Dark),
                "GitHub" | "Contour" => Some(themes::ThemeMode::Light),
                _ => None,
            }
        );
        if id == "Atlas" {
            assert_eq!(package.theme.manifest.entry.as_deref(), Some("theme.tsx"));
            assert!(package.modules.contains_key("theme.tsx"));
        } else {
            assert!(package.theme.manifest.entry.is_none());
        }
        assert!(!themes::preview_stylesheet(root.path(), id)
            .unwrap()
            .is_empty());
        fs::write(root.path().join(id).join("style.css"), b"my edits").unwrap();
    }
    let second = themes::create_example(root.path()).unwrap();
    assert_eq!(
        second,
        ["Monokai 2", "GitHub 2", "Atlas 2", "Contour 2"]
    );
    for id in &first {
        assert_eq!(
            fs::read(root.path().join(id).join("style.css")).unwrap(),
            b"my edits"
        );
    }
    assert_eq!(themes::scan(root.path()).themes.len(), 10);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 8, "Five cards use four folders per example set");
    let folder = theme(root.path(), "oversized");
    fs::write(
        folder.join("nested/main.tsx"),
        vec![b'x'; 2 * 1024 * 1024 + 1],
    )
    .unwrap();
    assert!(themes::package(root.path(), "oversized").is_err());
}

#[test]
fn themes_are_next_to_the_application_and_not_in_a_replaceable_bundle() {
    assert_eq!(
        themes::installed_directory(std::path::Path::new("/usr/bin/maploom")).unwrap(),
        std::path::PathBuf::from("/usr/lib/Maploom/themes")
    );
    assert_eq!(
        themes::beside_application(std::path::Path::new("portable/Maploom.exe")).unwrap(),
        std::path::PathBuf::from("portable/themes")
    );
    assert_eq!(
        themes::beside_application(std::path::Path::new(
            "Applications/Maploom.app/Contents/MacOS/maploom"
        ))
        .unwrap(),
        std::path::PathBuf::from("Applications/themes")
    );
    let old: maploom::preferences::Preferences =
        serde_json::from_str(r#"{"version":1,"dark":false,"locale":"ru"}"#).unwrap();
    assert!(old.theme.is_none());
    assert!(old.theme_variant.is_none());
    assert!(!old.dark);
}
