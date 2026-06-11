fn main() {
    // Embed Windows version metadata so the exe is not an anonymous binary
    // (unsigned exes without VERSIONINFO score worse with AV heuristics).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set("ProductName", "ssh4");
        res.set("FileDescription", "ssh4 SSH client (GUI and CLI)");
        res.set("CompanyName", "Keith Lam");
        res.set(
            "LegalCopyright",
            "Copyright (c) 2026 Keith Lam. MIT License.",
        );
        res.set("OriginalFilename", "ssh4.exe");
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed Windows resources: {e}");
        }
    }
}
