//! Reusable implementation algorithms. Storage and root reporting stay with callers.
use crate::Oop;
use std::{cell::RefCell, sync::atomic::{AtomicBool, Ordering}};

pub fn with_roots<T>(storage: &RefCell<Vec<Oop>>, values: &[Oop], body: impl FnOnce() -> T) -> T {
    struct Restore<'a>(&'a RefCell<Vec<Oop>>, usize);
    impl Drop for Restore<'_> {
        fn drop(&mut self) { self.0.borrow_mut().truncate(self.1); }
    }
    let restore = Restore(storage, storage.borrow().len());
    storage.borrow_mut().extend_from_slice(values);
    let result = body();
    drop(restore);
    result
}

pub struct Initializing<'a>(&'a AtomicBool);
impl<'a> Initializing<'a> {
    pub fn enter(busy: &'a AtomicBool) -> Option<Self> {
        if busy.swap(true, Ordering::AcqRel) { None } else { Some(Self(busy)) }
    }
}
impl Drop for Initializing<'_> {
    fn drop(&mut self) { self.0.store(false, Ordering::Release); }
}

/// Install the caller's root enumeration Method and register its existing owner.
/// The caller keeps that owner retained throughout this sequence of live sends.
pub fn register_roots(
    root: Oop, image: Oop, context: Oop, roots: Oop, owner: Oop, selector: Oop,
    send: impl Fn(Oop, &[u8], &[Oop]) -> Result<Oop, Oop>,
) -> Result<(), Oop> {
    let behavior = send(root, b"behaviorOf:", &[owner])?;
    let private = send(behavior, b"delegated", &[])?;
    send(root, b"reassociateBehaviorOf:with:", &[owner, private])?;
    let method = send(image, b"methodAt:", &[selector])?;
    let method = send(method, b"withContext:", &[context])?;
    send(private, b"addMethod", &[selector, method])?;
    send(roots, b"register:", &[owner])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_roots_restore_after_errors_and_panics() {
        let a=crate::size(1).unwrap(); let b=crate::size(2).unwrap();
        let roots=RefCell::new(vec![a]);
        with_roots(&roots,&[b],|| {
            assert_eq!(*roots.borrow(),vec![a,b]);
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_roots(&roots,&[a],|| { roots.borrow_mut().push(b); panic!("callback"); });
            }));
            assert!(result.is_err());
            assert_eq!(*roots.borrow(),vec![a,b]);
            let result:Result<(),()> = with_roots(&roots,&[a],||Err(()));
            assert!(result.is_err());
            assert_eq!(*roots.borrow(),vec![a,b]);
        });
        assert_eq!(*roots.borrow(),vec![a]);
    }
    #[test]
    fn failed_reentry_does_not_clear_the_active_initializer() {
        let busy=AtomicBool::new(false);
        let outer=Initializing::enter(&busy).unwrap();
        assert!(Initializing::enter(&busy).is_none());
        assert!(busy.load(Ordering::Acquire));
        drop(outer);
        assert!(!busy.load(Ordering::Acquire));
        assert!(Initializing::enter(&busy).is_some());
    }
}
