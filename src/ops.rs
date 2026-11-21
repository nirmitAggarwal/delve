//! File operations: copy, move, delete, trash, archive creation and extraction.
//!
//! Everything here is UI-free and runs on worker threads. Callers receive
//! progress through a callback and a structured outcome, including per-item
//! failures, so the UI can report exactly what went wrong.

use crate::fs;
use crate::settings::ConflictPolicy;
use std::fs::{self as stdfs, File};
use std::os::unix::fs::MetadataExt;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Progress {
    pub fraction: f64,
    pub message: String,
    pub done: u64,
    pub total: u64,
}

pub type ProgressFn = Arc<dyn Fn(Progress) + Send + Sync>;

#[derive(Clone, Debug)]
pub struct Failure {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Default, Debug)]
pub struct Outcome {
    pub succeeded: usize,
    pub skipped: usize,
    pub bytes: u64,
    pub failures: Vec<Failure>,
    pub cancelled: bool,
    pub message: String,
    /// (source, final destination) for each top-level item that was written.
    pub pairs: Vec<(PathBuf, PathBuf)>,
}

impl Outcome {
    pub fn ok(&self) -> bool {
        self.failures.is_empty() && !self.cancelled
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
}

impl ArchiveFormat {
    pub fn from_path(path: &Path) -> Option<ArchiveFormat> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        if name.ends_with(".zip") {
            Some(ArchiveFormat::Zip)
        } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            Some(ArchiveFormat::TarGz)
        } else if name.ends_with(".tar") {
            Some(ArchiveFormat::Tar)
        } else {
            None
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ArchiveFormat::Zip => ".zip",
            ArchiveFormat::Tar => ".tar",
            ArchiveFormat::TarGz => ".tar.gz",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ArchiveFormat::Zip => "ZIP",
            ArchiveFormat::Tar => "TAR",
            ArchiveFormat::TarGz => "TAR.GZ",
        }
    }
}

/// Effective conflict policy; nested conflicts never prompt individually.
#[derive(Clone, Copy)]
struct Policy(ConflictPolicy);

impl Policy {
    fn decide(&self, dest: &Path) -> Decision {
        if !dest.exists() && !dest.is_symlink() {
            return Decision::Write(dest.to_path_buf());
        }
        match self.0 {
            ConflictPolicy::Replace => Decision::Write(dest.to_path_buf()),
            ConflictPolicy::Skip => Decision::Skip,
            ConflictPolicy::KeepBoth | ConflictPolicy::Ask => {
                let parent = dest.parent().unwrap_or(Path::new("/"));
                let name = dest
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "item".into());
                Decision::Write(fs::unique_path(parent, &name))
            }
        }
    }
}

#[derive(Debug)]
enum Decision {
    Write(PathBuf),
    Skip,
}

pub fn count_tree_bytes(paths: &[PathBuf]) -> (u64, u64) {
    let mut bytes = 0u64;
    let mut items = 0u64;
    for path in paths {
        let meta = stdfs::symlink_metadata(path);
        match meta {
            Ok(m) if m.is_dir() => {
                items += 1;
                for (child, _) in fs::collect_tree(path, 250_000) {
                    items += 1;
                    if let Ok(cm) = stdfs::symlink_metadata(&child) {
                        if cm.is_file() {
                            bytes += cm.len();
                        }
                    }
                }
            }
            Ok(m) => {
                items += 1;
                bytes += m.len();
            }
            Err(_) => items += 1,
        }
    }
    (bytes, items)
}

fn copy_file(src: &Path, dest: &Path) -> io::Result<u64> {
    if let Some(parent) = dest.parent() {
        stdfs::create_dir_all(parent)?;
    }
    let mut input = File::open(src)?;
    let mut output = File::create(dest)?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut total = 0u64;
    loop {
        let read = input.read(&mut buf)?;
        if read == 0 {
            break;
        }
        output.write_all(&buf[..read])?;
        total += read as u64;
    }
    output.flush()?;
    if let Ok(meta) = stdfs::metadata(src) {
        let _ = stdfs::set_permissions(dest, meta.permissions());
    }
    Ok(total)
}

