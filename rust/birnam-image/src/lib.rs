//! Portable Birnam image-v8 support and runtime provider.

#![allow(clippy::missing_safety_doc)]

mod native_code;
mod retained_facts;
pub mod v8;

/// Version-neutral support used by the image-v8 codec and runtime provider.
pub mod support {

    pub const AUTHORITY_DIGEST_LEN: usize = 64;

    pub const IMAGE_ARCH_TAG: u32 = if cfg!(target_arch = "aarch64") {
        1
    } else if cfg!(target_arch = "x86_64") {
        2
    } else {
        0
    };
    const RUNTIME_ABI_VERSION: u32 = 7;
    const RUNTIME_SHIM_NAMES: &[&[u8]] = &[
        b"birnam_send",
        b"birnam_intern",
        b"birnam_mkSmallInt",
        b"birnam_smallIntValue",
        b"birnam_mkFloat",
        b"birnam_floatValue",
        b"birnam_mkWord",
        b"birnam_wordValue",
        b"birnam_uintValue",
        b"birnam_mkForeignAddress",
        b"birnam_foreignAddressValue",
        b"birnam_isForeignAddress",
        b"birnam_mkString",
        b"birnam_nil",
        b"birnam_true",
        b"birnam_false",
        b"birnam_objectVt",
        b"birnam_proto",
        b"birnam_method",
        b"birnam_alloc",
        b"birnam_cstr",
        b"birnam_free_cstr",
        b"birnam_layoutAllOops",
        b"birnam_global",
        b"birnam_candidate_validate_binding",
    ];
    const REGISTRATION_SHIM_NAME: &[u8] = b"birnam_register_resumable_v1";
    const REGISTRATION_ABI_FIELDS_V1: &[(&[u8], u32, u32, u32, &[u8], &[u8])] = &[
        (b"carrier_version", 0x00, 4, 4, b"u32", b"not-applicable"),
        (b"struct_size", 0x04, 4, 4, b"u32", b"not-applicable"),
        (b"operation", 0x08, 4, 4, b"u32", b"not-applicable"),
        (b"flags", 0x0c, 4, 4, b"u32", b"not-applicable"),
        (
            b"manifest_entry_index",
            0x10,
            4,
            4,
            b"u32",
            b"not-applicable",
        ),
        (b"reserved0", 0x14, 4, 4, b"u32", b"not-applicable"),
        (b"registration_token", 0x18, 8, 8, b"u64", b"not-applicable"),
        (
            b"descriptor_bytes",
            0x20,
            8,
            8,
            b"pointer<const-u8>",
            b"prepare-nonnull-abort-null",
        ),
        (b"descriptor_length", 0x28, 8, 8, b"u64", b"not-applicable"),
        (
            b"entry_thunk",
            0x30,
            8,
            8,
            b"function-pointer<BirnamMethodV1>",
            b"prepare-nonnull-abort-null",
        ),
        (
            b"step_method",
            0x38,
            8,
            8,
            b"function-pointer<BirnamStepMethodV1>",
            b"prepare-nonnull-abort-null",
        ),
        (
            b"implementation_oop",
            0x40,
            8,
            8,
            b"BirnamOopV1",
            b"prepare-named-nonnull-block-null-abort-null",
        ),
        (
            b"code_generation_identity",
            0x48,
            8,
            8,
            b"u64",
            b"not-applicable",
        ),
        (
            b"code_generation_lease",
            0x50,
            8,
            8,
            b"u64",
            b"not-applicable",
        ),
        (
            b"portable_implementation_identity",
            0x58,
            8,
            8,
            b"u64",
            b"not-applicable",
        ),
        (b"language_arity", 0x60, 4, 4, b"u32", b"not-applicable"),
        (
            b"driver_argument_count",
            0x64,
            4,
            4,
            b"u32",
            b"not-applicable",
        ),
        (
            b"descriptor_sha256",
            0x68,
            32,
            1,
            b"bytes[32]",
            b"not-applicable",
        ),
    ];
    const REGISTRATION_ABI_CONSTANTS_V1: &[(&[u8], u32)] = &[
        (b"BIRNAM_REGISTRATION_CARRIER_VERSION_V1", 1),
        (b"BIRNAM_REGISTRATION_PREPARE_V1", 1),
        (b"BIRNAM_REGISTRATION_ABORT_V1", 2),
        (b"BIRNAM_REGISTRATION_OK_V1", 0),
        (b"BIRNAM_REGISTRATION_REJECTED_V1", 1),
    ];

