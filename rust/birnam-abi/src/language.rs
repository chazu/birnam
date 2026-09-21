//! Checked language messages, supplied by the completed Runtime API.
use crate::Oop;
pub type Send = unsafe extern "C" fn(Oop, Oop, *const Oop, usize) -> Oop;
