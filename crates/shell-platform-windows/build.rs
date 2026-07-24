fn main() {
    println!("cargo:rerun-if-changed=src/win32_do_not_disturb_bridge.cpp");
    println!("cargo:rerun-if-changed=src/win32_brightness_bridge.cpp");
    if !std::env::var("TARGET").is_ok_and(|target| target.contains("windows")) {
        return;
    }
    cc::Build::new()
        .cpp(true)
        .file("src/win32_do_not_disturb_bridge.cpp")
        .file("src/win32_brightness_bridge.cpp")
        .flag_if_supported("/std:c++17")
        .compile("minhaui_windows_bridges");
    println!("cargo:rustc-link-lib=runtimeobject");
    println!("cargo:rustc-link-lib=wbemuuid");
    println!("cargo:rustc-link-lib=oleaut32");
}
