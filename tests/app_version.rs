#[test]
fn cli_help_uses_the_package_version() {
    let package: serde_json::Value = serde_json::from_str(include_str!("../package.json")).unwrap();
    let version = package["version"].as_str().unwrap();
    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        version,
        "Run npm run sync:version before direct Cargo commands"
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_maploom"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with(&format!("Maploom {version} —"))
    );
}