fn copy_entry(
    src: &Path,
    dest: &Path,
    policy: Policy,
    cancel: &AtomicBool,
    report: &dyn Fn(&Path),
    outcome: &mut Outcome,
) {
    if cancel.load(Ordering::Relaxed) {
        outcome.cancelled = true;
        return;
    }
    let dest = match policy.decide(dest) {
        Decision::Write(p) => p,
        Decision::Skip => {
            outcome.skipped += 1;
            report(src);
            return;
        }
    };
    copy_entry_at(src, &dest, policy, cancel, report, outcome);
}

/// Copy into an already-decided destination. The top level calls this directly
/// so it can record exactly where an item landed.
fn copy_entry_at(
    src: &Path,
    dest: &Path,
    policy: Policy,
    cancel: &AtomicBool,
    report: &dyn Fn(&Path),
    outcome: &mut Outcome,
) {
    let meta = match stdfs::symlink_metadata(src) {
        Ok(m) => m,
        Err(e) => {
            outcome.failures.push(Failure {
                path: src.to_path_buf(),
                message: fs::describe_io_error(&e, src),
            });
            report(src);
            return;
        }
    };
    if meta.is_dir() {
        if let Err(e) = stdfs::create_dir_all(dest) {
            outcome.failures.push(Failure {
                path: src.to_path_buf(),
                message: fs::describe_io_error(&e, src),
            });
            report(src);
            return;
        }
        let _ = stdfs::set_permissions(dest, meta.permissions());
        let rd = match stdfs::read_dir(src) {
            Ok(rd) => rd,
            Err(e) => {
                outcome.failures.push(Failure {
                    path: src.to_path_buf(),
                    message: fs::describe_io_error(&e, src),
                });
                report(src);
                return;
            }
        };
        let mut children: Vec<PathBuf> = rd.flatten().map(|d| d.path()).collect();
        children.sort();
        for child in children {
            report(&child);
            let name = child.file_name().map(|n| n.to_os_string()).unwrap_or_default();
            copy_entry(&child, &dest.join(name), policy, cancel, report, outcome);
        }
        outcome.succeeded += 1;
    } else if meta.file_type().is_symlink() {
        let target = match stdfs::read_link(src) {
            Ok(t) => t,
            Err(e) => {
                outcome.failures.push(Failure {
                    path: src.to_path_buf(),
                    message: format!("Could not read link: {e}"),
                });
                report(src);
                return;
            }
        };
        let _ = stdfs::remove_file(dest);
        match std::os::unix::fs::symlink(&target, dest) {
            Ok(()) => {
                outcome.succeeded += 1;
                outcome.bytes += 1;
            }
            Err(e) => outcome.failures.push(Failure {
                path: src.to_path_buf(),
                message: format!("Could not create link: {e}"),
            }),
        }
        report(src);
    } else {
        match copy_file(src, dest) {
            Ok(bytes) => {
                outcome.succeeded += 1;
                outcome.bytes += bytes;
            }
            Err(e) => outcome.failures.push(Failure {
                path: src.to_path_buf(),
                message: fs::describe_io_error(&e, src),
            }),
        }
        report(src);
    }
}

