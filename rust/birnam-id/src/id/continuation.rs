//! Continuation transfers use captured Methods and owned Sequence contexts.
//! Only this bootstrap owner reads those contexts. The trampoline reads Method
//! declaration metadata, never Execution payloads or a catalog of object kinds.
use super::{
    id_dispatch::{self, Selection},
    native_id::{OPERATIONS, State},
};
use crate::abi::{self, Oop, id_protocol::Method};
use core::ptr;

pub(super) struct Drive {
    synchronous: bool,
    native_depth: usize,
    pub paused: Option<Oop>,
}
struct DriveScope<'a> {
    s: &'a State,
    index: usize,
}
impl Drop for DriveScope<'_> {
    fn drop(&mut self) {
        let mut drives = self.s.drives.borrow_mut();
        debug_assert_eq!(drives.len(), self.index + 1);
        drives.pop();
    }
}
pub(super) struct NativeScope<'a> {
    s: &'a State,
    index: Option<usize>,
}
impl<'a> NativeScope<'a> {
    pub fn enter(s: &'a State) -> Self {
        let mut drives = s.drives.borrow_mut();
        let index = drives.len().checked_sub(1);
        if let Some(index) = index {
            drives[index].native_depth += 1;
        }
        Self { s, index }
    }
}
impl Drop for NativeScope<'_> {
    fn drop(&mut self) {
        if let Some(index) = self.index {
            self.s.drives.borrow_mut()[index].native_depth -= 1;
        }
    }
}
const SEND: usize = 0;
const ENTER: usize = 1;
const LOOKED_UP: usize = 2;
const RESULT: usize = 3;
pub(super) static ENTRIES: &[Method] = &[send_work, enter_work, looked_up, result];

