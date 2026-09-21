//! Canonical input shared by the 74G fixture and its committed-C golden.

pub(super) fn portable_program() -> Vec<u8> {
    use birnam_codegen::{Block, Expr, Statement, portable};
    use std::borrow::Cow;
    fn send(
        receiver: Expr<'static>,
        selector: &'static [u8],
        args: Vec<Expr<'static>>,
    ) -> Expr<'static> {
        Expr::Send {
            receiver: Box::new(receiver),
            selector: Cow::Borrowed(selector),
            args,
            source: b"",
        }
    }
    portable::encode_block(
        &Block {
            params: vec![b"input"],
            body: vec![
                Statement::Let {
                    name: b"carried",
                    value: send(Expr::Global(b"input"), b"probeDouble", vec![]),
                },
                Statement::Let {
                    name: b"alias",
                    value: Expr::Global(b"carried"),
                },
                Statement::Expr(send(
                    Expr::Global(b"Probe"),
                    b"probeEffect:alias:",
                    vec![Expr::Global(b"carried"), Expr::Global(b"alias")],
                )),
                Statement::Expr(send(
                    Expr::Global(b"Probe"),
                    b"probeFinish:alias:",
                    vec![Expr::Global(b"carried"), Expr::Global(b"alias")],
                )),
            ],
        },
        &[],
    )
    .unwrap()
}
