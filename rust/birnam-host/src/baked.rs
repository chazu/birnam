//! Pure executable trailer encoding, shared with the CLI image entry.
use std::{
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};
pub const BAKE_MAGIC: &[u8; 8] = b"BIRNMEXE";
pub const BAKE_TRAILER_LEN: usize = 16;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BakeError {
    EmptyImage,
    ImageTooLarge,
}
/// Append an image trailer to a runtime byte sequence. The runtime remains a
/// valid executable because the loader ignores trailing bytes; recovery uses
/// [`baked_image`] to select the exact image slice.
pub fn bake_executable(runtime: &[u8], image: &[u8]) -> Result<Vec<u8>, BakeError> {
    if image.is_empty() {
        return Err(BakeError::EmptyImage);
    }
    let image_len = u64::try_from(image.len()).map_err(|_| BakeError::ImageTooLarge)?;
    let capacity = runtime
        .len()
        .checked_add(image.len())
        .and_then(|length| length.checked_add(BAKE_TRAILER_LEN))
        .ok_or(BakeError::ImageTooLarge)?;
    let mut result = Vec::with_capacity(capacity);
    result.extend_from_slice(runtime);
    result.extend_from_slice(image);
    result.extend_from_slice(&image_len.to_le_bytes());
    result.extend_from_slice(BAKE_MAGIC);
    Ok(result)
}

/// Recover the image slice from a baked executable byte sequence. Invalid,
/// truncated, empty, or out-of-range trailers answer `None` without I/O.
pub fn baked_image(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() < BAKE_TRAILER_LEN || &bytes[bytes.len() - BAKE_MAGIC.len()..] != BAKE_MAGIC {
        return None;
    }
    let length_start = bytes.len() - BAKE_TRAILER_LEN;
    let image_len = u64::from_le_bytes(bytes[length_start..length_start + 8].try_into().ok()?);
    let image_len = usize::try_from(image_len).ok()?;
    if image_len == 0 || image_len > length_start {
        return None;
    }
    Some(&bytes[length_start - image_len..length_start])
}

/// Recover only the embedded image from a baked executable without reading the
/// runtime prefix into memory. A file without a valid trailer answers `None`.
pub fn read_baked_image(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let mut file = std::fs::File::open(path)?;
    let file_len = file.metadata()?.len();
    if file_len < BAKE_TRAILER_LEN as u64 {
        return Ok(None);
    }
    file.seek(SeekFrom::End(-(BAKE_TRAILER_LEN as i64)))?;
    let mut trailer = [0u8; BAKE_TRAILER_LEN];
    file.read_exact(&mut trailer)?;
    if &trailer[8..] != BAKE_MAGIC {
        return Ok(None);
    }
    let image_len = u64::from_le_bytes(trailer[..8].try_into().expect("bake image length"));
    let prefix_len = file_len
        .checked_sub(BAKE_TRAILER_LEN as u64)
        .and_then(|length| length.checked_sub(image_len));
    let Some(prefix_len) = prefix_len else {
        return Ok(None);
    };
    if image_len == 0 || image_len > usize::MAX as u64 {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(prefix_len))?;
    let mut image = vec![0u8; image_len as usize];
    file.read_exact(&mut image)?;
    Ok(Some(image))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bake_trailer_round_trips_exact_image_bytes() {
        let runtime = b"runtime-prefix";
        let image = b"BIRNAMIM\x01payload";
        let baked = bake_executable(runtime, image).expect("nonempty image bakes");
        assert_eq!(baked_image(&baked), Some(image.as_slice()));
        assert_eq!(&baked[..runtime.len()], runtime);
        assert!(baked_image(runtime).is_none());
    }

    #[test]
    fn bake_trailer_rejects_empty_and_corrupt_images() {
        assert_eq!(bake_executable(b"runtime", b""), Err(BakeError::EmptyImage));
        let mut corrupt = bake_executable(b"runtime", b"image").expect("fixture bakes");
        let trailer_start = corrupt.len() - BAKE_TRAILER_LEN;
        let magic_start = corrupt.len() - BAKE_MAGIC.len();
        corrupt[trailer_start..magic_start].copy_from_slice(&0u64.to_le_bytes());
        assert!(baked_image(&corrupt).is_none());
        corrupt[magic_start..].copy_from_slice(b"NOTBAKED");
        assert!(baked_image(&corrupt).is_none());
    }

    #[test]
    fn baked_image_file_reader_skips_the_runtime_prefix() {
        let path =
            std::env::temp_dir().join(format!("birnam-host-baked-reader-{}", std::process::id()));
        let image = b"BIRNAMIM\x08\0\0\0payload";
        let baked = bake_executable(&vec![b'r'; 1 << 20], image).expect("fixture bakes");
        std::fs::write(&path, baked).expect("fixture writes");
        assert_eq!(
            read_baked_image(&path).expect("fixture reads"),
            Some(image.to_vec())
        );
        std::fs::write(&path, b"plain runtime").expect("plain fixture writes");
        assert_eq!(read_baked_image(&path).expect("plain fixture reads"), None);
        std::fs::remove_file(path).expect("fixture removes");
    }
}
