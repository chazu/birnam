use super::*;

unsafe extern "C" fn first(_: *mut c_void, _: *mut c_void, _: *const birnam_abi::id_protocol::Operand, _: usize) -> birnam_abi::id_protocol::Outcome { panic!("cache must not invoke entries") }
unsafe extern "C" fn second(context: *mut c_void, receiver: *mut c_void, args: *const birnam_abi::id_protocol::Operand, count: usize) -> birnam_abi::id_protocol::Outcome { unsafe { first(context, receiver, args, count) } }
fn address(method: birnam_abi::id_protocol::Method) -> NativeEntry { NativeEntry::new(method, core::ptr::without_provenance_mut(83)) }

#[test]
fn all_slots_match_exact_keys_and_clear_without_owning_methods() {
    unsafe {
        let context = create();
        assert!(!context.is_null() && lookup(context, u64::MAX, 0, 0).method.is_none());
        for index in 0..SLOTS {
            let selector = index << 4;
            assert_eq!(slot(selector), index);
            assert!(lookup(context, 17, 19, selector).method.is_none());
            insert(context, 17, 19, selector, address(first));
        }
        for index in 0..SLOTS {
            let selector = index << 4;
            assert_eq!(lookup(context, 17, 19, selector), address(first));
            assert!(lookup(context, 18, 19, selector).method.is_none());
            assert!(lookup(context, 17, 20, selector).method.is_none());
            // Bit 16 leaves the six hash bits unchanged but changes the key.
            let collision = selector | (1 << 16);
            assert_eq!(slot(collision), index);
            assert!(lookup(context, 17, 19, collision).method.is_none());
            assert_eq!(lookup(context, 17, 19, selector), address(first));
            insert(context, u64::MAX, usize::MAX, collision, address(second));
            assert!(lookup(context, 17, 19, selector).method.is_none());
            assert_eq!(lookup(context, u64::MAX, usize::MAX, collision), address(second));
        }
        clear(context);
        for index in 0..SLOTS {
            assert!(lookup(context, u64::MAX, usize::MAX, (index << 4) | (1 << 16)).method.is_none());
        }
        assert!(lookup(context, u64::MAX, 0, 0).method.is_none());
        // Clear does not prohibit the exact sentinel keys in a subsequent entry.
        insert(context, u64::MAX, 0, 0, address(first));
        assert_eq!(lookup(context, u64::MAX, 0, 0), address(first));
        clear(context);clear(context);
        assert!(lookup(context, u64::MAX, 0, 0).method.is_none());
        destroy(context);
    }
}

#[test]
fn moved_owners_and_destruction_do_not_share_contents() {
    unsafe {
        let mut owners = vec![create(), create()];
        assert!(owners[0] != owners[1]);
        insert(owners[0], 1, 2, 3, address(first));
        insert(owners[1], 1, 2, 3, address(second));
        let moved = owners.remove(0);
        assert_eq!(lookup(moved, 1, 2, 3), address(first));
        clear(moved);destroy(moved);
        assert_eq!(lookup(owners[0], 1, 2, 3), address(second));
        destroy(owners[0]);
    }
}
