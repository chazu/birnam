//! Owned dynamic-library handles for the Alien boundary.
//!
//! This layer owns only native loader state. It deliberately has no `IdApi`,
//! object, allocator, TCC, or callback dependency; those authorities are
//! introduced by later R5 slices. Requested paths remain the lookup key so
//! image/replay code can preserve the logical source spelling while the
//! optional library directory controls where a bare name is materialized.

use core::ffi::{c_char, c_int, c_void};
use core::fmt;
use core::ptr::NonNull;

const RTLD_LAZY: c_int = 1;

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "visionos"
))]
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;
#[cfg(target_os = "linux")]
const RTLD_DEFAULT: *mut c_void = core::ptr::null_mut();

unsafe extern "C" {
    #[link_name = "dlopen"]
    fn system_dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    #[link_name = "dlsym"]
    fn system_dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    #[link_name = "dlclose"]
    fn system_dlclose(handle: *mut c_void) -> c_int;
}

/// A loader operation can fail without exposing platform-specific `dlerror`
/// strings across the Birnam boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoaderError {
    EmptyPath,
    EmbeddedNul,
    OpenFailed,
    NotOpen,
    EmptySymbol,
    SymbolEmbeddedNul,
    SymbolNotFound,
}

impl fmt::Display for LoaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyPath => "library path is empty",
            Self::EmbeddedNul => "library path contains an embedded NUL",
            Self::OpenFailed => "library could not be opened",
            Self::NotOpen => "library path has no open handle",
            Self::EmptySymbol => "symbol name is empty",
            Self::SymbolEmbeddedNul => "symbol name contains an embedded NUL",
            Self::SymbolNotFound => "symbol was not found in the open library",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for LoaderError {}

struct OpenLibrary {
    requested: Vec<u8>,
    resolved: Vec<u8>,
    // A loader handle is process-owned opaque state. Store its address as an
    // integer so the registry can be protected by a Rust mutex without
    // pretending that platform handles carry Rust thread-safety semantics.
    handle: usize,
}

/// One process-local owner of Alien's native library handles.
#[derive(Default)]
pub struct LibraryRegistry {
    library_dir: Option<Vec<u8>>,
    open: Vec<OpenLibrary>,
}

impl LibraryRegistry {
    /// Set or replace the directory used to resolve a bare library name.
    pub fn set_library_dir(&mut self, path: &[u8]) -> Result<(), LoaderError> {
        validate_path(path)?;
        self.library_dir = Some(path.to_vec());
        Ok(())
    }

    /// Open one library and retain one closeable entry. Repeated opens of the
    /// same requested path deliberately create repeated entries, matching the
    /// platform loader's reference-count semantics.
    pub fn open(&mut self, requested: &[u8]) -> Result<(), LoaderError> {
        validate_path(requested)?;
        let resolved = self.resolve(requested);
        let path = c_string(&resolved)?;
        // SAFETY: `path` is NUL-terminated and alive for the call; RTLD_LAZY is
        // the frozen Alien policy. The platform owns the returned handle.
        let handle = unsafe { system_dlopen(path.as_ptr(), RTLD_LAZY) };
        let Some(handle) = NonNull::new(handle) else {
            return Err(LoaderError::OpenFailed);
        };
        self.open.push(OpenLibrary {
            requested: requested.to_vec(),
            resolved,
            handle: handle.as_ptr() as usize,
        });
        Ok(())
    }

    /// Resolve a symbol from one or more handles opened under `requested`.
    /// Duplicate opens are searched newest-first, preserving the most recent
    /// loader binding while any matching handle remains live.
    pub fn symbol(&self, requested: &[u8], name: &[u8]) -> Result<NonNull<c_void>, LoaderError> {
        validate_path(requested)?;
        if name.is_empty() {
            return Err(LoaderError::EmptySymbol);
        }
        let name = c_string(name).map_err(|error| match error {
            LoaderError::EmbeddedNul => LoaderError::SymbolEmbeddedNul,
            other => other,
        })?;
        let Some(entry) = self
            .open
            .iter()
            .rev()
            .find(|entry| entry.requested == requested)
        else {
            return Err(LoaderError::NotOpen);
        };
        // SAFETY: the handle is retained by `self` and `name` is a live C
        // string for the duration of the lookup.
        let symbol = unsafe { system_dlsym(entry.handle as *mut c_void, name.as_ptr()) };
        NonNull::new(symbol).ok_or(LoaderError::SymbolNotFound)
    }

    /// Resolve a symbol from the process-wide loader namespace. The registry
    /// must retain at least one open entry before this is used so callers keep
    /// the library lifetime stronger than any patched wrapper address.
    pub fn symbol_default(&self, name: &[u8]) -> Result<NonNull<c_void>, LoaderError> {
        if self.open.is_empty() {
            return Err(LoaderError::NotOpen);
        }
        if name.is_empty() {
            return Err(LoaderError::EmptySymbol);
        }
        let name = c_string(name).map_err(|error| match error {
            LoaderError::EmbeddedNul => LoaderError::SymbolEmbeddedNul,
            other => other,
        })?;
        // SAFETY: `name` is a live C string and the registry retains all
        // successful `dlopen` entries for the duration of the lookup.
        let symbol = unsafe { system_dlsym(RTLD_DEFAULT, name.as_ptr()) };
        NonNull::new(symbol).ok_or(LoaderError::SymbolNotFound)
    }

