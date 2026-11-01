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
    resource.set("FileVersion", "0.1.0.0");
    resource.set("ProductVersion", "0.1.0.0");
    resource.set("OriginalFilename", "FreePlots.exe");
    resource.set("LegalCopyright", "MIT");
    if let Err(error) = resource.compile() {
        panic!("failed to embed the Windows icon: {error}");
    }
}
