//! Live, validated views of Execution-owned frames.
use super::*;

pub(in crate::synchronous) const METHODS: &[(&[u8], &[u8])] = &[
    (b"implementationIdentity", b"implementationIdentity"),
    (b"methodIdentity", b"implementationIdentity"),
    (b"generationIdentity", b"generationIdentity"),
    (b"sourceMapIdentity", b"sourceMapIdentity"),
    (b"programCounter", b"programCounter"),
    (b"sourceMapLocation", b"programCounter"),
    (b"slotCount", b"slotCount"),
    (b"oopAt:", b"frameViewOopAt:"),
    (b"slotKindAt:", b"slotKindAt:"),
    (b"clone", b"frameViewClone"),
];

unsafe fn error(s: &State, error: FrameViewError) -> Result<Oop, Oop> {
    let kind: &[u8] = match error {
        FrameViewError::InvalidFrameSlot => b"invalidFrameSlot",
        _ => b"staleFrameView",
    };
    unsafe { s.send(s.named(b"Error")?, b"kind:", &[s.names[kind]]) }
}

pub(in crate::synchronous) unsafe extern "C" fn current(
    _: Oop,
    c: Oop,
    r: Oop,
    _: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let q = s.scope(&[r]);
        let (handle, vt) = {
            let engine = s.engine.borrow();
            (
                checked(engine.current_frame_view_handle(OWNER, cap(s, r)?))?,
                engine.objects().ok_or_else(invalid)?.frame_view_vtable,
            )
        };
        let Some(handle) = handle else {
            return Ok(s.nil);
        };
        let view = q.keep(allocate(s, vt, 4)?);
        checked(s.engine.borrow().initialize_frame_view(
            &mut s.storage.borrow_mut(),
            view,
            r,
            handle,
        ))?;
        Ok(view)
    })()
    .unwrap_or_else(abi::fault_reference)
}

pub(in crate::synchronous) unsafe extern "C" fn word<const FIELD: usize>(
    _: Oop,
    c: Oop,
    r: Oop,
    _: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        let _q = s.scope(&[r]);
        let snapshot = s
            .engine
            .borrow()
            .frame_view_snapshot(&s.storage.borrow(), OWNER, r);
        let snapshot = match snapshot {
            Ok(v) => v,
            Err(e) => return error(s, e),
        };
        let value = match FIELD {
            0 => snapshot.identity.implementation,
            1 => snapshot.identity.generation,
            2 => snapshot.identity.source_map,
            3 => return number(s, snapshot.program_counter),
            _ => return number(s, snapshot.slot_count),
        };
        s.send(
            s.named(b"Word")?,
            b"fromUnsignedLow:high:",
            &[
                abi::size(value as u32 as usize).unwrap(),
                abi::size((value >> 32) as usize).unwrap(),
            ],
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}

pub(in crate::synchronous) unsafe extern "C" fn read<const KIND: bool>(
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [i] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let _q = s.scope(&[r, *i]);
        let i = match index(s, *i) {
            Ok(i) => i,
            Err(_) => return error(s, FrameViewError::InvalidFrameSlot),
        };
        let result = {
            let engine = s.engine.borrow();
            let storage = s.storage.borrow();
            if KIND {
                engine
                    .frame_view_slot_kind(&storage, OWNER, r, i)
                    .map(|_| s.names[b"oop".as_slice()])
            } else {
                engine.frame_view_oop(&storage, OWNER, r, i)
            }
        };
        match result {
            Ok(v) => Ok(v),
            Err(e) => error(s, e),
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}

pub(in crate::synchronous) unsafe fn install(s: &State, image: Oop, vt: Oop) -> Result<(), Oop> {
    unsafe {
        let q = s.scope(&[image, vt]);
        for &(selector, declaration) in METHODS {
            let method = q.keep(s.send(image, b"methodAt:", &[s.names[declaration]])?);
            let method = q.keep(s.send(method, b"withContext:", &[s.ns])?);
            s.send(vt, b"addMethod", &[s.names[selector], method])?;
        }
        Ok(())
    }
}
