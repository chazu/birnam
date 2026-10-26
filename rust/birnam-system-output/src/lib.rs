//! Native I/O uses ordinary Id sends and owner-local temporary roots.
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ptr,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
struct State {
    id: Protocol,
    ns: Oop,
    nil: Oop,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    ready: Cell<bool>,
    string_vt: Oop,
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
fn object(v: Oop) -> Result<Oop, Oop> {
    if v.is_null() { Err(invalid()) } else { Ok(v) }
}
unsafe fn state<'a>(c: Oop) -> Result<&'a State, Oop> {
    let p = INSTANCE.load(Ordering::Acquire);
    if p.is_null() {
        return Err(invalid());
    }
    let s = unsafe { &*p };
    if c == s.ns { Ok(s) } else { Err(invalid()) }
}
struct Scope<'a> {
    s: &'a State,
    start: usize,
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.scoped.borrow_mut().truncate(self.start);
    }
}
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[n], a) }
    }
    unsafe fn rooted<T>(
        &self,
        values: &[Oop],
        f: impl FnOnce() -> Result<T, Oop>,
    ) -> Result<T, Oop> {
        let scope = Scope {
            s: self,
            start: self.scoped.borrow().len(),
        };
        self.scoped.borrow_mut().extend_from_slice(values);
        let result = f();
        drop(scope);
        result
    }
    unsafe fn bytes(&self, value: Oop) -> Result<Option<Vec<u8>>, Oop> {
        unsafe {
            if self.send(self.id.root, b"behaviorOf:", &[value])? != self.string_vt {
                return Ok(None);
            }
            let bytes = self.send(value, b"bytes", &[])?;
            if bytes.is_null() {
                return Ok(None);
            }
            self.id
                .read_bytes(
                    bytes,
                    self.names[b"sizeWord".as_slice()],
                    self.names[b"atWord:".as_slice()],
                )
                .map(Some)
        }
    }
}
use std::io::{self, Write};
fn write_system_output_to<W: Write>(writer: &mut W, bytes: &[u8], newline: bool) -> io::Result<()> {
    writer.write_all(bytes)?;
    if newline {
        writer.write_all(b"\n")?;
    }
    writer.flush()
}
fn write_system_output(bytes: &[u8], newline: bool) {
    // No language send runs while the OS output lock or a borrowed String is held.
    let mut stdout = io::stdout().lock();
    let _ = write_system_output_to(&mut stdout, bytes, newline);
}
unsafe fn output(c: Oop, r: Oop, a: *const Oop, n: usize, newline: bool) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 1 {
            return Ok(s.nil);
        }
        let [value] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let value = object(*value)?;
        (|| {
            if let Some(bytes) = s.bytes(value)? {
                write_system_output(&bytes, newline);
                return Ok(r);
            }
            let rendered = s.send(value, b"printString", &[])?;
            let Some(rendered) = (!rendered.is_null()).then_some(rendered) else {
                return Ok(s.nil);
            };
            s.rooted(&[rendered], || {
                let Some(bytes) = s.bytes(rendered)? else {
                    return Ok(s.nil);
                };
                write_system_output(&bytes, newline);
                Ok(r)
            })
        })()
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn print(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { output(c, r, a, n, false) }
}
unsafe extern "C" fn println(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { output(c, r, a, n, true) }
}
// Generic UTF-8 diagnostic output; message composition belongs to callers.
unsafe extern "C" fn print_error_text(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let Some([value]) = borrowed_slice(a, n) else {
            return Ok(s.nil);
        };
        let value = object(*value)?;
        (|| {
            let Some(bytes) = s.bytes(value)? else {
                return Ok(s.nil);
            };
            let text = String::from_utf8_lossy(&bytes);
            let mut stderr = io::stderr().lock();
            let _ = write_system_output_to(&mut stderr, text.as_bytes(), false);
            Ok(r)
        })()
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn roots_do(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [visitor] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let values = s.scoped.borrow().clone();
        for value in values {
            s.send(*visitor, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"Bytes",
    b"Error",
    b"Roots",
    b"String",
    b"StringVt",
    b"System",
    b"Vector",
    b"add:",
    b"addMethod",
    b"at:",
    b"at:put:",
    b"atWord:",
    b"behaviorOf:",
    b"bytes",
    b"delegated",
    b"fromBytes:",
    b"ioError",
    b"kind:message:",
    b"methodAt:",
    b"new",
    b"nil",
    b"print:",
    b"printErrorText:",
    b"printString",
    b"println:",
    b"reassociateBehaviorOf:with:",
    b"register:",
    b"release:",
    b"retain:",
    b"rootsDo:",
    b"sequence",
    b"sizeWord",
    b"visit:",
    b"withContext:"
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
        entry: Some(print_error_text),
    },
    MethodSpec {
        name: index(b"rootsDo:"),
        entry: Some(roots_do),
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
        return if s.ready.get() && s.ns == *ns && s.id.root == id.root {
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
        let named = |n: &[u8]| object(send(*ns, b"at:", &[names[n]])?);
        let roots = named(b"Roots")?;
        let nil = named(b"nil")?;
        let system = named(b"System")?;
        let vt = send(id.root, b"behaviorOf:", &[system])?;
        let string_vt = named(b"StringVt")?;
        let owner = send(
            id.root,
            b"sequence",
            &[*ns, roots, nil, system, vt, string_vt],
        )?;
        let token = send(roots, b"retain:", &[owner])?;
        let s = Box::into_raw(Box::new(State {
            id,
            ns: *ns,
            nil,
            names: names.clone(),
            scoped: RefCell::new(Vec::new()),
            ready: Cell::new(false),
            string_vt,
        }));
        INSTANCE.store(s, Ordering::Release);
        let s = &*s;
        let owner_type = send(id.root, b"behaviorOf:", &[owner])?;
        let private = send(owner_type, b"delegated", &[])?;
        send(id.root, b"reassociateBehaviorOf:with:", &[owner, private])?;
        let method = send(image, b"methodAt:", &[names[b"rootsDo:".as_slice()]])?;
        let method = send(method, b"withContext:", &[*ns])?;
        send(
            private,
            b"addMethod",
            &[names[b"rootsDo:".as_slice()], method],
        )?;
        send(roots, b"register:", &[owner])?;
        for name in [
            b"print:".as_slice(),
            b"println:".as_slice(),
            b"printErrorText:".as_slice(),
        ] {
            let method = send(image, b"methodAt:", &[names[name]])?;
            let method = send(method, b"withContext:", &[*ns])?;
            send(vt, b"addMethod", &[names[name], method])?;
        }
        send(roots, b"release:", &[token])?;
        s.ready.set(true);
        Ok(ptr::null_mut())
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