    pub(crate) fn hash_registration_abi_tuple_v1(mut hash: u64) -> (u64, usize) {
        fn bytes(hash: &mut u64, count: &mut usize, value: &[u8]) {
            for byte in value {
                *hash ^= u64::from(*byte);
                *hash = hash.wrapping_mul(0x100000001b3);
            }
            *count += value.len();
        }

        fn u32_le(hash: &mut u64, count: &mut usize, value: u32) {
            bytes(hash, count, &value.to_le_bytes());
        }

        fn string(hash: &mut u64, count: &mut usize, value: &[u8]) {
            u32_le(hash, count, value.len() as u32);
            bytes(hash, count, value);
        }

        let mut count = 0;
        bytes(&mut hash, &mut count, b"birnam.registration-envelope.v1\0");
        string(
            &mut hash,
            &mut count,
            b"aarch64-apple-darwin|Mach-O|little-endian",
        );
        string(&mut hash, &mut count, b"birnam_register_resumable_v1");
        string(
            &mut hash,
            &mut count,
            b"u32(BirnamResumableRegistrationV1 *)",
        );
        for value in [136, 8, REGISTRATION_ABI_FIELDS_V1.len() as u32] {
            u32_le(&mut hash, &mut count, value);
        }
        for &(name, offset, width, alignment, abi_kind, nullability) in REGISTRATION_ABI_FIELDS_V1 {
            string(&mut hash, &mut count, name);
            for value in [offset, width, alignment] {
                u32_le(&mut hash, &mut count, value);
            }
            string(&mut hash, &mut count, abi_kind);
            string(&mut hash, &mut count, nullability);
        }
        u32_le(
            &mut hash,
            &mut count,
            REGISTRATION_ABI_CONSTANTS_V1.len() as u32,
        );
        for &(name, value) in REGISTRATION_ABI_CONSTANTS_V1 {
            string(&mut hash, &mut count, name);
            u32_le(&mut hash, &mut count, value);
        }
        (hash, count)
    }

