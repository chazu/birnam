use super::*;
use crate::runtime::numeric;
use crate::runtime::numeric::tests::Fixture;

thread_local! { static OBSERVATIONS: Cell<usize> = const { Cell::new(0) }; }
unsafe extern "C" fn observing_lookup(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    OBSERVATIONS.set(OBSERVATIONS.get() + 1);
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}

struct Mode(bool);
impl Drop for Mode {
    fn drop(&mut self) {
        numeric::ENABLED.set(self.0);
    }
}

#[test]
fn production_numeric_calls_use_current_targets_and_preserve_observing_lookup() {
    let bytes = portable::encode_block(
        &Block {
            params: vec![b"input"],
            body: vec![
                Statement::Let {
                    name: b"first",
                    value: Expr::Send {
                        receiver: Box::new(Expr::Global(b"input")),
                        selector: Cow::Borrowed(b"blend:"),
                        args: vec![Expr::Global(b"input")],
                        source: b"input blend: input",
                    },
                },
                Statement::Expr(Expr::Send {
                    receiver: Box::new(Expr::Global(b"first")),
                    selector: Cow::Borrowed(b"blend:"),
                    args: vec![Expr::Global(b"input")],
                    source: b"first blend: input",
                }),
            ],
        },
        &[],
    )
    .unwrap();
    let mut evidence = Vec::new();
    for observing in [false, true] {
        for enabled in [false, true] {
            let _mode = Mode(numeric::ENABLED.replace(enabled));
            numeric::HITS.set(0);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let native = Fixture::load(r);
            let blend = r.intern(b"blend:").unwrap();
            r.add_method(r.canonical().smallint_vt, blend, native.method(1));
            let block = install(r, &bytes, 741500, None);
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let input = r.make_integer(11);
            let _input = ScopedOopRootGuard::new(r, input);
            if observing {
                r.add_method(r.globals().vtable_vt, r.globals().lookup, observing_lookup);
            }
            let invoke = || r.enter(|| r.send_slice(block.receiver, block.selector, &[input]));
            for _ in 0..128 {
                assert_eq!(r.integer_value(invoke()), 33);
            }
            OBSERVATIONS.set(0);
            numeric::HITS.set(0);
            let before = r.gc_live_bytes();
            for _ in 0..16 {
                assert_eq!(r.integer_value(invoke()), 33);
            }
            let allocation_bytes = r.gc_live_bytes() - before;
            assert_eq!(numeric::HITS.get() > 0, enabled);
            assert_eq!(OBSERVATIONS.get() > 0, observing);
            evidence.push((allocation_bytes, OBSERVATIONS.get()));
            // A familiar or unfamiliar selector says nothing about its meaning.
            // Rebinding the same site to subtraction changes the next answer.
            r.add_method(r.canonical().smallint_vt, blend, native.method(2));
            assert_eq!(r.integer_value(invoke()), -11);
            r.enter(|| r.gc_collect_from_native_range(0, 0));
            assert_eq!(r.integer_value(invoke()), -11);
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
        }
    }
    assert_eq!(evidence[0], evidence[1], "canonical allocation parity");
    assert_eq!(
        evidence[2], evidence[3],
        "custom lookup effects and allocation parity"
    );
}
