// Embeds icon.ico (project root) into hesewake-notch.exe and sets the name shown in Task Manager / Explorer.
fn main() {
    println!("cargo:rerun-if-changed=icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icon.ico");
        res.set("ProductName", "Hesewake Notch");
        res.set("FileDescription", "Hesewake Notch");
        res.set("InternalName", "hesewake-notch");
        res.set("OriginalFilename", "hesewake-notch.exe");
        res.set("LegalCopyright", "Based on Poch by Commonly Works (MIT License)");
        if let Err(e) = res.compile() {
            // Do not break the build; the app still runs, only the exe icon would be missing.
            println!("cargo:warning=could not embed icon.ico into the exe: {e}");
        }
    }
}
