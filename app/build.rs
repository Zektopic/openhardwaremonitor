fn main() {
    // Embed the app icon + version info into the Windows executable, and — for
    // release builds only — a requireAdministrator manifest so the app elevates
    // at launch like HWiNFO does (full sensor access needs admin: Super-I/O,
    // MSR, SMBus via the kernel driver). Debug builds stay asInvoker so
    // `cargo test` / dev runs don't trip UAC.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if std::env::var("PROFILE").as_deref() == Ok("release") {
            res.set_manifest(
                r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>"#,
            );
        }
        res.compile().expect("failed to embed Windows resources");
    }
}
