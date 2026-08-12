use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=src/win32_do_not_disturb_bridge.cpp");
    println!("cargo:rerun-if-changed=src/win32_brightness_bridge.cpp");
    println!("cargo:rerun-if-changed=src/win32_tray_bridge_hook.cpp");
    println!("cargo:rerun-if-changed=src/win32_tray_bridge_host.cpp");
    if !std::env::var("TARGET").is_ok_and(|target| target.contains("windows")) {
        return Ok(());
    }
    cc::Build::new()
        .cpp(true)
        .file("src/win32_do_not_disturb_bridge.cpp")
        .file("src/win32_brightness_bridge.cpp")
        .flag_if_supported("/std:c++17")
        .compile("minhaui_windows_bridges");
    build_tray_bridge_hook()?;
    println!("cargo:rustc-link-lib=runtimeobject");
    println!("cargo:rustc-link-lib=wbemuuid");
    println!("cargo:rustc-link-lib=oleaut32");
    Ok(())
}

fn build_tray_bridge_hook() -> Result<(), Box<dyn Error>> {
    let output_directory = PathBuf::from(
        std::env::var_os("OUT_DIR")
            .ok_or("Cargo did not provide the required OUT_DIR to the tray bridge build script")?,
    );
    let dll = output_directory.join("obsidian_tray_bridge.dll");
    let object = output_directory.join("obsidian_tray_bridge_hook.obj");
    let import_library = output_directory.join("obsidian_tray_bridge.lib");
    let host = output_directory.join("obsidian_tray_bridge_host.exe");
    let host_object = output_directory.join("obsidian_tray_bridge_host.obj");
    let compiler = cc::Build::new().cpp(true).get_compiler();
    let mut command = compiler.to_command();
    command
        .arg("/nologo")
        .arg("/LD")
        .arg("/O2")
        .arg("/std:c++17")
        .arg("/EHsc-")
        .arg("/GR-")
        .arg(format!("/Fo{}", object.display()))
        .arg("src/win32_tray_bridge_hook.cpp")
        .arg("/link")
        .arg("/NOLOGO")
        .arg(format!("/OUT:{}", dll.display()))
        .arg(format!("/IMPLIB:{}", import_library.display()))
        .arg("user32.lib")
        .arg("kernel32.lib");
    let status = command.status()?;
    if !status.success() {
        return Err(format!("tray bridge DLL compilation failed with status {status}").into());
    }
    let mut host_command = compiler.to_command();
    host_command
        .arg("/nologo")
        .arg("/O2")
        .arg("/std:c++17")
        .arg("/EHsc-")
        .arg("/GR-")
        .arg(format!("/Fo{}", host_object.display()))
        .arg("src/win32_tray_bridge_host.cpp")
        .arg("/link")
        .arg("/NOLOGO")
        .arg("/SUBSYSTEM:WINDOWS")
        .arg(format!("/OUT:{}", host.display()))
        .arg("user32.lib")
        .arg("kernel32.lib");
    let host_status = host_command.status()?;
    if !host_status.success() {
        return Err(
            format!("tray bridge host compilation failed with status {host_status}").into(),
        );
    }
    Ok(())
}