/// Copy or move a set of paths into `dest_dir`.
pub fn transfer(
    sources: &[PathBuf],
    dest_dir: &Path,
    move_items: bool,
    policy: ConflictPolicy,
    cancel: Arc<AtomicBool>,
    progress: Option<ProgressFn>,
) -> Outcome {
    let mut outcome = Outcome::default();
    let (total_bytes, total_items) = count_tree_bytes(sources);
    let done = AtomicU64::new(0);
    let bytes_done = AtomicU64::new(0);
    let report = |path: &Path| {
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(cb) = &progress {
            if let Ok(meta) = stdfs::symlink_metadata(path) {
                if meta.is_file() {
                    bytes_done.fetch_add(meta.len(), Ordering::Relaxed);
                }
            }
            let fraction = if total_items > 0 {
                (n as f64 / total_items as f64).min(1.0)
            } else {
                0.0
            };
            cb(Progress {
                fraction,
                message: if move_items { "Moving" } else { "Copying" }.into(),
                done: n,
                total: total_items,
            });
        }
    };
    let policy = Policy(policy);
    for src in sources {
        if cancel.load(Ordering::Relaxed) {
            outcome.cancelled = true;
            break;
        }
        let Some(name) = src.file_name() else {
            outcome.failures.push(Failure {
                path: src.clone(),
                message: "The path has no file name and cannot be copied.".into(),
            });
            continue;
        };
        if src.is_dir() && fs::is_descendant(src, dest_dir) {
            outcome.failures.push(Failure {
                path: src.clone(),
                message: "A folder cannot be copied into itself.".into(),
            });
            continue;
        }
        let target = dest_dir.join(name);
        if move_items {
            let same_device = same_device(src, dest_dir);
            let exists = target.exists();
            if same_device && !exists {
                match stdfs::rename(src, &target) {
                    Ok(()) => {
                        outcome.succeeded += 1;
                        outcome.bytes += total_bytes;
                        outcome.pairs.push((src.clone(), target.clone()));
                        report(src);
                        continue;
                    }
                    Err(_) => {
                        // Fall through to copy+delete (e.g. permission issues on
                        // a bind mount).
                    }
                }
            }
        }
        let decided = match policy.decide(&target) {
            Decision::Write(p) => p,
            Decision::Skip => {
                outcome.skipped += 1;
                report(src);
                continue;
            }
        };
        let before = outcome.succeeded;
        copy_entry_at(src, &decided, policy, &cancel, &report, &mut outcome);
        outcome.pairs.push((src.clone(), decided.clone()));
        if move_items && outcome.succeeded > before {
            match remove_path(src) {
                Ok(()) => {}
                Err(e) => outcome.failures.push(Failure {
                    path: src.clone(),
                    message: format!("Copied, but the original could not be removed: {e}"),
                }),
            }
        }
    }
    if let Some(cb) = &progress {
        cb(Progress {
            fraction: 1.0,
            message: if outcome.cancelled { "Cancelled" } else { "Done" }.into(),
            done: total_items,
            total: total_items,
        });
    }
    outcome.message = format!(
        "{} item{}{}",
        outcome.succeeded,
        if outcome.succeeded == 1 { "" } else { "s" },
        if outcome.bytes > 0 {
            format!(" · {}", fs::human_size(outcome.bytes))
        } else {
            String::new()
        }
    );
    let _ = total_bytes;
    outcome
}

fn same_device(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (stdfs::metadata(a), stdfs::metadata(b)) {
        (Ok(x), Ok(y)) => x.dev() == y.dev(),
        _ => false,
    }
}

fn remove_path(path: &Path) -> io::Result<()> {
    let meta = stdfs::symlink_metadata(path)?;
    if meta.is_dir() {
        stdfs::remove_dir_all(path)
    } else {
        stdfs::remove_file(path)
    }
}

/// Permanently delete paths. `keep_root` false removes the top level too.
pub fn delete_permanent(
    paths: &[PathBuf],
    cancel: Arc<AtomicBool>,
    progress: Option<ProgressFn>,
) -> Outcome {
    let mut outcome = Outcome::default();
    let total = paths.len() as u64;
    let mut seen_failure = false;
    for (i, path) in paths.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            outcome.cancelled = true;
            break;
        }
        match remove_path(path) {
            Ok(()) => outcome.succeeded += 1,
            Err(e) => {
                seen_failure = true;
                outcome.failures.push(Failure {
                    path: path.clone(),
                    message: fs::describe_io_error(&e, path),
                });
            }
        }
        if let Some(cb) = &progress {
            cb(Progress {
                fraction: (i + 1) as f64 / total.max(1) as f64,
                message: "Deleting".into(),
                done: (i + 1) as u64,
                total,
            });
        }
    }
    let _ = seen_failure;
    outcome.message = format!("Deleted {} item(s)", outcome.succeeded);
    outcome
}

