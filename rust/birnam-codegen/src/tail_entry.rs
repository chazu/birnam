//! Loop-entry transfer plans over validated, immutable V2 entry layouts.
//! The runtime resolves the actual tail target before using one of these plans.
use core::ops::Range;

pub const MAX_SLOTS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    slot_count: usize,
    receiver: usize,
    arguments: Range<usize>,
    implementation: Option<usize>,
    captures: Box<[usize]>,
    clear: Box<[Range<usize>]>,
}

impl Plan {
    pub fn derive(
        slot_count: usize,
        receiver: usize,
        argument_start: usize,
        argument_count: usize,
        implementation: Option<usize>,
        captures: &[usize],
    ) -> Option<Self> {
        if slot_count == 0 || slot_count > MAX_SLOTS {
            return None;
        }
        let argument_end = argument_start.checked_add(argument_count)?;
        if argument_end > slot_count {
            return None;
        }
        let mut written = [false; MAX_SLOTS];
        let mut claim = |slot: usize| {
            let value = written.get_mut(slot).filter(|_| slot < slot_count)?;
            if core::mem::replace(value, true) {
                return None;
            }
            Some(())
        };
        claim(receiver)?;
        for slot in argument_start..argument_end {
            claim(slot)?;
        }
        if let Some(slot) = implementation {
            claim(slot)?;
        }
        for &slot in captures {
            claim(slot)?;
        }
        let mut clear = Vec::new();
        let mut slot = 0;
        while slot < slot_count {
            if written[slot] {
                slot += 1;
                continue;
            }
            let start = slot;
            while slot < slot_count && !written[slot] {
                slot += 1;
            }
            clear.push(start..slot);
        }
        Some(Self {
            slot_count,
            receiver,
            arguments: argument_start..argument_end,
            implementation,
            captures: captures.into(),
            clear: clear.into_boxed_slice(),
        })
    }

    pub fn slot_count(&self) -> usize {
        self.slot_count
    }
    /// All words are overwritten exactly once. Inputs must not alias `words`.
    /// Capture reads are supplied by the runtime after checking the actual
    /// receiver payload; the compiler gives no authority to opaque callbacks.
    pub fn write(
        &self,
        words: &mut [usize],
        receiver: usize,
        arguments: &[usize],
        implementation: usize,
        mut capture: impl FnMut(usize) -> usize,
        nil: usize,
    ) {
        assert_eq!(words.len(), self.slot_count);
        for range in &self.clear {
            words[range.clone()].fill(nil);
        }
        words[self.receiver] = receiver;
        let count = arguments.len().min(self.arguments.len());
        let start = self.arguments.start;
        words[start..start + count].copy_from_slice(&arguments[..count]);
        words[start + count..self.arguments.end].fill(nil);
        if let Some(slot) = self.implementation {
            words[slot] = implementation;
        }
        for (index, &slot) in self.captures.iter().enumerate() {
            words[slot] = capture(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entry_transfer_preserves_sparse_bindings_missing_arguments_and_captures() {
        for count in 0..8 {
            let plan = Plan::derive(14, 9, 1, 4, Some(12), &[7, 0, 10]).unwrap();
            let args: Vec<_> = (100..100 + count).collect();
            let captures = [50, 60, 70];
            let mut actual = [usize::MAX; 14];
            plan.write(&mut actual, 99, &args, 88, |i| captures[i], 42);
            let mut expected = [42; 14];
            expected[9] = 99;
            for i in 0..4 {
                expected[1 + i] = args.get(i).copied().unwrap_or(42);
            }
            expected[12] = 88;
            for (i, slot) in [7, 0, 10].into_iter().enumerate() {
                expected[slot] = captures[i];
            }
            assert_eq!(actual, expected);
        }
        assert!(Plan::derive(129, 0, 1, 0, None, &[]).is_none());
        assert!(Plan::derive(3, 0, usize::MAX, 1, None, &[]).is_none());
        assert!(Plan::derive(3, 0, 0, 1, None, &[]).is_none());
        assert!(Plan::derive(3, 0, 1, 1, Some(1), &[]).is_none());
        assert!(Plan::derive(3, 0, 1, 1, None, &[2, 2]).is_none());
        assert!(Plan::derive(3, 0, 1, 1, None, &[3]).is_none());
    }
}
