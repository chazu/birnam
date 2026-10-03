//! Atomic image-file publication remains with the live Image owner.
use std::{
    fs,
    io::{self, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
pub(crate) static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Publish complete image bytes through a same-directory temporary file and
/// rename. The destination is never opened for truncation; failed writes clean
/// up their temporary file and leave an existing destination untouched.
pub fn publish_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "image path has no file name")
    })?;
    let mut temporary_name = std::ffi::OsString::from(".");
    temporary_name.push(file_name);
    temporary_name.push(format!(
        ".birnam-tmp-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary_path = parent.join(temporary_name);

    let result = (|| -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}
