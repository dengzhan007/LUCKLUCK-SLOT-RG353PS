//! Background MTP gadget bring-up for BaseOS.
//!
//! BaseOS owns the init scripts and the ADB gadget.  The frontend only starts the
//! card-local helper so MTP can be added without changing the OS image or showing
//! another menu.  The helper is intentionally detached: a missing MTP function or
//! responder is a logged, non-fatal condition and never prevents the frontend from
//! starting.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Start the card-local MTP monitor once for this boot.
pub fn start(root: &Path) {
    let script = root.join("System/mtp-auto.sh");
    if !script.is_file() {
        return;
    }

    let root = root.to_path_buf();
    let _ = std::thread::Builder::new()
        .name("mtp-auto".to_string())
        .spawn(move || run(script, root));
}

fn run(script: PathBuf, root: PathBuf) {
    let _ = Command::new("/bin/sh")
        .arg(script)
        .env("SLOT_ROOT", root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