pub fn trash_paths(paths: &[PathBuf]) -> Outcome {
    let mut outcome = Outcome::default();
    match trash::delete_all(paths) {
        Ok(()) => outcome.succeeded = paths.len(),
        Err(e) => {
            outcome.failures.push(Failure {
                path: paths.first().cloned().unwrap_or_default(),
                message: format!("Could not move to Trash: {e}"),
            });
        }
    }
    outcome.message = format!("Moved {} item(s) to Trash", outcome.succeeded);
    outcome
}

/// Restore items whose original paths are in `paths` from the Trash.
pub fn restore_paths_from_trash(paths: &[PathBuf]) -> Outcome {
    let mut outcome = Outcome::default();
    match trash::os_limited::list() {
        Ok(items) => {
            let targets: Vec<_> = items
                .into_iter()
                .filter(|item| paths.iter().any(|p| item.original_path() == *p))
                .collect();
            if targets.is_empty() {
                outcome.failures.push(Failure {
                    path: paths.first().cloned().unwrap_or_default(),
                    message: "The deleted items are no longer in the Trash.".into(),
                });
            } else {
                let count = targets.len();
                match trash::os_limited::restore_all(targets) {
                    Ok(()) => outcome.succeeded = count,
                    Err(e) => outcome.failures.push(Failure {
                        path: paths.first().cloned().unwrap_or_default(),
                        message: format!("Could not restore from Trash: {e}"),
                    }),
                }
            }
        }
        Err(e) => outcome.failures.push(Failure {
            path: PathBuf::new(),
            message: format!("Could not read Trash: {e}"),
        }),
    }
    outcome.message = format!("Restored {} item(s)", outcome.succeeded);
    outcome
}

pub fn empty_trash() -> Outcome {
    let mut outcome = Outcome::default();
    match trash::os_limited::list() {
        Ok(items) => match trash::os_limited::purge_all(items) {
            Ok(()) => outcome.succeeded = 1,
            Err(e) => outcome.failures.push(Failure {
                path: PathBuf::new(),
                message: format!("Could not empty Trash: {e}"),
            }),
        },
        Err(e) => outcome.failures.push(Failure {
            path: PathBuf::new(),
            message: format!("Could not read Trash: {e}"),
        }),
    }
    outcome.message = "Trash emptied".into();
    outcome
}

pub fn restore_from_trash(names: &[String]) -> Outcome {
    let mut outcome = Outcome::default();
    match trash::os_limited::list() {
        Ok(items) => {
            for item in items {
                let name = item.name.to_string_lossy().to_string();
                if names.iter().any(|n| n == &name) {
                    let path = item.original_path();
                    match trash::os_limited::restore_all([item]) {
                        Ok(()) => outcome.succeeded += 1,
                        Err(e) => outcome.failures.push(Failure {
                            path,
                            message: format!("Could not restore: {e}"),
                        }),
                    }
                }
            }
        }
        Err(e) => outcome.failures.push(Failure {
            path: PathBuf::new(),
            message: format!("Could not read Trash: {e}"),
        }),
    }
    outcome.message = format!("Restored {} item(s)", outcome.succeeded);
    outcome
}

// ---------------------------------------------------------------------------
// Archives
// ---------------------------------------------------------------------------

