use crate::HostError;
use std::{fs, io::Read, path::Path};
pub const MAX_SCRIPT_BYTES: u64 = 16 * 1024 * 1024;
pub fn read_source_file(path: &Path) -> Result<Vec<u8>, HostError> {
    let read = || -> Result<Vec<u8>, HostError> {
        let metadata = fs::metadata(path).map_err(|e| HostError(e.to_string()))?;
        if !metadata.is_file() {
            return Err(HostError("source must be a regular file".into()));
        }
        let mut source = Vec::new();
        fs::File::open(path)
            .and_then(|f| f.take(MAX_SCRIPT_BYTES + 1).read_to_end(&mut source))
            .map_err(|e| HostError(e.to_string()))?;
        if source.len() as u64 > MAX_SCRIPT_BYTES {
            return Err(HostError(format!(
                "source exceeds {MAX_SCRIPT_BYTES} bytes"
            )));
        }
        Ok(source)
    };
    read().map_err(|e| HostError(format!("{}: {e}", path.display())))
}
