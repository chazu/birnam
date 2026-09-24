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
            self.send(
                self.id.receiver,
                b"allocationChargeFor:",
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
