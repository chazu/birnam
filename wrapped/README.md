# `wrapped/` — first-party wrapped C libraries

Small, well-chosen C libraries that birnam wraps over its FFI surface — vendored
source plus a declarative `.birnam` wrapper. The point: **authoring a wrapped
library needs no additional language toolchain.** You compile the C with the system `cc` and
describe the calls as birnam facts.

## Layout

```
wrapped/
  <lib>/
    <lib>.c, <lib>.h     vendored upstream source (keep LICENSE/header intact)
    <lib>.birnam         the wrapper: `Module load:` + `wrap:as:signature:`
    lib<lib>.<ext>       built dylib (git-ignored; produced by `birnam wrap`)
```

## Declaring a wrapped lib

Add a `[[wrapped]]` entry to `birnam.toml` (parsed by the Rust-host project layer):

```toml
[[wrapped]]
name    = "linenoise"                          # selects `birnam wrap linenoise`
sources = ["wrapped/linenoise/linenoise.c"]    # compiled together
wrapper = "wrapped/linenoise/linenoise.birnam" # the Module load:/wrap: decls
cflags  = ["-O2"]                              # passed to cc verbatim
# output = "..."   # optional; defaults to lib<name>.<ext> beside the wrapper
```

Relative paths resolve against the `birnam.toml` directory.

## Building

```
birnam wrap            # build every [[wrapped]] entry
birnam wrap linenoise  # build just one
```

`birnam wrap` runs `cc <cflags> -dynamiclib|-shared -fPIC <sources…> -o <output>`
(`-dynamiclib` on macOS, `-shared` on ELF). Then file in the
wrapper to use it:

```
birnam> :in wrapped/linenoise/linenoise.birnam
```

File-in is atomic — the wraps resolve at commit, so call them in a later input,
not inside the wrapper unit.

## Writing the wrapper

The wrapper is ordinary birnam: load the dylib, then declare one `wrap:` per C
function. The signature string is a sequence of sigchars, args then `)` then
return (see `docs/2d-ffi.md`): `Z` cstring, `p` pointer, `i` int, `v` void, etc.
**Arg order is `wrap: #birnamSelector as: "c_symbol" signature: "sig"`** — the
selector symbol first, the C symbol name second.

Functions that return a malloc'd buffer (e.g. `linenoise()`) leak on the `Z`
return path (birnam copies the string but cannot free the C buffer); use the
raw-pointer idiom (`…Ptr:` returning `p`, then `peekString:` + `free:`) when the
leak matters.
