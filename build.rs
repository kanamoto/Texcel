#[path = "tools/windows_resource.rs"]
mod windows_resource;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Adds the ICO resource only when building a Windows executable.
fn main() {
    println!("cargo:rerun-if-changed=assets/icons/app_icon.ico");
    println!("cargo:rerun-if-changed=tools/windows_resource.rs");
    println!("cargo:rerun-if-env-changed=RC");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        compile_windows_icon();
    }
}

/// Compiles the generated ICO into the Texcel executable resource table.
fn compile_windows_icon() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let msvc = target_env == "msvc";
    let icon = manifest.join("assets/icons/app_icon.ico");
    if !icon.is_file() {
        panic!("missing assets/icons/app_icon.ico; run the documented icon converter first");
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let resource_icon = out_dir.join(windows_resource::ICON_RESOURCE_NAME);
    let resource_file = out_dir.join("texcel-icon.rc");
    let output_file = out_dir.join(if msvc {
        "texcel-icon.res"
    } else {
        "texcel-icon.o"
    });
    fs::copy(&icon, &resource_icon).expect("copy icon beside Windows resource script");
    fs::write(&resource_file, windows_resource::icon_resource_line())
        .expect("write Windows resource script");

    let default_compiler = if msvc { "rc.exe" } else { "windres" };
    let compiler = env::var_os("RC").unwrap_or_else(|| default_compiler.into());
    let mut command = Command::new(compiler);
    command.current_dir(&out_dir);
    if msvc {
        command.args(["/nologo", "/fo", "texcel-icon.res", "texcel-icon.rc"]);
    } else {
        command.args([
            "-i",
            "texcel-icon.rc",
            "-o",
            "texcel-icon.o",
            "--output-format=coff",
        ]);
    }
    let status = command.status().expect("run the Windows resource compiler");
    if !status.success() {
        panic!("Windows resource compiler failed with {}", status);
    }
    println!("cargo:rustc-link-arg-bin=texcel={}", output_file.display());
}

#[cfg(test)]
#[path = "tools/windows_resource_test.rs"]
mod tests;