/// Reject archive entries that would escape the destination or use absolute
/// paths. Defensive by design: extraction never writes outside `dest`.
pub fn safe_entry_path(name: &str) -> Option<PathBuf> {
    let cleaned = name.replace('\\', "/");
    if cleaned.starts_with('/') || cleaned.contains('\0') {
        return None;
    }
    let mut out = PathBuf::new();
    for part in cleaned.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return None;
        }
        if part.contains(':') && out.as_os_str().is_empty() {
            // Reject Windows drive prefixes such as C:
            return None;
        }
        out.push(part);
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

pub fn compress(
    paths: &[PathBuf],
    archive: &Path,
    format: ArchiveFormat,
    cancel: Arc<AtomicBool>,
    progress: Option<ProgressFn>,
) -> Outcome {
    let mut outcome = Outcome::default();
    let (total_bytes, total_items) = count_tree_bytes(paths);
    let processed = AtomicU64::new(0);
    let bytes_written = AtomicU64::new(0);
    let report = |msg: &str| {
        let n = processed.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(cb) = &progress {
            cb(Progress {
                fraction: if total_items > 0 {
                    (n as f64 / total_items as f64).min(1.0)
                } else {
                    0.0
                },
                message: msg.into(),
                done: n,
                total: total_items,
            });
        }
    };

    if let Some(parent) = archive.parent() {
        if let Err(e) = stdfs::create_dir_all(parent) {
            outcome.failures.push(Failure {
                path: archive.to_path_buf(),
                message: fs::describe_io_error(&e, parent),
            });
            return outcome;
        }
    }
    let tmp = archive.with_extension(format!(
        "{}.part",
        archive.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default()
    ));
    let _ = stdfs::remove_file(&tmp);

    let result: io::Result<()> = (|| {
        match format {
            ArchiveFormat::Zip => {
                let file = File::create(&tmp)?;
                let mut writer = zip::ZipWriter::new(file);
                for src in paths {
                    add_zip_path(&mut writer, src, Path::new(""), &cancel, &report, &mut outcome, &total_bytes, &bytes_written)?;
                }
                writer.finish()?;
            }
            ArchiveFormat::Tar | ArchiveFormat::TarGz => {
                let file = File::create(&tmp)?;
                if format == ArchiveFormat::TarGz {
                    let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
                    let mut builder = tar::Builder::new(enc);
                    for src in paths {
                        add_tar_path(&mut builder, src, Path::new(""), &cancel, &report, &mut outcome)?;
                    }
                    builder.finish()?;
                } else {
                    let mut builder = tar::Builder::new(file);
                    for src in paths {
                        add_tar_path(&mut builder, src, Path::new(""), &cancel, &report, &mut outcome)?;
                    }
                    builder.finish()?;
                }
            }
        }
        Ok(())
    })();

    match result {
        Ok(()) => match stdfs::rename(&tmp, archive) {
            Ok(()) => {
                outcome.succeeded = 1;
                outcome.bytes = stdfs::metadata(archive).map(|m| m.len()).unwrap_or(0);
                outcome.message = format!("Created {}", archive.display());
            }
            Err(e) => {
                let _ = stdfs::remove_file(&tmp);
                outcome.failures.push(Failure {
                    path: archive.to_path_buf(),
                    message: format!("Could not write archive: {e}"),
                });
            }
        },
        Err(e) => {
            let _ = stdfs::remove_file(&tmp);
            if cancel.load(Ordering::Relaxed) {
                outcome.cancelled = true;
            } else {
                outcome.failures.push(Failure {
                    path: archive.to_path_buf(),
                    message: format!("Archive creation failed: {e}"),
                });
            }
        }
    }
    outcome
}

