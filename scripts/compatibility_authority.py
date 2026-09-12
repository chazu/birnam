"""Compare frozen contracts while allowing additive development entry points.

Receipts still hash the full current file. Only the trailing cfg(test) module
in registration.rs is outside the production authority comparison.
The 74T literal-aware PC entry is an additive compiler caller of the original
emitter. Exclude only that new entry; all baseline emission remains byte-frozen.
"""


def unchanged_authority(name, current, baseline):
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
