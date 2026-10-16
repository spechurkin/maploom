fn main() {
    #[cfg(feature = "desktop")]
    tauri_build::build();
    println!("cargo:rerun-if-changed=assets/branding/maploom.rc");
    println!("cargo:rerun-if-changed=assets/branding/maploom.ico");
    if !cfg!(feature = "desktop")
        && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
    {
        embed_resource::compile_for(
            "assets/branding/maploom.rc",
            ["maploom"],
            embed_resource::NONE,
        )
            .manifest_required()
            .expect("Could not embed the Maploom Windows icon");
    }
}
