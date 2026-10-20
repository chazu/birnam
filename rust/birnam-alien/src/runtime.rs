//! Alien owns foreign addresses, library handles and C marshalling. Dependencies
//! are selected through Id messages; executable mappings belong to TCC Units.
use crate::loader::LibraryRegistry;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::RefCell, ffi::c_void, ptr};
use std::collections::HashMap;

type Answer = Result<Operand, Outcome>;
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|o| !o.is_null()).ok_or_else(invalid)
}
fn done(r: Answer) -> Outcome {
    r.map_or_else(|e| e, Outcome::loaded)
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
    id: Protocol,
    ns: Oop,
    roots: Oop,
    model: Oop,
    gc: Oop,
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
    tokens: RefCell<Vec<usize>>,
}
impl Scope<'_> {
    unsafe fn keep(&self, o: Oop) -> Result<Oop, Outcome> {
        unsafe {
            let t = self
                .s
                .send(self.s.roots, b"retain:", &[Operand::object(o)])?
                .as_word()
                .ok_or_else(invalid)?;
            self.tokens.borrow_mut().push(t);
            Ok(o)
        }
    }
    unsafe fn call(&self, r: Oop, n: &[u8], a: &[Operand]) -> Answer {
        unsafe {
            let v = self.s.send(r, n, a)?;
            if let Some(o) = v.as_object() {
                self.keep(o)?;
            }
            Ok(v)
        }
    }
    unsafe fn obj(&self, r: Oop, n: &[u8], a: &[Operand]) -> Result<Oop, Outcome> {
        object(unsafe { self.call(r, n, a) }?)
    }
    unsafe fn named(&self, n: &[u8]) -> Result<Oop, Outcome> {
        unsafe {
            let key = self.keep(self.s.id.intern_name(n)?)?;
            self.obj(self.s.ns, b"at:", &[Operand::object(key)])
        }
    }
    unsafe fn string(&self, b: &[u8]) -> Result<Oop, Outcome> {
        unsafe { self.obj(self.named(b"String")?, b"fromBytes:", &[Operand::bytes(b)]) }
    }
    unsafe fn integer(&self, w: usize) -> Result<Oop, Outcome> {
        unsafe { self.obj(self.named(b"Integer")?, b"fromWord:", &[Operand::word(w)]) }
    }
    unsafe fn word(&self, o: Oop) -> Result<Option<usize>, Outcome> {
        unsafe {
            Ok(self
                .call(self.named(b"Integer")?, b"wordOf:", &[Operand::object(o)])?
                .as_word())
        }
    }
    unsafe fn bytes(&self, o: Oop) -> Result<Option<Vec<u8>>, Outcome> {
        unsafe {
            if self
                .call(self.s.id.receiver, b"behaviorOf:", &[Operand::object(o)])?
                .as_object()
                != Some(self.named(b"StringVt")?)
            {
                return Ok(None);
            }
            let b = self
                .call(o, b"bytes", &[])?
                .as_bytes()
                .ok_or_else(invalid)?;
            Ok(Some(
                borrowed_bytes(b.pointer, b.length)
                    .ok_or_else(invalid)?
                    .to_vec(),
            ))
        }
    }
    unsafe fn error(&self, kind: &[u8], text: &[u8]) -> Answer {
        unsafe {
            let kind = self.keep(self.s.id.intern_name(kind)?)?;
            let text = self.string(text)?;
            self.call(
                self.named(b"Error")?,
                b"kind:message:",
                &[Operand::object(kind), Operand::object(text)],
            )
        }
    }
    unsafe fn nil(&self) -> Answer {
        unsafe { Ok(Operand::object(self.named(b"nil")?)) }
    }
    unsafe fn boolean(&self, v: bool) -> Answer {
        unsafe {
            Ok(Operand::object(self.named(if v {
                b"true"
            } else {
                b"false"
            })?))
        }
    }
    unsafe fn is_error(&self, o: Oop) -> Result<bool, Outcome> {
        unsafe { Ok(self.obj(o, b"isError", &[])? == self.named(b"true")?) }
    }

}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        for t in self.tokens.get_mut().drain(..).rev() {
            unsafe {
                self.s
                    .send(self.s.roots, b"release:", &[Operand::word(t)])
                    .expect("release Alien root");
            }
        }
    }
}
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Operand]) -> Answer {
        unsafe { self.id.send_named(r, n, a) }
    }
    unsafe fn address(&self, scope: &Scope<'_>, pointer: usize, owned: bool) -> Answer {
        unsafe {
            let allocation = Address { pointer, owned };
            let o = scope.obj(
                self.model,
                b"allocate:bytes:",
                &[Operand::object(self.address_vt), Operand::word(0)],
            )?;
            self.addresses.borrow_mut().insert(o.addr(), allocation);
            Ok(Operand::object(o))
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
                            Ok(Operand::object(r))
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
                Ok(Operand::object(
                    scope.string(format!("ForeignAddress(0x{p:x})").as_bytes())?,
                ))
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
                let compiler_name = scope.keep(s.id.intern_name(b"AlienCompiler")?)?;
                let compiler = scope.obj(
                    scope.named(b"Globals")?,
                    b"at:",
                    &[Operand::object(compiler_name)],
                )?;
                let function_name = if matches!(op, Wrap) {
                    a[0]
                } else {
                    scope.named(b"nil")?
                };
                let signature_value = scope.string(&signature)?;
                let source = scope.obj(
                    compiler,
                    b"compile:signature:",
                    &[
                        Operand::object(function_name),
                        Operand::object(signature_value),
                    ],
                )?;
                if scope.is_error(source)? {
                    return Ok(Operand::object(source));
                }
                if matches!(op, Wrap) {
                    let bytes = match recorded_object(scope, a[0], a[1])? {
                        Some(bytes) => bytes,
                        None => {
                            let bytes = scope.obj(
                                scope.named(b"TCC")?,
                                b"compileObject:",
                                &[Operand::object(source)],
                            )?;
                            if scope.is_error(bytes)? {
                                return Ok(Operand::object(bytes));
                            }
                            bytes
                        }
                    };
                    let recipe = [a[0], a[1], a[2], a[3], bytes];
                    install_wrapper(s, scope, &recipe, true)
                } else {
                    let unit = scope.obj(
                        scope.named(b"TCC")?,
                        b"compile:",
                        &[Operand::object(source)],
                    )?;
                    if scope.is_error(unit)? {
                        return Ok(Operand::object(unit));
                    }
                    let value =
                        scope.obj(unit, b"initializeWith:", &[Operand::object(block.unwrap())])?;
                    let address = s.pointer(value).ok_or_else(invalid)?;
                    let token = s
                        .send(s.roots, b"retain:", &[Operand::object(unit)])?
                        .as_word()
                        .ok_or_else(invalid)?;
                    s.callbacks.borrow_mut().insert(address, (unit, token));
                    Ok(Operand::object(value))
                }
            }
            ReleaseCallback => {
                let Some(address) = s.pointer(a[0]) else {
                    return scope
                        .error(b"typeMismatch", b"releaseCallback: needs a ForeignAddress");
                };
                let removed = s.callbacks.borrow_mut().remove(&address);
                if let Some((_, token)) = removed {
                    s.send(s.roots, b"release:", &[Operand::word(token)])?;
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
                        PokeDouble => scope
                            .call(scope.named(b"Float")?, b"bitsOf:", &[Operand::object(a[0])])?
                            .as_word(),
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
                    PeekInt => Ok(Operand::object(
                        scope.integer(ptr::read_unaligned(p as *const usize))?,
                    )),
                    PeekByte => Ok(Operand::object(
                        scope.integer(ptr::read(p as *const u8) as usize)?,
                    )),
                    PeekPtr => s.address(scope, ptr::read_unaligned(p as *const usize), false),
                    PeekDouble => scope.call(
                        scope.named(b"Float")?,
                        b"fromBits:",
                        &[Operand::word(ptr::read_unaligned(p as *const usize))],
                    ),
                    PeekString => {
                        let b = std::ffi::CStr::from_ptr(p as *const _).to_bytes().to_vec();
                        Ok(Operand::object(scope.string(&b)?))
                    }
                    PokeByte => {
                        ptr::write(p as *mut u8, value as u8);
                        Ok(Operand::object(r))
                    }
                    _ => {
                        ptr::write_unaligned(p as *mut usize, value);
                        Ok(Operand::object(r))
                    }
                }
            }
        }
    }
}
// Recipes retain object code and ordinary identities, never process addresses.
unsafe fn recipe_values(q: &Scope<'_>, recipe: Oop) -> Result<[Oop; 5], Outcome> {
    unsafe {
        let size = q.obj(recipe, b"size", &[])?;
        if q.word(size)? != Some(5) {
            return Err(invalid());
        }
        let mut values = [ptr::null_mut(); 5];
        for (i, value) in values.iter_mut().enumerate() {
            let index = q.integer(i)?;
            *value = q.obj(recipe, b"at:", &[Operand::object(index)])?;
        }
        Ok(values)
    }
}
unsafe fn recipe_owner(q: &Scope<'_>) -> Result<Oop, Outcome> {
    unsafe {
        let name = q.keep(q.s.id.intern_name(b"AlienRecipes")?)?;
        q.obj(q.named(b"Globals")?, b"at:", &[Operand::object(name)])
    }
}
unsafe fn recorded_object(q: &Scope<'_>, function: Oop, signature: Oop) -> Result<Option<Oop>, Outcome> {
    unsafe {
        let value = q.obj(recipe_owner(q)?, b"objectFor:signature:",
            &[Operand::object(function), Operand::object(signature)])?;
        if q.is_error(value)? { return Err(Outcome::loaded(Operand::object(value))); }
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
        let unit = q.obj(
            q.named(b"TCC")?,
            b"loadObject:",
            &[Operand::object(recipe[4])],
        )?;
        if q.is_error(unit)? {
            return Ok(Operand::object(unit));
        }
        let address = q.integer(address)?;
        let method = q.obj(unit, b"initializeWith:", &[Operand::object(address)])?;
        if q.is_error(method)? {
            return Ok(Operand::object(method));
        }
        s.wrappers.borrow_mut().push(unit.addr());
        let behavior = q.obj(
            s.id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(recipe[3])],
        )?;
        let result = q.call(
            behavior,
            b"addMethod:as:",
            &[Operand::object(method), Operand::object(recipe[2])],
        )?;
        if result.as_object() == Some(method) {
            if !record { return Ok(Operand::object(method)); }
            let mut row = q.obj(q.named(b"Vector")?, b"new", &[])?;
            for value in recipe {
                row = q.obj(row, b"add:", &[Operand::object(*value)])?;
            }
            let recorded = q.obj(recipe_owner(q)?, b"record:method:",
                &[Operand::object(row), Operand::object(method)])?;
            if q.is_error(recorded)? { return Ok(Operand::object(recorded)); }
            Ok(Operand::object(recipe[3]))
        } else if !record && !result.as_object().map(|v| q.is_error(v)).transpose()?.unwrap_or(false) {
            q.error(b"invalidForeignRestoration", b"method publication did not return the constructed Method")
        } else {
            Ok(result)
        }
    }
}
macro_rules! methods{($(($f:ident,$op:ident)),*$(,)?)=>{$(unsafe extern "C" fn $f(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome{done((||unsafe{let s=&*c.cast::<State>();if s.thread!=std::thread::current().id(){return Err(invalid())}let q=Scope{s,tokens:RefCell::new(Vec::new())};q.keep(r)?;let a=borrowed_slice(a,n).ok_or_else(invalid)?.iter().map(|a|q.keep(object(*a)?)).collect::<Result<Vec<_>,_>>()?;run(s,&q,r,&a,Op::$op)})())})*};}
methods!(
    (library_dir, LibraryDir),
    (open, Open),
    (close, Close),
    (wrap, Wrap),
    (install_recipe, InstallRecipe),
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
unsafe extern "C" fn address_of(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        Ok(s.pointer(object(*v)?)
            .map(Operand::word)
            .unwrap_or_else(Operand::unit))
    })())
}
unsafe extern "C" fn from_address(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let q = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        s.address(&q, v.as_word().ok_or_else(invalid)?, false)
    })())
}
unsafe extern "C" fn check_thread(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    let s = unsafe { &*c.cast::<State>() };
    if n == 0 && s.thread == std::thread::current().id() {
        Outcome::loaded(Operand::word(1))
    } else {
        invalid()
    }
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        for &o in &s.permanent {
            s.send(object(*v)?, b"visit:", &[Operand::object(o)])?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn retire(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let b = v.as_bytes().ok_or_else(invalid)?;
        let b = borrowed_bytes(b.pointer, b.length).ok_or_else(invalid)?;
        if b.len() % size_of::<usize>() != 0 {
            return Err(invalid());
        }
        for chunk in b.chunks_exact(size_of::<usize>()) {
            let o = usize::from_ne_bytes(chunk.try_into().unwrap());
            s.addresses.borrow_mut().remove(&o);
            s.wrappers.borrow_mut().retain(|u| *u != o);
        }
        Ok(Operand::unit())
    })())
}
unsafe fn construct(id: Protocol, ns: Oop) -> Answer {
    unsafe {
        let named =
            |n: &[u8]| object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(n)?)])?);
        let existing = id.send_named(ns, b"at:", &[Operand::object(id.intern_name(b"Alien")?)])?;
        if existing.as_object().is_some() {
            return Ok(existing);
        }
        let mut state = Box::new(State {
            id,
            ns,
            roots: named(b"Roots")?,
            model: named(b"ObjectModel")?,
            gc: named(b"GC")?,
            receiver: ptr::null_mut(),
            address_vt: ptr::null_mut(),
            permanent: Vec::new(),
            addresses: RefCell::new(HashMap::new()),
            wrappers: RefCell::new(Vec::new()),
            callbacks: RefCell::new(HashMap::new()),
            libraries: RefCell::new(LibraryRegistry::default()),
            thread: std::thread::current().id(),
        });
        let q = Scope {
            s: &state,
            tokens: RefCell::new(Vec::new()),
        };
        let vt = q.obj(named(b"ObjectVt")?, b"delegated", &[])?;
        let av = q.obj(named(b"ObjectVt")?, b"delegated", &[])?;
        let receiver = q.obj(
            state.model,
            b"allocate:bytes:",
            &[Operand::object(vt), Operand::word(0)],
        )?;
        let prototype = q.obj(
            state.model,
            b"allocate:bytes:",
            &[Operand::object(av), Operand::word(0)],
        )?;
        // Transfer construction roots to the registered owner before releasing them.
        let tokens = q.tokens.take();
        drop(q);
        state.receiver = receiver;
        state.address_vt = av;
        state.permanent = vec![
            ns,
            state.roots,
            state.model,
            state.gc,
            vt,
            av,
            receiver,
            prototype,
        ];
        state.addresses.borrow_mut().insert(
            prototype.addr(),
            Address {
                pointer: 0,
                owned: false,
            },
        );
        id.send_named(
            id.receiver,
            b"ownerOf:put:",
            &[Operand::object(vt), Operand::object(receiver)],
        )?;
        id.send_named(
            id.receiver,
            b"ownerOf:put:",
            &[Operand::object(av), Operand::object(prototype)],
        )?;
        let s = Box::into_raw(state);
        for (target, name, f) in [
            (vt, b"libraryDir:".as_slice(), library_dir as Method),
            (vt, b"dlopen:", open),
            (vt, b"dlclose:", close),
            (vt, b"wrap:signature:as:on:", wrap),
            (vt, b"installRecipe:", install_recipe),
            (vt, b"callback:signature:", callback),
            (vt, b"releaseCallback:", release_callback),
            (vt, b"alloc:", alloc),
            (vt, b"addressOf:", address_of),
            (vt, b"fromAddress:", from_address),
            (vt, b"checkThread", check_thread),
            (vt, b"rootsDo:", roots),
            (vt, b"retire:", retire),
            (av, b"free", free_address),
            (av, b"isNull", is_null),
            (av, b"=", equal),
            (av, b"printString", print),
            (av, b"peekIntAt:", peek_int),
            (av, b"peekByteAt:", peek_byte),
            (av, b"peekPtrAt:", peek_ptr),
            (av, b"peekDoubleAt:", peek_double),
            (av, b"peekStringAt:", peek_string),
            (av, b"pokeInt:at:", poke_int),
            (av, b"pokeByte:at:", poke_byte),
            (av, b"pokePtr:at:", poke_ptr),
            (av, b"pokeDouble:at:", poke_double),
        ] {
            id.send_named(
                target,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::entry(NativeEntry::new(f, s.cast())),
                ],
            )?;
        }
        id.send_named((*s).roots, b"register:", &[Operand::object(receiver)])?;
        id.send_named(
            (*s).gc,
            b"registerRetirement:",
            &[Operand::object(receiver)],
        )?;
        for (n, o) in [
            (b"Alien".as_slice(), receiver),
            (b"AlienVt", vt),
            (b"ForeignAddress", prototype),
            (b"ForeignAddressVt", av),
        ] {
            id.send_named(
                ns,
                b"at:put:",
                &[Operand::object(id.intern_name(n)?), Operand::object(o)],
            )?;
        }
        for t in tokens {
            id.send_named((*s).roots, b"release:", &[Operand::word(t)])?;
        }
        Ok(Operand::object(receiver))
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([ns]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        construct(*id, object(*ns)?)
    })())
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
