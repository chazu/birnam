//! Full authored control unit, with binding changes confined to untimed setup.
use super::*;

pub(super) unsafe fn load_stdlib(rt: &Runtime) {
    unsafe {
        let path = directory().join("libbirnam_stdlib.dylib");
        let text = object(send(
            rt.id,
            rt.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(path.as_os_str().as_bytes())],
        ));
        let token = retain(rt.id, rt.roots, text);
        let result = object(send(
            rt.id,
            rt.named(b"Codegen"),
            b"load:",
            &[Operand::object(text)],
        ));
        assert_ne!(
            object(send(rt.id, result, b"isError", &[])),
            rt.named(b"true")
        );
        release(rt.id, rt.roots, token);
        let module = object(send(
            rt.id,
            rt.ns,
            b"resolveGlobal:",
            &[Operand::object(rt.id.intern_name(b"Module").unwrap())],
        ));
        send(
            rt.id,
            module,
            b"markLoaded:",
            &[Operand::object(rt.id.intern_name(b"stdlib").unwrap())],
        );
    }
}

pub(super) unsafe fn policy(
    rt: &mut Runtime,
    compiler: &mut Compiler,
    scratch: &std::path::Path,
    mask: usize,
) -> usize {
    unsafe {
        let source = std::fs::read_to_string("modules/stackless-control.birnam").unwrap();
        let declarations: Vec<_> = source
            .split("[Reflect defineMethod: #")
            .skip(1)
            .map(|s| {
                let selector = s.split_whitespace().next().unwrap().as_bytes().to_vec();
                let target = s
                    .split(" on: ")
                    .nth(1)
                    .unwrap()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .as_bytes()
                    .to_vec();
                (selector, target)
            })
            .collect();
        let case = Case {
            name: format!("stackless-policy-{mask}"),
            source: format!(
                "{}\n[Globals at: #MicroProbe put: Block]; true",
                source.replace("#stacklessControl", &format!("#microControls{mask}"))
            ),
            mask,
            expected: 0,
            result_selector: b"word",
            integer_inputs: true,
        };
        compile_case(rt, &case, mask, compiler, scratch, false);
        let mut entries = Vec::new();
        for (name, target) in declarations {
            let target = object(send(
                rt.id,
                rt.ns,
                b"resolveGlobal:",
                &[Operand::object(rt.id.intern_name(&target).unwrap())],
            ));
            let behavior = if send(
                rt.id,
                rt.id.receiver,
                b"isBehavior:",
                &[Operand::object(target)],
            )
            .as_word()
                == Some(1)
            {
                target
            } else {
                object(send(
                    rt.id,
                    rt.id.receiver,
                    b"behaviorOf:",
                    &[Operand::object(target)],
                ))
            };
            let selector = rt.id.intern_name(&name).unwrap();
            let entry = send(
                rt.id,
                rt.id.receiver,
                b"bindingOf:in:",
                &[Operand::object(selector), Operand::object(behavior)],
            )
            .as_entry()
            .unwrap();
            let method = object(send(
                rt.id,
                rt.named(b"Execution"),
                b"methodOfEntry:",
                &[Operand::entry(entry)],
            ));
            retain(rt.id, rt.roots, method);
            retain(rt.id, rt.roots, behavior);
            entries.push((behavior, selector, entry));
        }
        let index = rt.bindings.len();
        rt.bindings.push(entries);
        index
    }
}
