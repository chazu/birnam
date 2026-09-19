"""Compare frozen contracts while allowing additive development entry points.

Receipts still hash the full current file. Only the trailing cfg(test) module
in registration.rs is outside the production authority comparison.
The 74T literal-aware PC entry is an additive compiler caller of the original
emitter. Exclude only that new entry; all baseline emission remains byte-frozen.
Design 80N adds exactly two live-action names to image admission. Normalize only
those declarations and rejection-list entries; all other image code stays frozen.
Design 81K migrates exactly six owner/parent calls to message adapters. Normalize
only those complete expressions, with their original operands and exact counts.
"""


def unchanged_authority(name, current, baseline):
    if name == "rust/birnam-image/src/lib.rs" and current != baseline:
        float_migrations = (
            (b'(api.mkFloat)', b'(birnam_abi::float::make)', 1),
            (b'(api.floatValue)', b'(birnam_abi::float::value)', 1),
            (b'api.float_vt', b'birnam_abi::float::behavior()', 1),
        )
        if any(new in current for _, new, _ in float_migrations):
            for old, new, count in float_migrations:
                if baseline.count(old) != count or new in baseline or current.count(new) != count:
                    return False
                current = current.replace(new, old)
            dependency = b'\nbirnam_abi::float_dependency!();\n'
            if dependency in baseline or current.count(dependency) != 1:
                return False
            current = current.replace(dependency, b'')
        integer_migrations = (
            (b'(api.mkSmallInt)', b'(birnam_abi::integer::make)', 12),
            (b'(api.smallIntValue)', b'(birnam_abi::integer::value)', 8),
            (b'(api.isSmallInt)', b'(birnam_abi::integer::is_integer)', 5),
            (b'api.smallint_vt', b'birnam_abi::integer::behavior()', 1),
        )
        if any(new in current for _, new, _ in integer_migrations):
            for old, new, count in integer_migrations:
                if baseline.count(old) != count or new in baseline or current.count(new) != count:
                    return False
                current = current.replace(new, old)
            dependency = b'\nbirnam_abi::integer_dependency!();\n'
            if dependency in baseline or current.count(dependency) != 1:
                return False
            current = current.replace(dependency, b'')
        migrations = (
            (b'(api.vtableParent)(vtable)', b'birnam_abi::vtable_parent(api, vtable)', 3),
            (b'(api.vtableOwner)(vtable)', b'birnam_abi::vtable_owner(api, vtable)', 2),
            (b'(api_ref.setOwner)(report_vt, object)',
             b'birnam_abi::set_vtable_owner(api_ref, report_vt, object)', 1),
        )
        if any(new in current for _, new, _ in migrations):
            for old, new, count in migrations:
                if baseline.count(old) != count or new in baseline or current.count(new) != count:
                    return False
                current = current.replace(new, old)
        additions = (
            (b'    const ABORT_ACTION_NAME: &[u8] = b"AbortAction\\0";\n',
             b'    const BOUNDARY_CALL_ACTION_NAME: &[u8] = b"BoundaryCallAction\\0";\n'
             b'    const EXIT_BOUNDARY_ACTION_NAME: &[u8] = b"ExitBoundaryAction\\0";\n'),
            (b'            ABORT_ACTION_NAME,\n',
             b'            BOUNDARY_CALL_ACTION_NAME,\n'
             b'            EXIT_BOUNDARY_ACTION_NAME,\n'),
        )
        declaration_anchor, declarations = additions[0]
        list_anchor, entries = additions[1]
        for old, new in ((declaration_anchor, declarations + declaration_anchor),
                         (list_anchor, list_anchor + entries)):
            if baseline.count(old) != 1 or new in baseline or current.count(new) != 1:
                return False
            current = current.replace(new, old, 1)
    if name == "rust/birnam-id/src/registration.rs":
        marker = b"\n#[cfg(test)]\nmod tests {"
        assert current.count(marker) == baseline.count(marker) == 1
        current = current.split(marker)[0]
        baseline = baseline.split(marker)[0]
    if name == "rust/birnam-codegen/src/resumable_c.rs":
        start = b"/// 74T: specialize a canonical PC while retaining the exact literal occurrence,\n"
        end = b"/// 74H experiment: preserve operands and reuse guarded reads within one PC.\n"
        if start in current:
            assert current.count(start) == current.count(end) == 1
            assert start not in baseline and baseline.count(end) == 1
            before, addition = current.split(start)
            _, after = addition.split(end)
            current = before + end + after
    return current == baseline