fn add_zip_path(
    writer: &mut zip::ZipWriter<File>,
    src: &Path,
    prefix: &Path,
    cancel: &AtomicBool,
    report: &dyn Fn(&str),
    outcome: &mut Outcome,
    total_bytes: &u64,
    bytes_written: &AtomicU64,
) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }
    let meta = match stdfs::symlink_metadata(src) {
        Ok(m) => m,
        Err(e) => {
            outcome.failures.push(Failure {
                path: src.to_path_buf(),
                message: fs::describe_io_error(&e, src),
            });
            return Ok(());
        }
    };
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let rel = if prefix.as_os_str().is_empty() {
        PathBuf::from(&name)
    } else {
        prefix.join(&name)
    };
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    if meta.is_dir() {
        let opts = zip::write::SimpleFileOptions::default()
            .unix_permissions(meta.mode() & 0o777)
            .compression_method(zip::CompressionMethod::Stored);
        writer.add_directory(format!("{rel_str}/"), opts)?;
        let rd = stdfs::read_dir(src)?;
        let mut children: Vec<PathBuf> = rd.flatten().map(|d| d.path()).collect();
        children.sort();
        for child in children {
            add_zip_path(writer, &child, &rel, cancel, report, outcome, total_bytes, bytes_written)?;
        }
        report("Compressing");
    } else {
        let opts = zip::write::SimpleFileOptions::default()
            .unix_permissions(meta.mode() & 0o777)
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file(&rel_str, opts)?;
        let mut f = File::open(src)?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            writer.write_all(&buf[..n])?;
            bytes_written.fetch_add(n as u64, Ordering::Relaxed);
        }
        let _ = total_bytes;
        report("Compressing");
    }
    Ok(())
}

fn add_tar_path(
    builder: &mut tar::Builder<impl Write>,
    src: &Path,
    prefix: &Path,
    cancel: &AtomicBool,
    report: &dyn Fn(&str),
    outcome: &mut Outcome,
) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }
    let meta = match stdfs::symlink_metadata(src) {
        Ok(m) => m,
        Err(e) => {
            outcome.failures.push(Failure {
                path: src.to_path_buf(),
                message: fs::describe_io_error(&e, src),
            });
            return Ok(());
        }
    };
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let rel = if prefix.as_os_str().is_empty() {
        PathBuf::from(&name)
    } else {
        prefix.join(&name)
    };
    if meta.is_dir() {
        builder.append_dir(&rel, src)?;
        let rd = stdfs::read_dir(src)?;
        let mut children: Vec<PathBuf> = rd.flatten().map(|d| d.path()).collect();
        children.sort();
        for child in children {
            add_tar_path(builder, &child, &rel, cancel, report, outcome)?;
        }
    } else if meta.file_type().is_symlink() {
        let mut header = tar::Header::new_gnu();
        header.set_metadata(&meta);
        header.set_entry_type(tar::EntryType::Symlink);
        let target = stdfs::read_link(src)?;
        builder.append_link(&mut header, &rel, &target)?;
    } else {
        let mut f = File::open(src)?;
        builder.append_file(&rel, &mut f)?;
    }
    report("Compressing");
    Ok(())
}

pub fn inspect_archive(path: &Path) -> Result<Vec<(String, u64, bool)>, String> {
    let format = ArchiveFormat::from_path(path)
        .ok_or_else(|| "Unsupported archive format. Delve supports ZIP, TAR and TAR.GZ.".to_string())?;
    let mut out = Vec::new();
    match format {
        ArchiveFormat::Zip => {
            let file = File::open(path).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("Not a valid ZIP: {e}"))?;
            for i in 0..zip.len() {
                let entry = zip.by_index(i).map_err(|e| e.to_string())?;
                out.push((entry.name().to_string(), entry.size(), entry.is_dir()));
            }
        }
        ArchiveFormat::Tar | ArchiveFormat::TarGz => {
            let file = File::open(path).map_err(|e| e.to_string())?;
            let reader: Box<dyn Read> = if format == ArchiveFormat::TarGz {
                Box::new(flate2::read::GzDecoder::new(file))
            } else {
                Box::new(file)
            };
            let mut archive = tar::Archive::new(reader);
            for entry in archive.entries().map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let header = entry.header();
                let name = entry.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                out.push((name, header.size().unwrap_or(0), header.entry_type().is_dir()));
            }
        }
    }
    if out.len() > 5000 {
        out.truncate(5000);
    }
    Ok(out)
}