    /// Release exactly one open entry for `requested`.
    pub fn close(&mut self, requested: &[u8]) -> Result<(), LoaderError> {
        validate_path(requested)?;
        let Some(index) = self
            .open
            .iter()
            .position(|entry| entry.requested == requested)
        else {
            return Err(LoaderError::NotOpen);
        };
        let entry = self.open.swap_remove(index);
        // SAFETY: this entry owns one successful `dlopen` reference and has
        // not previously been closed.
        let _ = unsafe { system_dlclose(entry.handle as *mut c_void) };
        Ok(())
    }

    /// Number of retained opens for a requested path.
    pub fn open_count(&self, requested: &[u8]) -> usize {
        self.open
            .iter()
            .filter(|entry| entry.requested == requested)
            .count()
    }

    /// Return the materialized path used by the newest matching open.
    pub fn resolved_path(&self, requested: &[u8]) -> Option<&[u8]> {
        self.open
            .iter()
            .rev()
            .find(|entry| entry.requested == requested)
            .map(|entry| entry.resolved.as_slice())
    }

    fn resolve(&self, requested: &[u8]) -> Vec<u8> {
        let Some(directory) = self.library_dir.as_deref() else {
            return requested.to_vec();
        };
        if requested.contains(&b'/') || requested.contains(&b'\\') {
            return requested.to_vec();
        }
        let mut resolved = Vec::with_capacity(directory.len() + 1 + requested.len());
        resolved.extend_from_slice(directory);
        if !resolved.ends_with(b"/") && !resolved.ends_with(b"\\") {
            resolved.push(b'/');
        }
        resolved.extend_from_slice(requested);
        resolved
    }
}

impl Drop for LibraryRegistry {
    fn drop(&mut self) {
        for entry in self.open.drain(..) {
            // SAFETY: every drained entry owns one successful `dlopen`
            // reference and no other method can observe the registry during
            // `drop`.
            let _ = unsafe { system_dlclose(entry.handle as *mut c_void) };
        }
    }
}

fn validate_path(path: &[u8]) -> Result<(), LoaderError> {
    if path.is_empty() {
        return Err(LoaderError::EmptyPath);
    }
    if path.contains(&0) {
        return Err(LoaderError::EmbeddedNul);
    }
    Ok(())
}

fn c_string(bytes: &[u8]) -> Result<std::ffi::CString, LoaderError> {
    std::ffi::CString::new(bytes).map_err(|_| LoaderError::EmbeddedNul)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system_library() -> &'static [u8] {
        #[cfg(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "watchos"
        ))]
        {
            b"libSystem.B.dylib"
        }
        #[cfg(target_os = "linux")]
        {
            b"libc.so.6"
        }
    }

    #[test]
    fn rejects_invalid_paths_and_symbols_before_loader_calls() {
        let mut registry = LibraryRegistry::default();
        assert_eq!(registry.open(b""), Err(LoaderError::EmptyPath));
        assert_eq!(registry.open(b"bad\0path"), Err(LoaderError::EmbeddedNul));
        assert_eq!(registry.close(b"missing"), Err(LoaderError::NotOpen));
        assert_eq!(
            registry.symbol(b"missing", b""),
            Err(LoaderError::EmptySymbol)
        );
    }

    #[test]
    fn tracks_repeated_open_and_single_close_entries() {
        let mut registry = LibraryRegistry::default();
        let path = system_library();
        registry.open(path).expect("platform C library opens");
        registry
            .open(path)
            .expect("second open increments loader refcount");
        assert_eq!(registry.open_count(path), 2);
        assert!(registry.resolved_path(path).is_some());
        registry
            .symbol(path, b"malloc")
            .expect("standard allocator symbol resolves");
        registry
            .symbol_default(b"malloc")
            .expect("default loader namespace resolves standard allocator");
        registry.close(path).expect("one close succeeds");
        assert_eq!(registry.open_count(path), 1);
        registry.close(path).expect("second close succeeds");
        assert_eq!(registry.open_count(path), 0);
        assert_eq!(registry.close(path), Err(LoaderError::NotOpen));
    }

    #[test]
    fn bare_names_use_library_directory_but_explicit_paths_do_not() {
        let mut registry = LibraryRegistry::default();
        registry.set_library_dir(b"/tmp/birnam-libs").unwrap();
        assert_eq!(
            registry.resolve(b"fixture.dylib"),
            b"/tmp/birnam-libs/fixture.dylib"
        );
        assert_eq!(registry.resolve(b"./fixture.dylib"), b"./fixture.dylib");
        assert_eq!(
            registry.resolve(b"/opt/fixture.dylib"),
            b"/opt/fixture.dylib"
        );
    }
}
