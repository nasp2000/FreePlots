// SPDX-License-Identifier: MIT
//! Build script: embeds the application icon in the Windows executable.
//!
//! The icon is a resource, so it only applies when the *target* is Windows
//! (a build script runs on the host, hence `CARGO_CFG_TARGET_OS` rather than
//! `cfg(windows)`).

fn main() {
    println!("cargo:rerun-if-changed=assets/freeplots.ico");
    println!("cargo:rerun-if-changed=assets/freeplots.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/freeplots.ico");
    resource.set_manifest_file("assets/freeplots.manifest");
    resource.set("FileDescription", "FreePlots");
    resource.set("ProductName", "FreePlots");
    resource.set("CompanyName", "FreePlots");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_owned());
    let numeric = format!("{version}.0");
    resource.set("FileVersion", &numeric);
    resource.set("ProductVersion", &numeric);
    resource.set("OriginalFilename", "FreePlots.exe");
    resource.set("LegalCopyright", "MIT");
    if let Err(error) = resource.compile() {
        panic!("failed to embed the Windows icon: {error}");
    }
}
