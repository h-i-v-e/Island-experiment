fn main() {
    // Do not embed the build machine's checkout path as the macOS library ID.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg-cdylib=-Wl,-install_name,@rpath/libmotu.dylib");
    }
}
