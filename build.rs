use std::env;

fn is_gnu_windows_toolchain() -> bool {
    match env::var("TARGET").unwrap().as_str() {
        "x86_64-pc-windows-gnu" | "i686-pc-windows-gnu" => true,
        _ => false,
    }
}

fn main() {
    // if is_gnu_windows_toolchain() {
    //     println!("cargo:rustc-link-arg=ftd2xx.lib");
    // } else {
    //     println!("cargo:rustc-link-arg=ftd2xx");
    // }

    // println!("cargo:rustc-link-arg=ftd2xx.lib");
}
