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
use std::{fs, io::Write, os::unix::{ffi::OsStrExt, fs::DirBuilderExt}, path::PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
    fn __cxa_atexit(callback: unsafe extern "C" fn(*mut c_void), argument: *mut c_void, dso: *const c_void) -> c_int;
    static __dso_handle: u8;
    #[link_name = "dlopen"]
    fn system_dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    #[link_name = "dlsym"]
    fn system_dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    #[link_name = "dlclose"]
    fn system_dlclose(handle: *mut c_void) -> c_int;
}

// Native owners can live for the loaded DSO's entire lifetime. Tie filesystem
// cleanup to that DSO as well as ordinary registry Drop, without retaining Id
// objects or calling back into an already-destroyed runtime.
unsafe extern "C" fn remove_materialization(argument: *mut c_void) {
    let path = unsafe { Box::from_raw(argument.cast::<PathBuf>()) };
    let _ = fs::remove_dir_all(&*path);
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
    InvalidEmbeddedName,
    EmbeddedConflict,
    EmbeddedIo,
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
            Self::InvalidEmbeddedName => "embedded library requires a bare filename and non-empty bytes (maximum 256 MiB)",
            Self::EmbeddedConflict => "library name already denotes different content or a different open library",
            Self::EmbeddedIo => "could not materialize embedded library",
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
    embedded_dir: Option<PathBuf>,
    embedded: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>,
    adjacent: Vec<(Vec<u8>, Vec<u8>)>,
}

