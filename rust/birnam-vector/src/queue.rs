//! Persistent FIFO data operations. Queue owns its three reference slots;
//! private pair nodes are ordinary ObjectModel slots, accessed through messages.
use super::*;
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct Queue {
    front: Oop,
    back: Oop,
    count: Oop,
}
impl Queue {
    pub(super) fn empty(nil: Oop) -> Self {
        Self {
            front: nil,
            back: nil,
            count: abi::size(0).unwrap(),
        }
    }
}
unsafe fn read(s: &State, value: Oop) -> Result<Queue, Oop> {
    unsafe {
        if s.send(s.id.root, b"allocationSizeOf:", &[value])?
            != abi::size(size_of::<Queue>()).unwrap()
        {
            return Err(invalid());
        }
        let q = value.cast::<Queue>().read();
        abi::as_size(q.count).ok_or_else(invalid)?;
        Ok(q)
    }
}
unsafe fn make(s: &State, scope: &Scope<'_>, q: Queue) -> Result<Oop, Oop> {
    unsafe {
        scope.keep(q.front);
        scope.keep(q.back);
        let object = s.allocate(scope, s.queue_behavior, size_of::<Queue>())?;
        object.cast::<Queue>().write(q);
        Ok(object)
    }
}
unsafe fn part(s: &State, node: Oop, i: usize) -> Result<Oop, Oop> {
    unsafe { object(s.send(s.model, b"slotAt:in:", &[abi::size(i).unwrap(), node])?) }
}
unsafe fn link(s: &State, scope: &Scope<'_>, value: Oop, next: Oop) -> Result<Oop, Oop> {
    unsafe {
        scope.keep(value); scope.keep(next);
        let node = scope.keep(s.send(s.pair,b"clone",&[])?);
        s.send(s.model,b"slotAt:in:put:",&[abi::size(0).unwrap(),node,value])?;
        s.send(s.model,b"slotAt:in:put:",&[abi::size(1).unwrap(),node,next])?;
        Ok(node)
    }
}
unsafe fn run(s: &State, r: Oop, args: &[Oop], op: u8) -> Result<Oop, Oop> {
    unsafe {
        if args.len() != usize::from(op == 1) {
            return Err(invalid());
        }
        Scope::run(s, args, |scope| {
            if op == 0 {
                return make(s, scope, Queue::empty(s.nil()));
            }
            let mut q = read(s, r)?;
            scope.keep(q.front);
            scope.keep(q.back);
            let count = abi::as_size(q.count).ok_or_else(invalid)?;
            match op {
                1 => {
                    let next_count = count
                        .checked_add(1)
                        .and_then(abi::size)
                        .ok_or_else(invalid)?;
                    q.back = link(s, scope, args[0], q.back)?;
                    q.count = next_count;
                    make(s, scope, q)
                }
                2 => s.integer(i64::try_from(count).map_err(|_| invalid())?),
                3 => {
                    if count == 0 {
                        return build(s, [s.nil(), r]);
                    }
                    if q.front == s.nil() {
                        let mut back = q.back;
                        let mut front = s.nil();
                        for _ in 0..count {
                            if back == s.nil() {
                                return Err(invalid());
                            }
                            let value = scope.keep(part(s, back, 0)?);
                            let next = scope.keep(part(s, back, 1)?);
                            front = link(s, scope, value, front)?;
                            back = next;
                        }
                        if back != s.nil() {
                            return Err(invalid());
                        }
                        q.front = front;
                        q.back = s.nil();
                    }
                    let value = scope.keep(part(s, q.front, 0)?);
                    q.front = scope.keep(part(s, q.front, 1)?);
                    q.count = abi::size(count - 1).unwrap();
                    let rest = scope.keep(make(s, scope, q)?);
                    build(s, [value, rest])
                }
                4 => {
                    let mut values = Vec::new();
                    let mut node = q.front;
                    while node != s.nil() {
                        if values.len() == count {
                            return Err(invalid());
                        }
                        values.push(scope.keep(part(s, node, 0)?));
                        node = scope.keep(part(s, node, 1)?);
                    }
                    let front = values.len();
                    node = q.back;
                    while node != s.nil() {
                        if values.len() == count {
                            return Err(invalid());
                        }
                        values.push(scope.keep(part(s, node, 0)?));
                        node = scope.keep(part(s, node, 1)?);
                    }
                    if values.len() != count {
                        return Err(invalid());
                    }
                    values[front..].reverse();
                    build(s, values)
                }
                _ => Err(invalid()),
            }
        })
    }
}
macro_rules! entry {
    ($name:ident,$op:expr) => {
        pub(super) unsafe extern "C" fn $name(
            _: Oop,
            c: Oop,
            r: Oop,
            a: *const Oop,
            n: usize,
        ) -> Oop {
            done((|| unsafe {
                run(state(c)?, r, borrowed_slice(a, n).ok_or_else(invalid)?, $op)
            })())
        }
    };
}
entry!(new, 0);
entry!(add, 1);
entry!(size, 2);
entry!(take, 3);
entry!(vector, 4);
pub(super) const BINDINGS: &[(&[u8], &[u8])] = &[
    (b"queueImageSlots", b"imageAcceptsSlots:"),
    (b"queueNew", b"new"),
    (b"queueAdd", b"add:"),
    (b"queueSize", b"size"),
    (b"queueTake", b"takeFirst"),
    (b"queueVector", b"asVector"),
];

pub(super) unsafe extern "C" fn image_slots(_:Oop,c:Oop,_:Oop,a:*const Oop,n:usize)->Oop {
    done((|| unsafe {
        let s=state(c)?;
        let [words]=borrowed_slice(a,n).ok_or_else(invalid)? else {return Err(invalid());};
        Ok(s.value(if abi::as_size(*words)==Some(3) {b"true"} else {b"false"}))
    })())
}