    pub fn runtime_abi_hash() -> u64 {
        let mut hash = 0xcbf29ce484222325;
        for name in RUNTIME_SHIM_NAMES {
            for byte in *name {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
        for byte in REGISTRATION_SHIM_NAME {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash = hash_registration_abi_tuple_v1(hash).0;
        // Exact-entry adapters require this symbol before any generated code
        // can run. Reject earlier image runtimes at admission.
        for byte in b"birnam_enter_resumable_v1" {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        let mut version = RUNTIME_ABI_VERSION;
        for _ in 0..4 {
            hash ^= u64::from((version & 0xff) as u8);
            hash = hash.wrapping_mul(0x100000001b3);
            version >>= 8;
        }
        hash
    }

    #[test]
    fn exact_entry_runtime_abi_matches_the_install_receipt() {
        assert_eq!(runtime_abi_hash(), 0xf57d502f7c1c0437);
    }

    use std::fs;
    use std::io::{self, Write};
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum EncodeError {
        PayloadTooLarge,
    }

    /// Encoder for image-v8 canonical Oop references.
    #[derive(Default)]
    pub struct FieldEncoder {
        bytes: Vec<u8>,
    }

    impl FieldEncoder {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn nil(&mut self) {
            self.bytes.push(0);
        }

        pub fn true_value(&mut self) {
            self.bytes.push(1);
        }

        pub fn false_value(&mut self) {
            self.bytes.push(2);
        }

        pub fn heap_ref(&mut self, id: u32) {
            self.bytes.push(7);
            self.bytes.extend_from_slice(&id.to_le_bytes());
        }

        pub fn null_oop(&mut self) {
            self.bytes.push(8);
        }

        pub fn symbol(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
            self.length_delimited(5, bytes)
        }

        pub fn export_ref(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
            self.length_delimited(9, bytes)
        }

        pub fn finish(self) -> Vec<u8> {
            self.bytes
        }

        pub fn as_bytes(&self) -> &[u8] {
            &self.bytes
        }

        fn length_delimited(&mut self, tag: u8, bytes: &[u8]) -> Result<(), EncodeError> {
            let length = u32::try_from(bytes.len()).map_err(|_| EncodeError::PayloadTooLarge)?;
            self.bytes.push(tag);
            self.bytes.extend_from_slice(&length.to_le_bytes());
            self.bytes.extend_from_slice(bytes);
            Ok(())
        }
    }

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

    fn read_u64(bytes: &[u8]) -> u64 {
        u64::from_le_bytes(bytes.try_into().expect("fixed-width image field"))
    }

    fn read_u32(bytes: &[u8]) -> u32 {
        u32::from_le_bytes(bytes.try_into().expect("fixed-width image field"))
    }

    const WYHASH_SECRET: [u64; 4] = [
        0xa076_1d64_78bd_642f,
        0xe703_7ed1_a0b4_28db,
        0x8ebc_6af0_9c88_c6e3,
        0x5899_65cc_7537_4cc3,
    ];

    pub(crate) fn wyhash(seed: u64, input: &[u8]) -> u64 {
        let mut state = [seed ^ mix(seed ^ WYHASH_SECRET[0], WYHASH_SECRET[1]); 3];
        let (a, b) = if input.len() <= 16 {
            let mut a = 0;
            let mut b = 0;
            small_key(input, &mut a, &mut b);
            (a, b)
        } else {
            let mut offset = 0;
            if input.len() >= 48 {
                while offset + 48 < input.len() {
                    round(&mut state, &input[offset..offset + 48]);
                    offset += 48;
                }
                state[0] ^= state[1] ^ state[2];
            }
            let remaining = &input[offset..];
            let mut inner = 0;
            while inner + 16 < remaining.len() {
                state[0] = mix(
                    read_u64(&remaining[inner..inner + 8]) ^ WYHASH_SECRET[1],
                    read_u64(&remaining[inner + 8..inner + 16]) ^ state[0],
                );
                inner += 16;
            }
            (
                read_u64(&input[input.len() - 16..input.len() - 8]),
                read_u64(&input[input.len() - 8..]),
            )
        };
        let mut a = a ^ WYHASH_SECRET[1];
        let mut b = b ^ state[0];
        let product = (a as u128).wrapping_mul(b as u128);
        a = product as u64;
        b = (product >> 64) as u64;
        mix(
            a ^ WYHASH_SECRET[0] ^ input.len() as u64,
            b ^ WYHASH_SECRET[1],
        )
    }

    fn small_key(input: &[u8], a: &mut u64, b: &mut u64) {
        if input.len() >= 4 {
            let end = input.len() - 4;
            let quarter = (input.len() >> 3) << 2;
            *a = (u64::from(read_u32(&input[..4])) << 32)
                | u64::from(read_u32(&input[quarter..quarter + 4]));
            *b = (u64::from(read_u32(&input[end..end + 4])) << 32)
                | u64::from(read_u32(&input[end - quarter..end - quarter + 4]));
        } else if !input.is_empty() {
            *a = (u64::from(input[0]) << 16)
                | (u64::from(input[input.len() >> 1]) << 8)
                | u64::from(input[input.len() - 1]);
        }
    }

    fn round(state: &mut [u64; 3], input: &[u8]) {
        for index in 0..3 {
            state[index] = mix(
                read_u64(&input[index * 16..index * 16 + 8]) ^ WYHASH_SECRET[index + 1],
                read_u64(&input[index * 16 + 8..index * 16 + 16]) ^ state[index],
            );
        }
    }

    fn mix(left: u64, right: u64) -> u64 {
        let product = (left as u128).wrapping_mul(right as u128);
        product as u64 ^ (product >> 64) as u64
    }
}

#[allow(unused_imports)]
pub use support::*;

#[cfg(feature = "module")]
mod native;
