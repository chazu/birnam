//! One owner for an Id registry whose address survives upper-layer adoption.
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

pub(crate) struct Owned<T>(Box<UnsafeCell<T>>);
impl<T> Owned<T> {
    pub(crate) fn new(value: T) -> Self {
        Self(Box::new(UnsafeCell::new(value)))
    }
    /// Raw native entry contexts may retain this address while the owner lives.
    /// The caller must serialize access and hold no registry reference across
    /// native calls. UnsafeCell keeps those accesses in the same aliasing domain.
    pub(crate) fn pointer(&self) -> *mut T {
        self.0.get()
    }
}
impl<T> Deref for Owned<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.pointer() }
    }
}
impl<T> DerefMut for Owned<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.pointer() }
    }
}
