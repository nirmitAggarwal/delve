//! Compile the bundled icon theme into a GResource.
//!
//! Delve deliberately ships its complete UI icon theme in the binary. This
//! keeps the app self-contained on minimal Linux systems and makes release
//! builds reproducible and fully offline. `glib-compile-resources` is part of
//! the GLib development tools already required to build a GTK application.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/icons");
    println!("cargo:rerun-if-changed=assets/delve.svg");
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let icons_dir = manifest_dir.join("assets/icons");

    let mut files = Vec::new();
    collect_files(&icons_dir, &icons_dir, &mut files);
    files.sort();

    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <gresources>\n\
         \x20 <gresource prefix=\"/io/github/delve/FileManager\">\n",
    );
    for relative in &files {
        let source = format!("icons/{}", relative.display());
        xml.push_str("    <file alias=\"");
        xml.push_str(&escape_xml(&source));
        xml.push_str("\">");
        xml.push_str(&escape_xml(&source));
        xml.push_str("</file>\n");
    }
    xml.push_str("  </gresource>\n</gresources>\n");

    let manifest = out_dir.join("delve.gresource.xml");
    fs::write(&manifest, xml).expect("write GResource manifest");
    let target = out_dir.join("delve.gresource");

    let output = Command::new("glib-compile-resources")
        .arg("--sourcedir")
        .arg(&manifest_dir.join("assets"))
        .arg("--target")
        .arg(&target)
        .arg(&manifest)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "could not run glib-compile-resources: {error}\n\
                 Install the GLib development tools (on Debian/Ubuntu: \
                 libglib2.0-dev-bin) and try again."
            )
        });

    if !output.status.success() {
        panic!(
            "glib-compile-resources failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        panic!("could not read asset directory {}", dir.display());
    };
    for entry in entries {
        let path = entry.expect("read asset entry").path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else if path.is_file() {
            let relative = path.strip_prefix(root).expect("asset path is under root");
            files.push(relative.to_path_buf());
        }
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
