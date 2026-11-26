fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-changed=macos/open_files.m");
        cc::Build::new()
            .file("macos/open_files.m")
            .flag("-fobjc-arc")
            .compile("imageoptim_open");
        println!("cargo:rustc-link-lib=framework=Cocoa");
        println!("cargo:rustc-link-lib=framework=Quartz");
    }
}
