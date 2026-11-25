use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=RUSTC_VERSION");
    println!("cargo:rerun-if-env-changed=RUSTC");

    let version = std::env::var("RUSTC_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
            let output = Command::new(rustc).arg("--version").output().ok()?;
            if !output.status.success() {
                return None;
            }
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        })
        .unwrap_or_else(|| "rustc version unavailable at build time".to_string());

    println!("cargo:rustc-env=RUSTC_VERSION={version}");
}