impl LibraryRegistry {
    /// Bind an artifact's declared native sibling without changing the caller's
    /// fallback directory or running constructors. The file remains caller-owned.
    pub fn provide_path(&mut self, name: &[u8], path: &[u8]) -> Result<(), LoaderError> {
        if name.is_empty() || name == b"." || name == b".." || name.iter().any(|b| matches!(b, 0 | b'/' | b'\\')) {
            return Err(LoaderError::InvalidEmbeddedName);
        }
        if self.embedded.iter().any(|(n, _, _)| n == name) { return Err(LoaderError::EmbeddedConflict); }
        let resolved = fs::canonicalize(std::ffi::OsStr::from_bytes(path)).map_err(|_| LoaderError::OpenFailed)?;
        if !resolved.is_file() { return Err(LoaderError::OpenFailed); }
        for entry in self.open.iter().filter(|entry| entry.requested == name) {
            if fs::canonicalize(std::ffi::OsStr::from_bytes(&entry.resolved)).ok().as_ref() != Some(&resolved) {
                return Err(LoaderError::EmbeddedConflict);
            }
        }
        let resolved = resolved.as_os_str().as_bytes().to_vec();
        if let Some(entry) = self.adjacent.iter_mut().find(|(n, _)| n == name) { entry.1 = resolved; }
        else { self.adjacent.push((name.to_vec(), resolved)); }
        Ok(())
    }
    /// Stage all bundled siblings before opening any of them. This performs no
    /// dlopen and runs no constructors. The registry owns the private directory
    /// until all retained loader handles have been closed at teardown.
    pub fn provide(&mut self, name: &[u8], bytes: &[u8]) -> Result<(), LoaderError> {
        if name.is_empty() || name == b"." || name == b".."
            || name.iter().any(|b| matches!(b, 0 | b'/' | b'\\'))
            || bytes.is_empty() || bytes.len() > 256 * 1024 * 1024 {
            return Err(LoaderError::InvalidEmbeddedName);
        }
        if let Some((_, existing, _)) = self.embedded.iter().find(|(n, _, _)| n == name) {
            return if existing == bytes { Ok(()) } else { Err(LoaderError::EmbeddedConflict) };
        }
        if self.open_count(name) != 0 || self.adjacent.iter().any(|(n, _)| n == name) { return Err(LoaderError::EmbeddedConflict); }
        if self.embedded_dir.is_none() {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            for _ in 0..128 {
                let path = std::env::temp_dir().join(format!("birnam-alien-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
                match fs::DirBuilder::new().mode(0o700).create(&path) {
                    Ok(()) => {
                        let cleanup = Box::into_raw(Box::new(path.clone()));
                        let registered = unsafe { __cxa_atexit(remove_materialization, cleanup.cast(), (&raw const __dso_handle).cast()) };
                        if registered != 0 {
                            unsafe { remove_materialization(cleanup.cast()) };
                            return Err(LoaderError::EmbeddedIo);
                        }
                        self.embedded_dir = Some(path); break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(_) => return Err(LoaderError::EmbeddedIo),
                }
            }
        }
        let directory = self.embedded_dir.as_ref().ok_or(LoaderError::EmbeddedIo)?;
        let path = directory.join(std::ffi::OsStr::from_bytes(name));
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path)
            .map_err(|_| LoaderError::EmbeddedIo)?;
        if file.write_all(bytes).is_err() {
            drop(file); let _ = fs::remove_file(&path); return Err(LoaderError::EmbeddedIo);
        }
        drop(file);
        self.embedded.push((name.to_vec(), bytes.to_vec(), path.as_os_str().as_bytes().to_vec()));
        Ok(())
    }
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

    /// Resolve a symbol from the process-wide loader namespace. Symbols from
    /// the main executable and platform libraries are process-owned; symbols
    /// supplied by an explicitly opened library remain protected by that
    /// library's retained registry entry.
    pub fn symbol_default(&self, name: &[u8]) -> Result<NonNull<c_void>, LoaderError> {
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
        if let Some((_, _, path)) = self.embedded.iter().find(|(name, _, _)| name == requested) {
            return path.clone();
        }
        if let Some((_, path)) = self.adjacent.iter().find(|(name, _)| name == requested) { return path.clone(); }
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
        if let Some(path) = self.embedded_dir.take() { let _ = fs::remove_dir_all(path); }
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

    #[test]
    fn embedded_materialization_is_inert_bounded_and_owned() {
        let mut registry = LibraryRegistry::default();
        for name in [b"../escape".as_slice(), b"/escape", b".", b"..", b"a\\b", b"a\0b", b""] {
            assert_eq!(registry.provide(name, b"bytes"), Err(LoaderError::InvalidEmbeddedName));
        }
        registry.provide(b"libprobe.dylib", b"not an executable").unwrap();
        let path = PathBuf::from(std::ffi::OsStr::from_bytes(&registry.resolve(b"libprobe.dylib")));
        assert_eq!(fs::read(&path).unwrap(), b"not an executable");
        assert_eq!(registry.open_count(b"libprobe.dylib"), 0, "providing never invokes dlopen");
        let mut adjacent = LibraryRegistry::default();
        adjacent.set_library_dir(b"/tmp/fallback").unwrap();
        adjacent.provide_path(b"libprobe.dylib", path.as_os_str().as_bytes()).unwrap();
        assert_eq!(adjacent.resolve(b"libprobe.dylib"), fs::canonicalize(&path).unwrap().as_os_str().as_bytes());
        assert_eq!(adjacent.resolve(b"other.dylib"), b"/tmp/fallback/other.dylib");
        assert_eq!(adjacent.open_count(b"libprobe.dylib"), 0);
        drop(adjacent);
        assert!(path.exists(), "adjacent files remain caller-owned");
        registry.provide(b"libprobe.dylib", b"not an executable").unwrap();
        assert_eq!(registry.provide(b"libprobe.dylib", b"different"), Err(LoaderError::EmbeddedConflict));
        let parent = path.parent().unwrap().to_owned();
        drop(registry);
        assert!(!parent.exists(), "private materialization is reclaimed");
    }

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
    fn process_symbols_resolve_without_an_explicit_library_open() {
        let registry = LibraryRegistry::default();
        registry
            .symbol_default(b"malloc")
            .expect("the process loader namespace resolves libc symbols");
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
