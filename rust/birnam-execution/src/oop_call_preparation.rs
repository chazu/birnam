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
        // This operation owns the complete transfer. Explicit slot messages remain
        // replaceable, but are not callbacks inside default call preparation.
        // All source and destination slots were checked when the plan was created.
        // No send or collection occurs while the frame is being changed.
        let mut engine = s.engine.borrow_mut();
        let mut copy = |source: usize, destination: usize| -> Result<(), Oop> {
            let value = checked(engine.read_current_oop(OWNER, cap, source))?;
            checked(engine.write_current_oop(OWNER, cap, destination, value))?;
            Ok(())
        };
        for transfer in &plan.copies {
            copy(transfer.source, transfer.destination)?;
        }
        copy(plan.sources[0], call.receiver_slot)?;
        for (i, source) in plan.sources[1..].iter().enumerate() {
            copy(*source, call.argument_start + i)?;
        }
        checked(engine.write_current_oop(OWNER, cap, call.selector_slot, *selector))?;
        checked(engine.publish_call_site(&mut s.storage.borrow_mut(), OWNER, cap, site, call.kind))

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
