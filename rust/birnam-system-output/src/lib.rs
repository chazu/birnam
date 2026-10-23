//! System output transfers bytes as Oops; OS writes remain private here.
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::{cell::Cell, ptr};
use std::{
    collections::HashMap,
    io::{self, Write},
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
struct State {
    id: Protocol,
    ns: Oop,
    string_vt: Oop,
    nil: Oop,
    names: HashMap<&'static [u8], Oop>,
    writer: Oop,
    ready: Cell<bool>,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
struct Constructing;
impl Drop for Constructing {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}
fn invalid() -> Oop {
    let p = API.load(Ordering::Acquire);
    if p.is_null() {
        std::process::abort();
    }
    unsafe { abi::object_reference((*p).invalid) }
}
unsafe fn state<'a>(c: Oop) -> Result<&'a State, Oop> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        return Err(invalid());
    }
    let s = unsafe { &*p };
    if c == s.ns { Ok(s) } else { Err(invalid()) }
}
impl State {
    unsafe fn send(&self, r: Oop, name: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[name], a) }
    }
    unsafe fn emit(&self, value: Oop, receiver: Oop, mode: usize) -> Result<Option<Oop>, Oop> {
        unsafe {
            if self.send(self.id.root, b"behaviorOf:", &[value])? != self.string_vt {
                return Ok(None);
            }
            let bytes = self.send(value, b"bytes", &[])?;
            if bytes.is_null() {
                return Ok(None);
            }
            self.send(
                bytes,
                b"apply:to:with:",
                &[self.writer, receiver, abi::size(mode).unwrap()],
            )
            .map(Some)
        }
    }
}
fn write_system_output_to<W: Write>(writer: &mut W, bytes: &[u8], newline: bool) -> io::Result<()> {
    writer.write_all(bytes)?;
    if newline {
        writer.write_all(b"\n")?;
    }
    writer.flush()
}
unsafe extern "C" fn write_bytes(_: Oop, c: Oop, r: Oop, a: *const Oop, count: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let a = borrowed_slice(a, count).ok_or_else(invalid)?;
        let Some((mode, values)) = a.split_last() else {
            return Err(invalid());
        };
        let mode = abi::as_size(*mode).filter(|n| *n < 3).ok_or_else(invalid)?;
        let Some(bytes) = values
            .iter()
            .map(|&v| abi::as_size(v).and_then(|v| u8::try_from(v).ok()))
            .collect::<Option<Vec<_>>>()
        else {
            return Ok(s.nil);
        };
        // No language send occurs while holding an OS output lock. Preserve the
        // existing ignored-write-error and explicit-flush behavior.
        if mode == 2 {
            let text = String::from_utf8_lossy(&bytes);
            let _ = write_system_output_to(&mut io::stderr().lock(), text.as_bytes(), false);
        } else {
            let _ = write_system_output_to(&mut io::stdout().lock(), &bytes, mode == 1);
        }
        Ok(r)
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn output(c: Oop, r: Oop, a: *const Oop, count: usize, mode: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let Some([value]) = borrowed_slice(a, count) else {
            return Ok(s.nil);
        };
        if value.is_null() {
            return Err(invalid());
        }
        if let Some(result) = s.emit(*value, r, mode)? {
            return Ok(result);
        }
        if mode == 2 {
            return Ok(s.nil);
        }
        let rendered = s.send(*value, b"printString", &[])?;
        if rendered.is_null() {
            return Ok(s.nil);
        }
        Ok(s.emit(rendered, r, mode)?.unwrap_or(s.nil))
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn print(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { output(c, r, a, n, 0) }
}
unsafe extern "C" fn println(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { output(c, r, a, n, 1) }
}
unsafe extern "C" fn print_error(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { output(c, r, a, n, 2) }
}
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"at:",
    b"at:put:",
    b"Roots",
    b"StringVt",
    b"System",
    b"nil",
    b"behaviorOf:",
    b"sequence",
    b"retain:",
    b"release:",
    b"methodAt:",
    b"withContext:",
    b"addMethod",
    b"apply:to:with:",
    b"bytes",
    b"printString",
    b"print:",
    b"println:",
    b"printErrorText:",
    b"writeBytes"
];
const fn index(n: &[u8]) -> usize {
    let mut i = 0;
    while i < NAMES.len() {
        let c = NAMES[i];
        if c.len() == n.len() {
            let mut j = 0;
            while j < n.len() && c[j] == n[j] {
                j += 1;
            }
            if j == n.len() {
                return i;
            }
        }
        i += 1;
    }
    panic!("missing literal")
}
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: index(b"print:"),
        entry: Some(print),
    },
    MethodSpec {
        name: index(b"println:"),
        entry: Some(println),
    },
    MethodSpec {
        name: index(b"printErrorText:"),
        entry: Some(print_error),
    },
    MethodSpec {
        name: index(b"writeBytes"),
        entry: Some(write_bytes),
    },
];
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    count: usize,
) -> Oop {
    API.store(p.cast_mut(), Ordering::Release);
    let id = unsafe { *p };
    let Some([ns]) = (unsafe { borrowed_slice(a, count) }) else {
        return id.invalid;
    };
    let Some(symbols) = (unsafe { borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if n != NAMES.len() || ns.is_null() {
        return id.invalid;
    }
    let old = INSTANCE.load(Ordering::Acquire);
    if !old.is_null() {
        let s = unsafe { &*old };
        return if s.ready.get() && s.id.root == id.root && s.ns == *ns {
            ptr::null_mut()
        } else {
            id.invalid
        };
    }
    if BUSY.swap(true, Ordering::AcqRel) {
        return id.invalid;
    }
    let _constructing = Constructing;
    (|| unsafe {
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |n: &[u8]| -> Result<Oop, Oop> {
            let v = send(*ns, b"at:", &[names[n]])?;
            if v.is_null() { Err(invalid()) } else { Ok(v) }
        };
        let roots = named(b"Roots")?;
        let string_vt = named(b"StringVt")?;
        let nil = named(b"nil")?;
        let system = named(b"System")?;
        let vt = send(id.root, b"behaviorOf:", &[system])?;
        // Complete the private root Sequence before publishing any System method.
        let owner = send(
            id.root,
            b"sequence",
            &[*ns, roots, string_vt, nil, system, vt, ptr::null_mut()],
        )?;
        let token = send(roots, b"retain:", &[owner])?;
        let prepared = (|| {
            let method = send(image, b"methodAt:", &[names[b"writeBytes".as_slice()]])?;
            let helper = send(method, b"withContext:", &[*ns])?;
            send(owner, b"at:put:", &[abi::size(6).unwrap(), helper])?;
            Ok(helper)
        })();
        let writer = match prepared {
            Ok(helper) => helper,
            Err(error) => {
                send(roots, b"release:", &[token])?;
                return Err(error);
            }
        };
        let s = Box::into_raw(Box::new(State {
            id,
            ns: *ns,
            string_vt,
            nil,
            names: names.clone(),
            writer,
            ready: Cell::new(false),
        }));
        INSTANCE.store(s, Ordering::Release);
        let s = &*s;
        let result = (|| {
            for name in [b"print:".as_slice(), b"println:", b"printErrorText:"] {
                let method = s.send(image, b"methodAt:", &[s.names[name]])?;
                let method = s.send(method, b"withContext:", &[*ns])?;
                s.send(vt, b"addMethod", &[s.names[name], method])?;
            }
            s.ready.set(true);
            Ok(ptr::null_mut())
        })();
        // Publication can fail after a replacement addMethod has exposed the
        // Method. Its dependencies must remain live with the resident state.
        result
    })()
    .unwrap_or_else(abi::fault_reference)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v2: Module = Module {
    version: 2,
    byte_size: size_of::<Module>() as u32,
    literals: LITERALS.as_ptr(),
    literal_count: LITERALS.len(),
    methods: METHODS.as_ptr(),
    method_count: METHODS.len(),
    initialize: Some(initialize),
};
#[cfg(test)]
mod tests {
    use super::write_system_output_to;
    use std::io::{self, Write};

    struct FlushProbe {
        bytes: Vec<u8>,
        flushed: bool,
    }

    impl Write for FlushProbe {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushed = true;
            Ok(())
        }
    }

    #[test]
    fn system_output_flushes_a_newline_free_prompt() {
        let mut output = FlushProbe {
            bytes: Vec::new(),
            flushed: false,
        };

        write_system_output_to(&mut output, b"birnam> ", false).expect("prompt write");

        assert_eq!(output.bytes, b"birnam> ");
        assert!(
            output.flushed,
            "a prompt must be visible before stdin blocks"
        );
    }
}
