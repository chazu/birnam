//! Frozen image-v8 codecs and protocol value helpers. No native owner or heap
//! restoration state; production format admission lives in ImageFormat.
pub mod behavior_links;
pub mod retained_facts;
pub mod v8;
pub mod support {

    pub const AUTHORITY_DIGEST_LEN: usize = 64;

    pub const IMAGE_ARCH_TAG: u32 = if cfg!(target_arch = "aarch64") {
        1
    } else if cfg!(target_arch = "x86_64") {
        2
    } else {
        0
    };
    /// Frozen image-v8 compatibility seed. The reference format records its
    /// historical derivation; those retired runtime shims are not live APIs.
    /// Image combines this value with its current Oop image revision.
    pub const IMAGE_V8_ABI_SEED: u64 = 0xf57d502f7c1c0437;

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

    pub fn wyhash(seed: u64, input: &[u8]) -> u64 {
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