pub fn extract(
    archive: &Path,
    dest: &Path,
    policy: ConflictPolicy,
    cancel: Arc<AtomicBool>,
    progress: Option<ProgressFn>,
) -> Outcome {
    let mut outcome = Outcome::default();
    let format = match ArchiveFormat::from_path(archive) {
        Some(f) => f,
        None => {
            outcome.failures.push(Failure {
                path: archive.to_path_buf(),
                message: "Unsupported archive format. Delve supports ZIP, TAR and TAR.GZ.".into(),
            });
            return outcome;
        }
    };
    if let Err(e) = stdfs::create_dir_all(dest) {
        outcome.failures.push(Failure {
            path: dest.to_path_buf(),
            message: fs::describe_io_error(&e, dest),
        });
        return outcome;
    }
    let policy = Policy(policy);
    let total = inspect_archive(archive).map(|v| v.len() as u64).unwrap_or(0);
    let mut done = 0u64;
    let mut tick = |msg: &str, outcome: &mut Outcome| {
        done += 1;
        if let Some(cb) = &progress {
            cb(Progress {
                fraction: if total > 0 { (done as f64 / total as f64).min(1.0) } else { 0.0 },
                message: msg.into(),
                done,
                total,
            });
        }
        if cancel.load(Ordering::Relaxed) {
            outcome.cancelled = true;
        }
    };

    let result: Result<(), String> = (|| {
        match format {
            ArchiveFormat::Zip => {
                let file = File::open(archive).map_err(|e| e.to_string())?;
                let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("Invalid ZIP: {e}"))?;
                for i in 0..zip.len() {
                    if outcome.cancelled {
                        break;
                    }
                    let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
                    let raw_name = entry.name().to_string();
                    let Some(rel) = safe_entry_path(&raw_name) else {
                        outcome.failures.push(Failure {
                            path: PathBuf::from(raw_name),
                            message: "Skipped unsafe archive entry (path traversal).".into(),
                        });
                        continue;
                    };
                    let target = dest.join(&rel);
                    if entry.is_dir() {
                        let _ = stdfs::create_dir_all(&target);
                        tick("Extracting", &mut outcome);
                        continue;
                    }
                    let target = match policy.decide(&target) {
                        Decision::Write(p) => p,
                        Decision::Skip => {
                            outcome.skipped += 1;
                            tick("Extracting", &mut outcome);
                            continue;
                        }
                    };
                    if let Some(parent) = target.parent() {
                        let _ = stdfs::create_dir_all(parent);
                    }
                    let mut out = File::create(&target).map_err(|e| format!("{}: {e}", target.display()))?;
                    io::copy(&mut entry, &mut out).map_err(|e| format!("{}: {e}", target.display()))?;
                    if let Some(mode) = entry.unix_mode() {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = stdfs::set_permissions(&target, stdfs::Permissions::from_mode(mode & 0o777));
                    }
                    outcome.succeeded += 1;
                    tick("Extracting", &mut outcome);
                }
            }
            ArchiveFormat::Tar | ArchiveFormat::TarGz => {
                let file = File::open(archive).map_err(|e| e.to_string())?;
                let reader: Box<dyn Read> = if format == ArchiveFormat::TarGz {
                    Box::new(flate2::read::GzDecoder::new(file))
                } else {
                    Box::new(file)
                };
                let mut ar = tar::Archive::new(reader);
                ar.set_overwrite(false);
                let entries = ar.entries().map_err(|e| e.to_string())?;
                for entry in entries {
                    if outcome.cancelled {
                        break;
                    }
                    let mut entry = match entry {
                        Ok(e) => e,
                        Err(e) => {
                            outcome.failures.push(Failure {
                                path: archive.to_path_buf(),
                                message: format!("Archive entry error: {e}"),
                            });
                            continue;
                        }
                    };
                    let path = match entry.path() {
                        Ok(p) => p.to_string_lossy().to_string(),
                        Err(_) => continue,
                    };
                    let Some(rel) = safe_entry_path(&path) else {
                        outcome.failures.push(Failure {
                            path: PathBuf::from(path),
                            message: "Skipped unsafe archive entry (path traversal).".into(),
                        });
                        continue;
                    };
                    let target = dest.join(&rel);
                    let kind = entry.header().entry_type();
                    if kind.is_dir() {
                        let _ = stdfs::create_dir_all(&target);
                        tick("Extracting", &mut outcome);
                        continue;
                    }
                    if kind.is_symlink() || kind.is_hard_link() {
                        // Only allow links that resolve inside the destination.
                        if let Ok(Some(link)) = entry.link_name() {
                            let link = link.to_path_buf();
                            if link.is_absolute() || link.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                                outcome.failures.push(Failure {
                                    path: target.clone(),
                                    message: "Skipped unsafe archive link.".into(),
                                });
                                continue;
                            }
                        }
                    }
                    let target = match policy.decide(&target) {
                        Decision::Write(p) => p,
                        Decision::Skip => {
                            outcome.skipped += 1;
                            tick("Extracting", &mut outcome);
                            continue;
                        }
                    };
                    if let Some(parent) = target.parent() {
                        let _ = stdfs::create_dir_all(parent);
                    }
                    match entry.unpack(&target) {
                        Ok(_) => outcome.succeeded += 1,
                        Err(e) => outcome.failures.push(Failure {
                            path: target.clone(),
                            message: format!("Could not extract: {e}"),
                        }),
                    }
                    tick("Extracting", &mut outcome);
                }
            }
        }
        Ok(())
    })();

    if let Err(e) = result {
        outcome.failures.push(Failure { path: archive.to_path_buf(), message: e });
    }
    outcome.message = format!("Extracted {} item(s)", outcome.succeeded);
    if let Some(cb) = &progress {
        cb(Progress {
            fraction: 1.0,
            message: if outcome.cancelled { "Cancelled" } else { "Done" }.into(),
            done,
            total: total.max(done),
        });
    }
    outcome
}

