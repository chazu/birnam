//! Alien owns foreign addresses, library handles and C marshalling. Dependencies
//! are selected through Id messages; executable mappings belong to TCC Units.
use crate::loader::LibraryRegistry;
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::collections::HashMap;

type Answer = Result<Oop, Failure>;
#[derive(Debug)]
enum Failure {
    Native(Oop),
    Value(Oop),
}
impl From<Oop> for Failure {
    fn from(v: Oop) -> Self {
        Self::Native(v)
    }
}
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
use std::sync::atomic::{AtomicPtr, Ordering};
fn invalid() -> Failure {
    Failure::Native(unsafe { abi::object_reference((*API.load(Ordering::Acquire)).invalid) })
}
fn object(v: Oop) -> Result<Oop, Failure> {
    if v.is_null() { Err(invalid()) } else { Ok(v) }
}
fn done(v: Answer) -> Oop {
    match v {
        Ok(v) | Err(Failure::Value(v)) => v,
        Err(Failure::Native(e)) => abi::fault_reference(e),
    }
}
unsafe fn state(c: Oop) -> Result<&'static State, Failure> {
    unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        let s = &*p;
        if s.ns != c || s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        Ok(s)
    }
}
unsafe extern "C" {
    fn malloc(n: usize) -> *mut c_void;
    fn free(p: *mut c_void);
}
struct Address {
    pointer: usize,
    owned: bool,
}
impl Drop for Address {
    fn drop(&mut self) {
        if self.owned && self.pointer != 0 {
            unsafe { free(self.pointer as *mut c_void) };
        }
    }
}
struct State {
    pub(crate) byte_reader: abi::byte_transfer::Reader,
    id: Protocol,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    ready: Cell<bool>,
    ns: Oop,
    roots: Oop,
    model: Oop,
    receiver: Oop,
    address_vt: Oop,
    permanent: Vec<Oop>,
    addresses: RefCell<HashMap<usize, Address>>,
    // Wrapper Units are weak keys: their Methods and active invocations retain them.
    wrappers: RefCell<Vec<usize>>,
    callbacks: RefCell<HashMap<usize, (Oop, usize)>>,
    libraries: RefCell<LibraryRegistry>,
    thread: std::thread::ThreadId,
}
struct Scope<'a> {
    s: &'a State,
    start: usize,
}
impl<'a> Scope<'a> {
    fn new(s: &'a State) -> Self {
        Self {
            s,
            start: s.scoped.borrow().len(),
        }
    }
    unsafe fn keep(&self, o: Oop) -> Result<Oop, Failure> {
        self.s.scoped.borrow_mut().push(o);
        Ok(o)
    }
    unsafe fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Answer {
        unsafe { self.keep(self.s.send(r, n, a)?) }
    }
    unsafe fn obj(&self, r: Oop, n: &[u8], a: &[Oop]) -> Answer {
        object(unsafe { self.call(r, n, a) }?)
    }
    unsafe fn named(&self, n: &[u8]) -> Answer {
        unsafe { self.obj(self.s.ns, b"at:", &[self.s.intern(n)?]) }
    }
    unsafe fn byte_value(&self, b: &[u8]) -> Answer {
        unsafe {
            let a: Vec<_> = b.iter().map(|b| abi::size(*b as usize).unwrap()).collect();
            self.call(self.s.id.root, b"sequence", &a)
        }
    }
    unsafe fn string(&self, b: &[u8]) -> Answer {
        unsafe {
            self.obj(
                self.named(b"String")?,
                b"fromBytes:",
                &[self.byte_value(b)?],
            )
        }
    }
    unsafe fn raw_word(&self, v: usize) -> Answer {
        unsafe {
            if let Some(v) = abi::size(v) {
                return Ok(v);
            }
            self.obj(
                self.named(b"Word")?,
                b"fromUnsignedLow:high:",
                &[
                    abi::size(v & 0xffff_ffff).unwrap(),
                    abi::size(v >> 32).unwrap(),
                ],
            )
        }
    }
    unsafe fn bits(&self, v: Oop) -> Result<usize, Failure> {
        unsafe {
            if let Some(v) = abi::as_size(v) {
                return Ok(v);
            }
            self.keep(v)?;
            let lo = abi::as_size(self.call(v, b"low32", &[])?).ok_or_else(invalid)?;
            let hi = abi::as_size(self.call(v, b"high32", &[])?).ok_or_else(invalid)?;
            Ok(lo | (hi << 32))
        }
    }
    unsafe fn integer(&self, w: usize) -> Answer {
        unsafe { self.obj(self.named(b"Integer")?, b"fromWord:", &[self.raw_word(w)?]) }
    }
    unsafe fn word(&self, o: Oop) -> Result<Option<usize>, Failure> {
        unsafe {
            let v = self.call(self.named(b"Integer")?, b"wordOf:", &[o])?;
            if v.is_null() {
                Ok(None)
            } else {
                self.bits(v).map(Some)
            }
        }
    }
    unsafe fn buffer(&self, o: Oop) -> Result<Vec<u8>, Failure> {
        unsafe {
            let v = self.call(o, b"bytes", &[])?;
            Ok(self.s.byte_reader.read(v)?)
        }
    }
    unsafe fn bytes(&self, o: Oop) -> Result<Option<Vec<u8>>, Failure> {
        unsafe {
            if self.call(self.s.id.root, b"behaviorOf:", &[o])? != self.named(b"StringVt")? {
                return Ok(None);
            }
            self.buffer(o).map(Some)
        }
    }
    unsafe fn error(&self, kind: &[u8], text: &[u8]) -> Answer {
        unsafe {
            let kind = self.keep(self.s.intern(kind)?)?;
            let text = self.string(text)?;
            self.call(self.named(b"Error")?, b"kind:message:", &[kind, text])
        }
    }
    unsafe fn nil(&self) -> Answer {
        unsafe { Ok(self.named(b"nil")?) }
    }
    unsafe fn boolean(&self, v: bool) -> Answer {
        unsafe { Ok(self.named(if v { b"true" } else { b"false" })?) }
    }
    unsafe fn is_error(&self, o: Oop) -> Result<bool, Failure> {
        unsafe { Ok(self.obj(o, b"isError", &[])? == self.named(b"true")?) }
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.scoped.borrow_mut().truncate(self.start);
    }
}
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Answer {
        unsafe { self.id.send(r, self.names[n], a).map_err(Failure::Native) }
    }
    unsafe fn intern(&self, n: &[u8]) -> Answer {
        unsafe {
            if let Some(v) = self.names.get(n) {
                return Ok(*v);
            }
            let a: Vec<_> = n.iter().map(|b| abi::size(*b as usize).unwrap()).collect();
            let name = self.send(self.id.root, b"nameWithBytes", &a)?;
            Ok(self.id.send(name, self.id.intern, &[])?)
        }
    }
    unsafe fn address(&self, scope: &Scope<'_>, pointer: usize, owned: bool) -> Answer {
        unsafe {
            let allocation = Address { pointer, owned };
            let o = scope.obj(
                self.model,
                b"allocate:bytes:",
                &[self.address_vt, scope.raw_word(0)?],
            )?;
            self.addresses.borrow_mut().insert(o.addr(), allocation);
            Ok(o)
        }
    }
    fn pointer(&self, o: Oop) -> Option<usize> {
        self.addresses.borrow().get(&o.addr()).map(|a| a.pointer)
    }
}
#[derive(Clone, Copy)]
enum Op {
    LibraryDir,
    Open,
    Close,
    Wrap,
    InstallRecipe,
    InstallBinding,
    ProvideLibrary,
    ProvideLibraryPath,
    Callback,
    ReleaseCallback,
    Alloc,
    Free,
    IsNull,
    Equal,
    Print,
    PeekInt,
    PeekByte,
    PeekPtr,
    PeekDouble,
    PeekString,
    PokeInt,
    PokeByte,
    PokePtr,
    PokeDouble,
}
unsafe fn run(s: &State, scope: &Scope<'_>, r: Oop, a: &[Oop], op: Op) -> Answer {
    unsafe {
        use Op::*;
        let expected = match op {
            Wrap => 4,
            InstallBinding => 5,
            ProvideLibrary | ProvideLibraryPath => 2,
            Callback | Equal => {
                if matches!(op, Callback) {
                    2
                } else {
                    1
                }
            }
            Free | IsNull | Print => 0,
            PokeInt | PokeByte | PokePtr | PokeDouble => 2,
            _ => 1,
        };
        if a.len() != expected {
            return scope.nil();
        }
        match op {
            ProvideLibraryPath => {
                let Some(name) = scope.bytes(a[1])? else {
                    return scope.error(b"invalidNativeLibrary", b"library name must be a String");
                };
                let Some(path) = scope.bytes(a[0])? else {
                    return scope.error(b"invalidNativeLibrary", b"library path must be a String");
                };
                let result = s.libraries.borrow_mut().provide_path(&name, &path);
                match result {
                    Ok(()) => Ok(r),
                    Err(e) => scope.error(b"invalidNativeLibrary", e.to_string().as_bytes()),
                }
            }
            ProvideLibrary => {
                let Some(name) = scope.bytes(a[1])? else {
                    return scope
                        .error(b"invalidEmbeddedLibrary", b"library name must be a String");
                };
                let buffer = scope.call(a[0], b"bytes", &[])?;
                let count =
                    abi::as_size(scope.call(buffer, b"sizeWord", &[])?).ok_or_else(invalid)?;
                if count > 256 * 1024 * 1024 {
                    return scope.error(b"invalidEmbeddedLibrary", b"library exceeds 256 MiB");
                }
                let bytes = s.byte_reader.read_count(buffer, count)?;
                let result = s.libraries.borrow_mut().provide(&name, &bytes);
                match result {
                    Ok(()) => Ok(r),
                    Err(e) => scope.error(b"invalidEmbeddedLibrary", e.to_string().as_bytes()),
                }
            }
            InstallBinding => install_wrapper(s, scope, &[a[0], a[1], a[2], a[3], a[4]], true),
            LibraryDir | Open | Close => {
                let Some(path) = scope.bytes(a[0])? else {
                    return scope.nil();
                };
                if matches!(op, Close)
                    && !s.wrappers.borrow().is_empty()
                    && s.libraries.borrow().open_count(&path) > 0
                {
                    return scope.error(
                        b"inUse",
                        b"dlclose: a live wrapper may still call this library",
                    );
                }
                let result = {
                    let mut libs = s.libraries.borrow_mut();
                    match op {
                        LibraryDir => libs.set_library_dir(&path),
                        Open => libs.open(&path),
                        _ => libs.close(&path),
                    }
                };
                match result {
                    Ok(()) => {
                        if matches!(op, LibraryDir) {
                            Ok(r)
                        } else {
                            scope.boolean(true)
                        }
                    }
                    Err(e) => {
                        if matches!(op, Close) {
                            scope.error(b"notFound", e.to_string().as_bytes())
                        } else {
                            scope.nil()
                        }
                    }
                }
            }
            Alloc => {
                let Some(n) = scope.word(a[0])? else {
                    return scope.error(b"typeMismatch", b"alloc: needs an Integer byte count");
                };
                if (n as isize) <= 0 {
                    return scope.error(b"badArgument", b"alloc: needs a positive byte count");
                }
                let p = malloc(n);
                if p.is_null() {
                    return scope.error(b"outOfMemory", b"alloc: malloc failed");
                };
                ptr::write_bytes(p.cast::<u8>(), 0, n);
                s.address(scope, p.addr(), true)
            }
            Free => {
                let state = s
                    .addresses
                    .borrow()
                    .get(&r.addr())
                    .map(|v| (v.pointer, v.owned));
                match state {
                    None => scope.error(b"typeMismatch", b"free needs a ForeignAddress"),
                    Some((_, false)) => scope.error(
                        b"notOwned",
                        b"free only accepts an address returned by Alien alloc:",
                    ),
                    Some((0, true)) => {
                        scope.error(b"alreadyFreed", b"the address was already freed")
                    }
                    Some((p, true)) => {
                        s.addresses.borrow_mut().get_mut(&r.addr()).unwrap().pointer = 0;
                        free(p as *mut c_void);
                        scope.nil()
                    }
                }
            }
            IsNull => scope.boolean(s.pointer(r) == Some(0)),
            Equal => scope.boolean(s.pointer(r).is_some() && s.pointer(r) == s.pointer(a[0])),
            Print => {
                let Some(p) = s.pointer(r) else {
                    return scope.nil();
                };
                Ok(scope.string(format!("ForeignAddress(0x{p:x})").as_bytes())?)
            }
            InstallRecipe => {
                let recipe = recipe_values(scope, a[0])?;
                install_wrapper(s, scope, &recipe, false)
            }
            Wrap | Callback => {
                let (_resolved_function, signature, block) = if matches!(op, Wrap) {
                    let Some(name) = scope.bytes(a[0])? else {
                        return scope.nil();
                    };
                    let Some(sig) = scope.bytes(a[1])? else {
                        return scope.nil();
                    };
                    let address = s.libraries.borrow().symbol_default(&name);
                    let address = match address {
                        Ok(p) => p.as_ptr().addr(),
                        Err(e) => return scope.error(b"symbolNotFound", e.to_string().as_bytes()),
                    };
                    (Some((name, address)), sig, None)
                } else {
                    let Some(sig) = scope.bytes(a[1])? else {
                        return scope.nil();
                    };
                    (None, sig, Some(a[0]))
                };
                let compiler_name = scope.keep(s.intern(b"AlienCompiler")?)?;
                let compiler = scope.obj(scope.named(b"Globals")?, b"at:", &[compiler_name])?;
                let function_name = if matches!(op, Wrap) {
                    a[0]
                } else {
                    scope.named(b"nil")?
                };
                let signature_value = scope.string(&signature)?;
                let source = scope.obj(
                    compiler,
                    b"compile:signature:",
                    &[function_name, signature_value],
                )?;
                if scope.is_error(source)? {
                    return Ok(source);
                }
                if matches!(op, Wrap) {
                    let bytes = match recorded_object(scope, a[0], a[1])? {
                        Some(bytes) => bytes,
                        None => {
                            let bytes =
                                scope.obj(scope.named(b"TCC")?, b"compileObject:", &[source])?;
                            if scope.is_error(bytes)? {
                                return Ok(bytes);
                            }
                            bytes
                        }
                    };
                    let recipe = [a[0], a[1], a[2], a[3], bytes];
                    install_wrapper(s, scope, &recipe, true)
                } else {
                    let unit = scope.obj(scope.named(b"TCC")?, b"compile:", &[source])?;
                    if scope.is_error(unit)? {
                        return Ok(unit);
                    }
                    let value = scope.obj(unit, b"initializeWith:", &[block.unwrap()])?;
                    let address = s.pointer(value).ok_or_else(invalid)?;
                    let token =
                        abi::as_size(s.send(s.roots, b"retain:", &[unit])?).ok_or_else(invalid)?;
                    s.callbacks.borrow_mut().insert(address, (unit, token));
                    Ok(value)
                }
            }
            ReleaseCallback => {
                let Some(address) = s.pointer(a[0]) else {
                    return scope
                        .error(b"typeMismatch", b"releaseCallback: needs a ForeignAddress");
                };
                let removed = s.callbacks.borrow_mut().remove(&address);
                if let Some((_, token)) = removed {
                    s.send(s.roots, b"release:", &[scope.raw_word(token)?])?;
                    s.addresses
                        .borrow_mut()
                        .get_mut(&a[0].addr())
                        .unwrap()
                        .pointer = 0;
                    scope.nil()
                } else {
                    scope.error(b"notFound", b"callback is not registered")
                }
            }
            _ => {
                let offset = a[if a.len() == 2 { 1 } else { 0 }];
                let Some(offset) = scope.word(offset)? else {
                    return scope.error(b"typeMismatch", b"the offset must be an Integer");
                };
                // Convert the value before observing the receiver address: conversions may re-enter/free it.
                let value = if a.len() == 2 {
                    match op {
                        PokePtr => s.pointer(a[0]),
                        PokeDouble => Some(scope.bits(scope.call(
                            scope.named(b"Float")?,
                            b"bitsOf:",
                            &[a[0]],
                        )?)?),
                        _ => scope.word(a[0])?,
                    }
                } else {
                    Some(0)
                };
                let Some(value) = value else {
                    return scope.error(b"typeMismatch", b"invalid value for foreign memory write");
                };
                let Some(base) = s.pointer(r) else {
                    return scope.error(
                        b"typeMismatch",
                        b"memory access needs a ForeignAddress receiver",
                    );
                };
                if base == 0 {
                    return scope.error(b"nullPointer", b"cannot peek/poke through a null pointer");
                };
                let p = base.wrapping_add(offset);
                match op {
                    PeekInt => Ok(scope.integer(ptr::read_unaligned(p as *const usize))?),
                    PeekByte => Ok(scope.integer(ptr::read(p as *const u8) as usize)?),
                    PeekPtr => s.address(scope, ptr::read_unaligned(p as *const usize), false),
                    PeekDouble => scope.call(
                        scope.named(b"Float")?,
                        b"fromBits:",
                        &[scope.raw_word(ptr::read_unaligned(p as *const usize))?],
                    ),
                    PeekString => {
                        let b = std::ffi::CStr::from_ptr(p as *const _).to_bytes().to_vec();
                        Ok(scope.string(&b)?)
                    }
                    PokeByte => {
                        ptr::write(p as *mut u8, value as u8);
                        Ok(r)
                    }
                    _ => {
                        ptr::write_unaligned(p as *mut usize, value);
                        Ok(r)
                    }
                }
            }
        }
    }
}
// Recipes retain object code and ordinary identities, never process addresses.
unsafe fn recipe_values(q: &Scope<'_>, recipe: Oop) -> Result<[Oop; 5], Failure> {
    unsafe {
        let size = q.obj(recipe, b"size", &[])?;
        if q.word(size)? != Some(5) {
            return Err(invalid());
        }
        let mut values = [ptr::null_mut(); 5];
        for (i, value) in values.iter_mut().enumerate() {
            let index = q.integer(i)?;
            *value = q.obj(recipe, b"at:", &[index])?;
        }
        Ok(values)
    }
}
unsafe fn recipe_owner(q: &Scope<'_>) -> Result<Oop, Failure> {
    unsafe {
        let name = q.keep(q.s.intern(b"AlienRecipes")?)?;
        q.obj(q.named(b"Globals")?, b"at:", &[name])
    }
}
unsafe fn recorded_object(
    q: &Scope<'_>,
    function: Oop,
    signature: Oop,
) -> Result<Option<Oop>, Failure> {
    unsafe {
        let value = q.obj(
            recipe_owner(q)?,
            b"objectFor:signature:",
            &[function, signature],
        )?;
        if q.is_error(value)? {
            return Err(Failure::Value(value));
        }
        Ok((value != q.named(b"nil")?).then_some(value))
    }
}
unsafe fn install_wrapper(s: &State, q: &Scope<'_>, recipe: &[Oop; 5], record: bool) -> Answer {
    unsafe {
        let name = q.bytes(recipe[0])?.ok_or_else(invalid)?;
        let address = match s.libraries.borrow().symbol_default(&name) {
            Ok(p) => p.as_ptr().addr(),
            Err(e) => return q.error(b"symbolNotFound", e.to_string().as_bytes()),
        };
        let unit = q.obj(q.named(b"TCC")?, b"loadObject:", &[recipe[4]])?;
        if q.is_error(unit)? {
            return Ok(unit);
        }
        let address = q.integer(address)?;
        let method = q.obj(unit, b"initializeWith:", &[address])?;
        if q.is_error(method)? {
            return Ok(method);
        }
        s.wrappers.borrow_mut().push(unit.addr());
        let behavior = q.obj(s.id.root, b"ensurePrivateVtableFor:", &[recipe[3]])?;
        let result = q.call(behavior, b"addMethod:as:", &[method, recipe[2]])?;
        if result == method {
            if !record {
                return Ok(method);
            }
            let mut row = q.obj(q.named(b"Vector")?, b"new", &[])?;
            for value in recipe {
                row = q.obj(row, b"add:", &[*value])?;
            }
            let recorded = q.obj(recipe_owner(q)?, b"record:method:", &[row, method])?;
            if q.is_error(recorded)? {
                return Ok(recorded);
            }
            Ok(recipe[3])
        } else if !record && !q.is_error(result)? {
            q.error(
                b"invalidForeignRestoration",
                b"method publication did not return the constructed Method",
            )
        } else {
            Ok(result)
        }
    }
}
macro_rules! methods{($(($f:ident,$op:ident)),*$(,)?)=>{$(unsafe extern "C" fn $f(_:Oop,c:Oop,r:Oop,a:*const Oop,n:usize)->Oop{done((||unsafe{let s=state(c)?;if s.thread!=std::thread::current().id(){return Err(invalid())}let q=Scope::new(s);q.keep(r)?;let a=borrowed_slice(a,n).ok_or_else(invalid)?.iter().map(|a|q.keep(object(*a)?)).collect::<Result<Vec<_>,_>>()?;run(s,&q,r,&a,Op::$op)})())})*};}
methods!(
    (library_dir, LibraryDir),
    (open, Open),
    (close, Close),
    (wrap, Wrap),
    (install_recipe, InstallRecipe),
    (install_binding, InstallBinding),
    (provide_library, ProvideLibrary),
    (provide_library_path, ProvideLibraryPath),
    (callback, Callback),
    (release_callback, ReleaseCallback),
    (alloc, Alloc),
    (free_address, Free),
    (is_null, IsNull),
    (equal, Equal),
    (print, Print),
    (peek_int, PeekInt),
    (peek_byte, PeekByte),
    (peek_ptr, PeekPtr),
    (peek_double, PeekDouble),
    (peek_string, PeekString),
    (poke_int, PokeInt),
    (poke_byte, PokeByte),
    (poke_ptr, PokePtr),
    (poke_double, PokeDouble)
);
unsafe extern "C" fn address_of(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let q = Scope::new(s);
        match s.pointer(object(*v)?) {
            Some(p) => q.raw_word(p),
            None => Ok(ptr::null_mut()),
        }
    })())
}
unsafe extern "C" fn from_address(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let q = Scope::new(s);
        s.address(&q, q.bits(*v)?, false)
    })())
}
unsafe extern "C" fn check_thread(_: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let _ = state(c)?;
        if n != 0 {
            return Err(invalid());
        }
        Ok(abi::size(1).unwrap())
    })())
}

unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let mut values = s.permanent.clone();
        values.extend(s.names.values().copied());
        values.extend(s.scoped.borrow().iter().copied());
        for o in values {
            s.send(object(*v)?, b"visit:", &[o])?;
        }
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn retire(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        for value in borrowed_slice(a, n).ok_or_else(invalid)? {
            let o = value.addr();
            s.addresses.borrow_mut().remove(&o);
            s.wrappers.borrow_mut().retain(|u| *u != o);
        }
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    count: usize,
) -> Oop {
    API.store(p.cast_mut(), Ordering::Release);
    done((|| unsafe {
        let id = *p;
        let [ns] = borrowed_slice(a, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let symbols = borrowed_slice(l, n)
            .filter(|v| v.len() == NAMES.len())
            .ok_or_else(invalid)?;
        let old = INSTANCE.load(Ordering::Acquire);
        if !old.is_null() {
            let s = &*old;
            return if s.ready.get() && s.ns == *ns && s.id.root == id.root {
                Ok(s.receiver)
            } else {
                Err(invalid())
            };
        }
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |n: &[u8]| send(*ns, b"at:", &[names[n]]);
        let roots = named(b"Roots")?;
        let model = named(b"ObjectModel")?;
        let gc = named(b"GC")?;
        let mut tokens = Vec::new();
        let mut keep = |v| -> Result<Oop, Oop> {
            tokens.push(send(roots, b"retain:", &[v])?);
            Ok(v)
        };
        let vt = keep(send(named(b"ObjectVt")?, b"delegated", &[])?)?;
        let av = keep(send(named(b"ObjectVt")?, b"delegated", &[])?)?;
        let receiver = keep(send(vt, b"allocate", &[abi::size(0).unwrap()])?)?;
        let prototype = keep(send(av, b"allocate", &[abi::size(0).unwrap()])?)?;
        let s = Box::leak(Box::new(State {
                byte_reader: abi::byte_transfer::Reader::new(id, image, roots, |n| names[n])?,
            id,
            names,
            ns: *ns,
            roots,
            model,
            receiver,
            address_vt: av,
            ready: Cell::new(false),
            scoped: RefCell::new(Vec::new()),
            permanent: vec![*ns, roots, model, gc, vt, av, receiver, prototype],
            addresses: RefCell::new(HashMap::new()),
            wrappers: RefCell::new(Vec::new()),
            callbacks: RefCell::new(HashMap::new()),
            libraries: RefCell::new(LibraryRegistry::default()),
            thread: std::thread::current().id(),
        }));
        INSTANCE.store(s, Ordering::Release);
        s.addresses.borrow_mut().insert(
            prototype.addr(),
            Address {
                pointer: 0,
                owned: false,
            },
        );
        for (i, (address, name)) in INSTALL.iter().enumerate() {
            let m = s.send(image, b"newMethodAt:", &[abi::size(i).unwrap()])?;
            let m = s.send(m, b"withContext:", &[*ns])?;
            s.send(
                if *address { av } else { vt },
                b"addMethod",
                &[s.intern(name)?, m],
            )?;
        }
        s.send(roots, b"register:", &[receiver])?;
        s.send(gc, b"registerRetirement:", &[receiver])?;
        for (v, o) in [(vt, receiver), (av, prototype)] {
            s.send(id.root, b"ownerOf:put:", &[v, o])?;
        }
        for (name, value) in [
            (b"Alien".as_slice(), receiver),
            (b"AlienVt", vt),
            (b"ForeignAddress", prototype),
            (b"ForeignAddressVt", av),
        ] {
            s.send(*ns, b"at:put:", &[s.intern(name)?, value])?;
        }
        for t in tokens {
            s.send(roots, b"release:", &[t])?;
        }
        s.ready.set(true);
        Ok(receiver)
    })())
}
static INSTALL: &[(bool, &[u8])] = &[
    (false, b"libraryDir:"),
    (false, b"dlopen:"),
    (false, b"dlclose:"),
    (false, b"wrap:signature:as:on:"),
    (false, b"bind:signature:as:on:"),
    (false, b"bind:signature:as:on:code:"),
    (false, b"provideLibrary:named:"),
    (false, b"provideLibraryPath:named:"),
    (false, b"installRecipe:"),
    (false, b"callback:signature:"),
    (false, b"releaseCallback:"),
    (false, b"alloc:"),
    (false, b"addressOf:"),
    (false, b"fromAddress:"),
    (false, b"checkThread"),
    (false, b"rootsDo:"),
    (false, b"retire:"),
    (true, b"free"),
    (true, b"isNull"),
    (true, b"="),
    (true, b"printString"),
    (true, b"peekIntAt:"),
    (true, b"peekByteAt:"),
    (true, b"peekPtrAt:"),
    (true, b"peekDoubleAt:"),
    (true, b"peekStringAt:"),
    (true, b"pokeInt:at:"),
    (true, b"pokeByte:at:"),
    (true, b"pokePtr:at:"),
    (true, b"pokeDouble:at:"),
];
macro_rules! literals { ($($v:literal),*$(,)?) => {
 const NAMES:&[&[u8]]=&[$($v),*];
 static LITERALS:&[Literal]=&[$(Literal::new($v)),*];
}; }
literals![
    b"at:",
    b"sequence",
    b"String",
    b"fromBytes:",
    b"Word",
    b"fromUnsignedLow:high:",
    b"low32",
    b"high32",
    b"Integer",
    b"fromWord:",
    b"wordOf:",
    b"bytes",
    b"sizeWord",
    b"atWord:",
    b"behaviorOf:",
    b"StringVt",
    b"Error",
    b"kind:message:",
    b"nil",
    b"true",
    b"false",
    b"isError",
    b"nameWithBytes",
    b"allocate:bytes:",
    b"invalidNativeLibrary",
    b"invalidEmbeddedLibrary",
    b"inUse",
    b"notFound",
    b"typeMismatch",
    b"badArgument",
    b"outOfMemory",
    b"notOwned",
    b"alreadyFreed",
    b"symbolNotFound",
    b"AlienCompiler",
    b"Globals",
    b"compile:signature:",
    b"TCC",
    b"compileObject:",
    b"compile:",
    b"initializeWith:",
    b"retain:",
    b"release:",
    b"Float",
    b"bitsOf:",
    b"nullPointer",
    b"fromBits:",
    b"size",
    b"AlienRecipes",
    b"objectFor:signature:",
    b"loadObject:",
    b"ensurePrivateVtableFor:",
    b"addMethod:as:",
    b"Vector",
    b"new",
    b"add:",
    b"record:method:",
    b"invalidForeignRestoration",
    b"visit:",
    b"Roots",
    b"ObjectModel",
    b"GC",
    b"ObjectVt",
    b"delegated",
    b"allocate",
    b"newMethodAt:",
    b"withContext:",
    b"addMethod",
    b"register:",
    b"registerRetirement:",
    b"ownerOf:put:",
    b"Alien",
    b"AlienVt",
    b"ForeignAddress",
    b"ForeignAddressVt",
    b"at:put:",
    b"libraryDir:",
    b"dlopen:",
    b"dlclose:",
    b"wrap:signature:as:on:",
    b"bind:signature:as:on:",
    b"bind:signature:as:on:code:",
    b"provideLibrary:named:",
    b"provideLibraryPath:named:",
    b"installRecipe:",
    b"callback:signature:",
    b"releaseCallback:",
    b"alloc:",
    b"addressOf:",
    b"fromAddress:",
    b"checkThread",
    b"rootsDo:",
    b"retire:",
    b"free",
    b"isNull",
    b"=",
    b"printString",
    b"peekIntAt:",
    b"peekByteAt:",
    b"peekPtrAt:",
    b"peekDoubleAt:",
    b"peekStringAt:",
    b"pokeInt:at:",
    b"pokeByte:at:",
    b"pokePtr:at:",
    b"pokeDouble:at:",
    b"copyByteValuesFrom:count:to:",
    b"appendByteValues",
    b"objectBehavior",
    b"methodAt:",
    b"releaseTokensProtecting",
    b"receiveByteValues",
];
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: 76,
        entry: Some(library_dir),
    },
    MethodSpec {
        name: 77,
        entry: Some(open),
    },
    MethodSpec {
        name: 78,
        entry: Some(close),
    },
    MethodSpec {
        name: 79,
        entry: Some(wrap),
    },
    MethodSpec {
        name: 80,
        entry: Some(wrap),
    },
    MethodSpec {
        name: 81,
        entry: Some(install_binding),
    },
    MethodSpec {
        name: 82,
        entry: Some(provide_library),
    },
    MethodSpec {
        name: 83,
        entry: Some(provide_library_path),
    },
    MethodSpec {
        name: 84,
        entry: Some(install_recipe),
    },
    MethodSpec {
        name: 85,
        entry: Some(callback),
    },
    MethodSpec {
        name: 86,
        entry: Some(release_callback),
    },
    MethodSpec {
        name: 87,
        entry: Some(alloc),
    },
    MethodSpec {
        name: 88,
        entry: Some(address_of),
    },
    MethodSpec {
        name: 89,
        entry: Some(from_address),
    },
    MethodSpec {
        name: 90,
        entry: Some(check_thread),
    },
    MethodSpec {
        name: 91,
        entry: Some(roots),
    },
    MethodSpec {
        name: 92,
        entry: Some(retire),
    },
    MethodSpec {
        name: 93,
        entry: Some(free_address),
    },
    MethodSpec {
        name: 94,
        entry: Some(is_null),
    },
    MethodSpec {
        name: 95,
        entry: Some(equal),
    },
    MethodSpec {
        name: 96,
        entry: Some(print),
    },
    MethodSpec {
        name: 97,
        entry: Some(peek_int),
    },
    MethodSpec {
        name: 98,
        entry: Some(peek_byte),
    },
    MethodSpec {
        name: 99,
        entry: Some(peek_ptr),
    },
    MethodSpec {
        name: 100,
        entry: Some(peek_double),
    },
    MethodSpec {
        name: 101,
        entry: Some(peek_string),
    },
    MethodSpec {
        name: 102,
        entry: Some(poke_int),
    },
    MethodSpec {
        name: 103,
        entry: Some(poke_byte),
    },
    MethodSpec {
        name: 104,
        entry: Some(poke_ptr),
    },
    MethodSpec {
        name: 105,
        entry: Some(poke_double),
    },

    MethodSpec { name: NAMES.len()-1, entry: Some(abi::byte_transfer::receive_entry) },
];
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
