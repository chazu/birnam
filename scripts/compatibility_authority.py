"""Compare frozen contracts while allowing Rust registration test maintenance.

Receipts still hash the full current file. Only the trailing cfg(test) module
in registration.rs is outside the production authority comparison.
"""


def unchanged_authority(name, current, baseline):
    if name == "rust/birnam-id/src/registration.rs":
        marker = b"\n#[cfg(test)]\nmod tests {"
        assert current.count(marker) == baseline.count(marker) == 1
        current = current.split(marker)[0]
        baseline = baseline.split(marker)[0]
    return current == baseline
