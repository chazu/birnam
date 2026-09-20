use super::*;
#[test]
fn independent_owners_start_at_zero_and_replace_every_bit() {
    unsafe {
        let a = create();
        let b = create();
        assert!(!a.is_null() && !b.is_null() && a != b);
        assert_eq!(read(a), 0);
        assert_eq!(read(b), 0);
        for value in [1, 17, usize::MAX, 0] {
            replace(a, value);
            assert_eq!(read(a), value);
            assert_eq!(read(b), 0);
        }
        destroy(a);
        assert_eq!(charge(b, 19), 1);
        assert_eq!(read(b), 19);
        destroy(b);
    }
}
#[test]
fn checked_charge_preserves_the_total_on_overflow() {
    unsafe {
        let owner = create();
        replace(owner, usize::MAX - 7);
        assert_eq!(charge(owner, 7), 1);
        assert_eq!(read(owner), usize::MAX);
        assert_eq!(charge(owner, 0), 1);
        assert_eq!(read(owner), usize::MAX);
        assert_eq!(charge(owner, 1), 0);
        assert_eq!(read(owner), usize::MAX);
        replace(owner, 0);
        assert_eq!(charge(owner, usize::MAX), 1);
        destroy(owner);
    }
}
#[test]
fn preview_never_charges_and_refusal_preserves_output() {
    unsafe {
        let owner = create();
        replace(owner, usize::MAX - 7);
        let mut out = 19;
        assert_eq!(preview(owner, 7, &raw mut out), 1);
        assert_eq!(out, usize::MAX);
        assert_eq!(read(owner), usize::MAX - 7);
        out = 23;
        assert_eq!(preview(owner, 8, &raw mut out), 0);
        assert_eq!(out, 23);
        assert_eq!(read(owner), usize::MAX - 7);
        assert_eq!(preview(owner, 0, &raw mut out), 1);
        assert_eq!(out, usize::MAX - 7);
        destroy(owner);
    }
}
