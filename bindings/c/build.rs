//! Name the shared library by its file name alone, so programs find it
//! through their rpath or the loader's search path wherever it lands.
// cargo reads a build script's instructions from its stdout
#![allow(clippy::print_stdout)]

fn main() {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => {
            println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libkatui.dylib");
        }
        Ok("linux") => println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libkatui.so"),
        _ => {}
    }
}
