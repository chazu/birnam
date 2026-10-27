//! Execution owns admitted call plans and the logical frame they prepare.
use super::*;
pub(in crate::synchronous) unsafe extern "C" fn prepared(
    _: Oop,
    c: Oop,
    machine: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [site, selector] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let site = abi::as_size(*site).ok_or_else(invalid)?;
        if selector.is_null() {
            return Err(invalid());
        }
        let q = s.scope(&[machine, *selector]);
        let cap = cap(s, machine)?;
        let (descriptor, call) = {
            let e = s.engine.borrow();
            let m = checked(e.machine(OWNER, cap))?;
            let frame = m.arena().current_handle().ok_or_else(invalid)?;
            let descriptor = checked(m.arena().descriptor_arc(frame))?;
            let plan = descriptor.call_plans.get(site).ok_or_else(invalid)?;
            let call = *descriptor.call_site(site).ok_or_else(invalid)?;
            if checked(m.arena().program_counter(frame))? != plan.pc {
                return Err(invalid());
            }
            (descriptor, call)
        };
        let plan = &descriptor.call_plans[site];
        let copy = |source: usize, destination: usize| -> Result<(), Oop> {
            let source = abi::size(source).ok_or_else(invalid)?;
            let destination = abi::size(destination).ok_or_else(invalid)?;
            let value = q.keep(s.send(machine, b"oopAt:", &[source])?);
            s.send(machine, b"oopAt:put:", &[destination, value])?;
            Ok(())
        };
        // Keep the established current frame operations visible. No engine borrow
        // spans these sends, including collecting or authored replacements.
        for transfer in &plan.copies {
            copy(transfer.source, transfer.destination)?;
        }
        copy(plan.sources[0], call.receiver_slot)?;
        s.send(
            machine,
            b"oopAt:put:",
            &[
                abi::size(call.selector_slot).ok_or_else(invalid)?,
                *selector,
            ],
        )?;
        for (i, source) in plan.sources[1..].iter().enumerate() {
            copy(*source, call.argument_start + i)?;
        }
        s.send(
            machine,
            if call.kind == CallSiteKind::TailCall {
                b"tailCallSite:"
            } else {
                b"callSite:"
            },
            &[abi::size(site).ok_or_else(invalid)?],
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
pub(in crate::synchronous) unsafe extern "C" fn field(
    _: Oop,
    c: Oop,
    machine: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [site, field] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[machine, *site, *field]);
        let site = index(s, *site)?;
        let field = index(s, *field)?;
        let cap = cap(s, machine)?;
        let value = {
            let e = s.engine.borrow();
            let m = checked(e.machine(OWNER, cap))?;
            let frame = m.arena().current_handle().ok_or_else(invalid)?;
            let descriptor = checked(m.arena().descriptor_arc(frame))?;
            let plan = descriptor.call_plans.get(site).ok_or_else(invalid)?;
            if checked(m.arena().program_counter(frame))? != plan.pc {
                return Err(invalid());
            }
            let call = descriptor.call_site(site).ok_or_else(invalid)?;
            match field {
                0 => call.receiver_slot,
                1 => call.selector_slot,
                2 => call.argument_start,
                3 => call.argument_count,
                4 => usize::from(call.kind == CallSiteKind::TailCall),
                5 => plan.copies.len(),
                n if n - 6 < plan.sources.len() => plan.sources[n - 6],
                n => {
                    let n = n - 6 - plan.sources.len();
                    let copy = plan.copies.get(n / 2).ok_or_else(invalid)?;
                    if n % 2 == 0 {
                        copy.source
                    } else {
                        copy.destination
                    }
                }
            }
        };
        let result = q.keep(number(s, value)?);
        Ok(result)
    })()
    .unwrap_or_else(abi::fault_reference)
}