impl State {
    fn control_failure(&self, name: &'static [u8]) -> Oop {
        self.make_fault(self.selector(name), ptr::null_mut())
    }
    fn work(&self, entry: usize, values: &[Oop]) -> Result<Oop, Oop> {
        // Both constructors are owner-local, non-callback allocation. Protect
        // the inputs independently of their previous owner throughout creation.
        let _roots = self.scope(self.root, ptr::null_mut(), values);
        for &value in values {
            self.behavior(value)?;
        }
        let context = self.new_sequence(values)?;
        self.new_method_index(OPERATIONS.len() + entry, context, self.root)
    }
    pub(super) fn prepare_send(&self, values: &[Oop]) -> Result<Oop, Oop> {
        let [_, _, k, ..] = values else {
            return Err(self.invalid());
        };
        if !k.is_null() {
            self.method_data(*k)?;
        }
        self.work(SEND, values)
    }
    pub(super) fn prepare_entry(&self, values: &[Oop]) -> Result<Oop, Oop> {
        let [method, _, k, ..] = values else {
            return Err(self.invalid());
        };
        self.method_data(*method)?;
        if !k.is_null() {
            self.method_data(*k)?;
        }
        self.work(ENTER, values)
    }
    pub(super) fn prepare_answer(&self, k: Oop, value: Oop) -> Result<Oop, Oop> {
        if k.is_null() {
            return Ok(ptr::null_mut());
        }
        self.prepare_entry(&[k, k, ptr::null_mut(), value])
    }
    pub(super) fn pause_work(&self, work: Oop) -> Result<Oop, Oop> {
        if work.is_null() {
            return Err(self.invalid());
        }
        self.method_data(work)?;
        let mut drives = self.drives.borrow_mut();
        let Some(drive) = drives.last_mut() else {
            return Err(self.invalid());
        };
        // No native caller may remain waiting across suspension.
        if drive.synchronous || drive.native_depth != 0 {
            drop(drives);
            return Err(self.control_failure(b"continuationBarrier"));
        }
        if drive.paused.is_some() {
            return Err(self.invalid());
        }
        drive.paused = Some(work);
        Ok(ptr::null_mut())
    }
    pub(super) unsafe fn drive_work(
        &self,
        mut work: Oop,
        budget: usize,
        synchronous: bool,
    ) -> Result<Oop, Oop> {
        self.check()?;
        let index = {
            let mut drives = self.drives.borrow_mut();
            if !synchronous && !drives.is_empty() {
                drop(drives);
                return Err(self.control_failure(b"driveActive"));
            }
            let index = drives.len();
            drives.push(Drive {
                synchronous,
                native_depth: 0,
                paused: None,
            });
            index
        };
        let _drive = DriveScope { s: self, index };
        for _ in 0..budget {
            if work.is_null() {
                return Ok(work);
            }
            let scope = self.scope(work, ptr::null_mut(), &[]);
            let data = self.method_data(work)?;
            scope.entered(work, data.context, data.owner);
            let next = if data.continuation {
                let arguments = [ptr::null_mut()];
                let value =
                    unsafe { (data.entry)(self.root, data.context, work, arguments.as_ptr(), 1) };
                self.behavior(value)?;
                if abi::is_fault(value) {
                    return Err(abi::object_reference(value));
                }
                value
            } else {
                // An ordinary native work entry runs synchronously. Its answer
                // has no destination here and cannot be mistaken for control.
                unsafe { self.enter_method(work, work, &[], &scope) }?;
                ptr::null_mut()
            };
            let paused = self.drives.borrow()[index].paused;
            if let Some(pending) = paused {
                if !next.is_null() {
                    return Err(self.invalid());
                }
                return Ok(pending);
            }
            // No callbacks or collection occur between this handoff and the
            // next scope. A budget stop transfers rooting to the native caller.
            work = next;
        }
        if !work.is_null() {
            self.method_data(work)?;
        }
        Ok(work)
    }
    pub(super) unsafe fn enter_synchronously(
        &self,
        method: Oop,
        receiver: Oop,
        args: &[Oop],
    ) -> Result<Oop, Oop> {
        // A heap context, not a pointer to a native stack result slot. A retained
        // callback delivered after this boundary closes is rejected safely.
        let context = self.new_sequence(&[
            abi::size(1).unwrap(),
            abi::size(0).unwrap(),
            ptr::null_mut(),
        ])?;
        let kept = [context, method, receiver];
        let _roots = self.scope(self.root, ptr::null_mut(), &kept);
        let answer = self.new_method_index(OPERATIONS.len() + RESULT, context, self.root)?;
        let mut values = vec![method, receiver, answer];
        values.extend_from_slice(args);
        let work = self.prepare_entry(&values)?;
        let outcome = unsafe { self.drive_work(work, usize::MAX, true) };
        let ready = self.reference_at(context, 1)?;
        let value = self.reference_at(context, 2)?;
        // Closing an escaped answer continuation must not retain the completed
        // answer forever. These owner-local writes cannot call or collect.
        self.reference_put(context, 0, abi::size(0).unwrap())?;
        self.reference_put(context, 2, ptr::null_mut())?;
        outcome?;
        if abi::as_size(ready) != Some(1) {
            return Err(self.control_failure(b"continuationBarrier"));
        }
        if abi::is_fault(value) {
            Err(abi::object_reference(value))
        } else {
            Ok(value)
        }
    }
    fn selected_work(&self, values: &[Oop], method: Oop) -> Result<Oop, Oop> {
        let [receiver, selector, k, args @ ..] = values else {
            return Err(self.invalid());
        };
        if abi::is_fault(method) {
            return if k.is_null() {
                Err(abi::object_reference(method))
            } else {
                self.prepare_answer(*k, method)
            };
        }
        if !method.is_null() {
            let mut next = vec![method, *receiver, *k];
            next.extend_from_slice(args);
            return match self.prepare_entry(&next) {
                Ok(work) => Ok(work),
                Err(error) if !k.is_null() => self.prepare_answer(*k, abi::fault_reference(error)),
                Err(error) => Err(error),
            };
        }
        if *selector == self.dnu
            || (*selector == self.lookup && self.behavior(*receiver)? == self.meta)
        {
            return if k.is_null() {
                Err(self.unbound())
            } else {
                self.prepare_answer(*k, abi::fault_reference(self.unbound()))
            };
        }
        let mut next = vec![*receiver, self.dnu, *k, *selector];
        next.extend_from_slice(args);
        self.prepare_send(&next)
    }
}

