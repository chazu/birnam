//! Value-only compilation requests. These are frontend-local records; native
//! consumers marshal them into Objects rather than sharing Rust representations.
#[derive(Clone)]
pub enum Request {
    Program {
        program: Vec<u8>,
        source: Vec<u8>,
        filename: Vec<u8>,
        persistent: bool,
        synchronous: bool,
    },
    Factory {
        program: Vec<u8>,
        source: Vec<u8>,
        dispositions: Vec<bool>,
    },
    Named {
        arity: u32,
        synchronous: bool,
    },
}
impl Request {
    pub fn program(program: crate::Program<'_>, source: &[u8], filename: &[u8]) -> Option<Self> {
        let program = crate::portable::encode_block(
            &crate::Block {
                params: vec![],
                body: program.body,
            },
            &[],
        )
        .ok()?;
        Some(Self::Program {
            program,
            source: source.to_vec(),
            filename: filename.to_vec(),
            persistent: true,
            synchronous: false,
        })
    }
}
