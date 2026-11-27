use std::path::{Path, PathBuf};
use std::process::Command;

fn find_rc_exe() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("RC") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    if let Ok(entries) = std::fs::read_dir(kits) {
        let mut versions: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path().join("x64").join("rc.exe"))
            .filter(|p| p.is_file())
            .collect();
        versions.sort();
        if let Some(latest) = versions.pop() {
            return Some(latest);
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=studio_icon.rc");
    println!("cargo:rerun-if-changed=../../studio/studio.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Some(rc) = find_rc_exe() else {
        println!("cargo:warning=rc.exe not found; skipping studio_icon.rc");
        return;
    };
    let Ok(out_dir) = std::env::var("OUT_DIR") else {
        return;
    };
    let res_path = PathBuf::from(out_dir).join("literofi_studio.res");
    match Command::new(rc)
        .args(["/nologo", "/fo"])
        .arg(&res_path)
        .arg("studio_icon.rc")
        .status()
    {
        Ok(status) if status.success() => {
            println!("cargo:rustc-link-arg-bins={}", res_path.display());
        }
        Ok(status) => {
            println!("cargo:warning=rc.exe exited with {status}");
        }
        Err(e) => {
            println!("cargo:warning=failed to run rc.exe: {e}");
        }
    }
}