/// Bulk move used by Organise. Conflict policy keeps names predictable.
pub fn organise_moves(
    moves: &[(PathBuf, PathBuf)],
    cancel: Arc<AtomicBool>,
    progress: Option<ProgressFn>,
) -> Outcome {
    let mut outcome = Outcome::default();
    let total = moves.len() as u64;
    for (i, (src, dest)) in moves.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            outcome.cancelled = true;
            break;
        }
        if let Some(parent) = dest.parent() {
            if let Err(e) = stdfs::create_dir_all(parent) {
                outcome.failures.push(Failure {
                    path: dest.clone(),
                    message: fs::describe_io_error(&e, parent),
                });
                continue;
            }
        }
        let target = if dest.exists() { fs::unique_path(dest.parent().unwrap_or(Path::new("/")), &fs::file_name_of(dest)) } else { dest.clone() };
        match stdfs::rename(src, &target) {
            Ok(()) => outcome.succeeded += 1,
            Err(_) => {
                // Cross-device fallback.
                let mut o = Outcome::default();
                let noop = AtomicBool::new(false);
                copy_entry(src, &target, Policy(ConflictPolicy::KeepBoth), &noop, &|_| {}, &mut o);
                if o.failures.is_empty() {
                    let _ = remove_path(src);
                    outcome.succeeded += 1;
                } else {
                    outcome.failures.extend(o.failures);
                }
            }
        }
        if let Some(cb) = &progress {
            cb(Progress {
                fraction: (i + 1) as f64 / total.max(1) as f64,
                message: "Organising".into(),
                done: (i + 1) as u64,
                total,
            });
        }
    }
    outcome.message = format!("Moved {} item(s)", outcome.succeeded);
    outcome
}
