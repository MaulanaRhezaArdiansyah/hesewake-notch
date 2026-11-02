// Puts the notch's icon, a name for Task Manager and a manifest into hesewake-odoo.exe. The manifest asks for the
// current look of the setup window's boxes and buttons (Common Controls 6) and for sharp text on high-dpi screens.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../icon.ico");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../icon.ico");
        res.set("ProductName", "Hesewake Odoo");
        res.set("FileDescription", "Odoo for Hesewake Notch");
        res.set("InternalName", "hesewake-odoo");
        res.set("OriginalFilename", "hesewake-odoo.exe");
        res.set_manifest(MANIFEST);
        if let Err(e) = res.compile() {
            // the extension still works without it, only plainer and blurry on a scaled screen
            println!("cargo:warning=could not embed the resources: {e}");
        }
    }
}

#[cfg(windows)]
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#;
