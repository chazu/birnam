//! Environment lookup uses Oop messages and completed owner-local dependencies.
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::{cell::Cell, ptr};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
struct State {
    id: Protocol,
    ns: Oop,
    string: Oop,
    string_vt: Oop,
    read_env: Oop,
    nil: Oop,
    names: HashMap<&'static [u8], Oop>,
    finish_error: Oop,
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
    unsafe fn make_string(&self, bytes: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let values: Vec<_> = bytes
                .iter()
                .map(|&b| abi::size(b as usize).unwrap())
                .collect();
            let source = self.send(self.id.root, b"sequence", &values)?;
            self.send(self.string, b"fromBytes:", &[source])
        }
    }
    unsafe fn error(&self, text: &[u8]) -> Result<Oop, Oop> {
        unsafe {
            let message = self.make_string(text)?;
            if message.is_null() {
                return Err(invalid());
            }
            // The diagnostic is the helper's receiver, so Id protects it through
            // lazy Error lookup and construction, with no root-token cleanup send.
            self.send(self.finish_error, b"apply", &[message])
        }
    }
}
unsafe extern "C" fn finish_error(_: Oop, c: Oop, message: Oop, _: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        if n != 0 || message.is_null() {
            return Err(invalid());
        }
        let factory = s.send(s.ns, b"at:", &[s.names[b"Error".as_slice()]])?;
        if factory.is_null() {
            return Ok(s.nil);
        }
        s.send(
            factory,
            b"kind:message:",
            &[s.names[b"ioError".as_slice()], message],
        )
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn get_env(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let Some([name]) = borrowed_slice(a, n) else {
            return Ok(s.nil);
        };
        if name.is_null() {
            return Err(invalid());
        }
        if s.send(s.id.root, b"behaviorOf:", &[*name])? != s.string_vt {
            return Ok(s.nil);
        }
        let bytes = s.send(*name, b"bytes", &[])?;
        if bytes.is_null() {
            return Ok(s.nil);
        }
        s.send(bytes, b"apply:to:with:", &[s.read_env, r])
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn read_env(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let a = borrowed_slice(a, n).ok_or_else(invalid)?;
        let Some(bytes) = a
            .iter()
            .map(|&v| abi::as_size(v).and_then(|v| u8::try_from(v).ok()))
            .collect::<Option<Vec<_>>>()
        else {
            return Ok(s.nil);
        };
        let Ok(name) = std::str::from_utf8(&bytes) else {
            return s.error(b"could not read environment");
        };
        match std::env::var(name) {
            Ok(value) => s.make_string(value.as_bytes()),
            Err(std::env::VarError::NotPresent) => Ok(s.nil),
            Err(std::env::VarError::NotUnicode(_)) => s.error(b"could not read environment"),
        }
    })()
    .unwrap_or_else(abi::fault_reference)
}
macro_rules! literals{($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"at:",
    b"at:put:",
    b"Roots",
    b"String",
    b"StringVt",
    b"System",
    b"nil",
    b"Error",
    b"ioError",
    b"behaviorOf:",
    b"fromBytes:",
    b"sequence",
    b"kind:message:",
    b"retain:",
    b"release:",
    b"methodAt:",
    b"withContext:",
    b"addMethod",
    b"apply",
    b"apply:to:with:",
    b"bytes",
    b"printString",
    b"getEnv:",
    b"readEnvironmentBytes",
    b"finishError"
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
        name: index(b"readEnvironmentBytes"),
        entry: Some(read_env),
    },
    MethodSpec {
        name: index(b"getEnv:"),
        entry: Some(get_env),
    },
    MethodSpec {
        name: index(b"finishError"),
        entry: Some(finish_error),
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
        let string = named(b"String")?;
        let string_vt = named(b"StringVt")?;
        let nil = named(b"nil")?;
        let system = named(b"System")?;
        let vt = send(id.root, b"behaviorOf:", &[system])?;
        let owner = send(
            id.root,
            b"sequence",
            &[
                *ns,
                roots,
                string,
                nil,
                system,
                vt,
                string_vt,
                ptr::null_mut(),
                ptr::null_mut(),
            ],
        )?;
        let token = send(roots, b"retain:", &[owner])?;
        let prepared = (|| {
            let mut helpers = [ptr::null_mut(); 2];
            for (i, name) in [b"finishError".as_slice(), b"readEnvironmentBytes"]
                .into_iter()
                .enumerate()
            {
                let method = send(image, b"methodAt:", &[names[name]])?;
                helpers[i] = send(method, b"withContext:", &[*ns])?;
                send(owner, b"at:put:", &[abi::size(i + 7).unwrap(), helpers[i]])?;
            }
            Ok(helpers)
        })();
        let [finish_error, read_env] = match prepared {
            Ok(helpers) => helpers,
            Err(error) => {
                send(roots, b"release:", &[token])?;
                return Err(error);
            }
        };
        let s = Box::into_raw(Box::new(State {
            id,
            ns: *ns,
            string,
            string_vt,
            read_env,
            nil,
            names: names.clone(),
            finish_error,
            ready: Cell::new(false),
        }));
        INSTANCE.store(s, Ordering::Release);
        let s = &*s;
        let result = (|| {
            let method = s.send(image, b"methodAt:", &[s.names[b"getEnv:".as_slice()]])?;
            let method = s.send(method, b"withContext:", &[*ns])?;
            s.send(vt, b"addMethod", &[s.names[b"getEnv:".as_slice()], method])?;
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