unsafe fn state<'a>(root: Oop) -> &'a State {
    unsafe { &*root.cast::<*const State>().read() }
}
unsafe fn arguments<'a>(s: &State, a: *const Oop, n: usize) -> Result<&'a [Oop], Oop> {
    unsafe { abi::borrowed_slice(a, n) }.ok_or(s.invalid())
}
unsafe extern "C" fn send_work(root: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(root);
        s.check()?;
        if arguments(s, a, n)?.len() != 1 {
            return Err(s.invalid());
        }
        let mut values = s.sequence_values(c)?;
        if values.len() >= 2 {
            values[0] = abi::object_reference(values[0]);
            values[1] = abi::object_reference(values[1]);
        }
        let [receiver, selector, _, ..] = values.as_slice() else {
            return Err(s.invalid());
        };
        let _scope = s.scope(c, ptr::null_mut(), &values);
        match id_dispatch::selection(s, *receiver, *selector) {
            Ok(Selection::Bound(method)) => {
                s.selected_work(&values, method.unwrap_or(ptr::null_mut()))
            }
            Err(error) if !values[2].is_null() => {
                s.prepare_answer(values[2], abi::fault_reference(error))
            }
            Err(error) => Err(error),
            Ok(Selection::Lookup(behavior)) => {
                let mut pending = vec![abi::size(0).unwrap()];
                pending.extend_from_slice(&values);
                let k = s.work(LOOKED_UP, &pending)?;
                s.prepare_send(&[behavior, s.lookup, k, *selector])
            }
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn looked_up(root: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(root);
        s.check()?;
        let [_, method] = arguments(s, a, n)? else {
            return Err(s.invalid());
        };
        let values = s.sequence_values(c)?;
        let [used, original @ ..] = values.as_slice() else {
            return Err(s.invalid());
        };
        if abi::as_size(*used) != Some(0) {
            return Err(s.control_failure(b"continuationConsumed"));
        }
        let mut roots = values.clone();
        roots.push(*method);
        let _scope = s.scope(c, ptr::null_mut(), &roots);
        s.reference_put(c, 0, abi::size(1).unwrap())?;
        // A consumed lookup continuation retains its rejection state, not the
        // receiver/arguments/code of a call it can no longer resume.
        for index in 1..values.len() {
            s.reference_put(c, index, ptr::null_mut())?;
        }
        s.selected_work(original, *method)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn enter_work(root: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(root);
        s.check()?;
        if arguments(s, a, n)?.len() != 1 {
            return Err(s.invalid());
        }
        let values = s.sequence_values(c)?;
        let [method, receiver, k, args @ ..] = values.as_slice() else {
            return Err(s.invalid());
        };
        let scope = s.scope(*receiver, ptr::null_mut(), &values);
        let data = s.method_data(*method)?;
        scope.entered(*method, data.context, data.owner);
        if data.continuation {
            let mut passed = vec![*k];
            passed.extend_from_slice(args);
            let value = (data.entry)(
                s.root,
                data.context,
                *receiver,
                passed.as_ptr(),
                passed.len(),
            );
            s.behavior(value)?;
            if abi::is_fault(value) && !k.is_null() {
                s.prepare_answer(*k, value)
            } else if abi::is_fault(value) {
                Err(abi::object_reference(value))
            } else {
                Ok(value)
            }
        } else {
            let value = s
                .enter_method(*method, *receiver, args, &scope)
                .unwrap_or_else(abi::fault_reference);
            if abi::is_fault(value) && k.is_null() {
                Err(abi::object_reference(value))
            } else {
                s.prepare_answer(*k, value)
            }
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn result(root: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(root);
        s.check()?;
        let [_, value] = arguments(s, a, n)? else {
            return Err(s.invalid());
        };
        if abi::as_size(s.reference_at(c, 0)?) != Some(1)
            || abi::as_size(s.reference_at(c, 1)?) != Some(0)
        {
            return Err(s.control_failure(b"continuationConsumed"));
        }
        s.reference_put(c, 2, *value)?;
        s.reference_put(c, 1, abi::size(1).unwrap())?;
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}

/// Public v3 native primitive. This does not discover another owner's services
/// or inspect a caller's state. Its contract is Id's fixed transfer mechanism.
pub(super) unsafe extern "C" fn control(root: Oop, operation: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(root);
        s.check()?;
        let args = arguments(s, a, n)?;
        let _roots = s.scope(root, operation, args);
        if operation == s.selector(b"prepareSend") {
            s.prepare_send(args)
        } else if operation == s.selector(b"prepareEntry") {
            s.prepare_entry(args)
        } else if operation == s.selector(b"answerTo:with:") {
            let [k, value] = args else {
                return Err(s.invalid());
            };
            s.prepare_answer(*k, *value)
        } else if operation == s.selector(b"drive:budget:") {
            let [work, budget] = args else {
                return Err(s.invalid());
            };
            s.drive_work(*work, abi::as_size(*budget).ok_or(s.invalid())?, false)
        } else if operation == s.selector(b"pause:") {
            let [work] = args else {
                return Err(s.invalid());
            };
            s.pause_work(*work)
        } else {
            Err(s.invalid())
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
