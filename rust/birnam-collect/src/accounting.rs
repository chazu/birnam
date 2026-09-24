//! Accumulated allocation work, distinct from live storage or a reservation.
//! The scalar belongs to GC; Id supplies its own storage geometry by message.
use super::*;

pub(super) fn add(current: usize, bytes: usize) -> usize {
    current.checked_add(bytes).unwrap_or_else(|| {
        // Preserve the established fatal policy for a committed charge. Preview
        // refuses overflow instead; it must not mutate the allocation budget.
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
    pub(super) fn fits(&self, bytes: usize) -> bool {
        self.charged
            .get()
            .checked_add(bytes)
            .is_some_and(|total| total < self.budget.get())
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

pub(super) unsafe extern "C" fn preview(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let m = memory(c)?;
        let Some([bytes]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let bytes = bytes.as_word().ok_or_else(invalid)?;
        Ok(Operand::word(usize::from(
            m.phase.get() == Phase::Idle && m.fits(bytes),
        )))
    })())
}
