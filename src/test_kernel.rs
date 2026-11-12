use std::{path::PathBuf, process::{Command, Stdio}};

/// Builds the test kernel and returns the path of its ELF file.
///
/// The test kernel is the kernel crate built by `cargo test`: it contains
/// the `ktest!` cases and the harness in `kernel/src/ktest`. Cargo cannot
/// build it as an artifact dependency, so this function calls cargo.
///
/// Cargo does not use a `runner` from `.cargo/config.toml` for a package
/// with `forced-target`, so `cargo test -p azpos-kernel` cannot start the
/// tests itself.
pub fn build() -> Result<PathBuf, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let output = Command::new(cargo)
        .args(["test", "--package", "azpos-kernel", "--bin", "azpos-kernel", "--no-run"])
        // Cargo prints the compiler messages as text on stderr and one JSON
        // object for each artifact on stdout
        .arg("--message-format=json-render-diagnostics")
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("Cannot start cargo: {e}"))?;

    if !output.status.success() {
        return Err("The test kernel does not build".to_string());
    }

    // The test kernel is the artifact of the kernel crate that has an
    // executable and the test profile
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter(|message| message["target"]["name"] == "azpos-kernel")
        .filter(|message| message["profile"]["test"] == true)
        .find_map(|message| message["executable"].as_str().map(PathBuf::from))
        .ok_or_else(|| "Cargo did not report the test kernel executable".to_string())
}
