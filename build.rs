fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    if !std::path::Path::new("assets/icon.ico").exists() {
        println!("cargo:warning=assets/icon.ico missing; skipping Windows resources");
        return;
    }

    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    res.set("ProductName", "LocalRecord");
    // Shown as the session name in the Windows volume mixer, so keep it short
    // and recognizable rather than a full description of the app.
    res.set("FileDescription", "LocalRecord");
    res.set("CompanyName", "LocalRecord");
    res.set("LegalCopyright", "Copyright (C) LocalRecord");
    res.set("OriginalFilename", "localrecord.exe");
    res.set("InternalName", "localrecord");

    if std::path::Path::new("assets/app.manifest").exists() {
        res.set_manifest_file("assets/app.manifest");
    }

    if let Err(err) = res.compile() {
        println!("cargo:warning=Could not compile Windows resources: {err}");
        println!("cargo:warning=On Windows, install the MSVC build tools or Windows SDK so windres/rc.exe is available.");
    }
}
