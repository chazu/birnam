//! Accumulated allocation work, distinct from live storage or a reservation.
//! The scalar belongs to GC; Id supplies its own storage geometry by message.
use super::*;

pub(super) fn add(current: usize, bytes: usize) -> usize {
    current.checked_add(bytes).unwrap_or_else(|| {
        // A committed allocation charge must not wrap or silently saturate.
        eprintln!("birnam-id invariant failed: GC allocation byte counter overflow");
        std::process::abort()
    })
}

impl Memory {
    pub(super) unsafe fn allocation_charge(&self, payload: usize) -> Result<usize, Outcome> {
        unsafe {
            self.id.send(
                self.id.receiver,
                self.allocation_charge_selector,
                &[Operand::word(payload)],
            )?
            .as_word()
            .filter(|n| *n != 0)
            .ok_or_else(invalid)
        }
    }
    pub(super) fn below_threshold(&self) -> bool {
        self.charged.get() < self.budget.get()
    }
}

pub(super) unsafe extern "C" fn read(
    c: *mut c_void,
    _: Oop,
    _: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let m = memory(c)?;
        if n != 0 {
            return Err(invalid());
        }
        Ok(Operand::word(m.charged.get()))
    })())
}

/// Account committed owner storage, without collecting or reserving capacity.
/// The owner publishes and roots its object before this replaceable send.
pub(super) unsafe extern "C" fn external(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let m = memory(c)?;
        if !matches!(m.phase.get(), Phase::Inactive | Phase::Idle) {
            return Err(invalid());
        }
        let [bytes] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let bytes = bytes.as_word().ok_or_else(invalid)?;
        m.charged.set(add(m.charged.get(), bytes));
        Ok(Operand::unit())
    })())
}
