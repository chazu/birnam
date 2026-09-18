//! Independent Integer protocol oracle. No link to the Integer implementation.
use super::*;

type Create = unsafe extern "C" fn(*const SeedApi, *mut Oop) -> *mut c_void;
type Destroy = unsafe extern "C" fn(*mut c_void);
const OVERFLOW: Status = 0x100;

pub(super) struct IntegerFixture {
    _dylib: Dylib,
    handle: *mut c_void,
    destroy: Destroy,
    factory: Oop,
    behavior: Oop,
    from_word: Oop,
    as_word: Oop,
    add: Oop,
    subtract: Oop,
    multiply: Oop,
    constructor_wrapper: Box<Wrapper>,
    allocator_wrapper: Box<Wrapper>,
}
impl IntegerFixture {
    pub(super) fn load(h: Harness, path: &str) -> Self {
        let dylib = Dylib::open(path);
        let create: Create =
            unsafe { std::mem::transmute(dylib.symbol(c"birnam_integer_create_v1")) };
        let destroy: Destroy =
            unsafe { std::mem::transmute(dylib.symbol(c"birnam_integer_destroy_v1")) };
        let mut factory = 0;
        let handle = unsafe { create(h.0, &mut factory) };
        assert!(!handle.is_null());
        let behavior = h.send(factory, h.intern(b"integerBehavior"), &[]).unwrap();
        assert_eq!(behavior.kind, OBJECT);
        let from_word = h.intern(b"fromWord:");
        Self {
            _dylib: dylib,
            handle,
            destroy,
            factory,
            behavior: behavior.a,
            from_word,
            as_word: h.intern(b"asWord"),
            add: h.intern(b"+"),
            subtract: h.intern(b"-"),
            multiply: h.intern(b"*"),
            constructor_wrapper: Box::new(Wrapper {
                original: h.method(h.behavior(factory), from_word),
                calls: Cell::new(0),
            }),
            allocator_wrapper: Box::new(Wrapper {
                original: h.method(h.behavior(behavior.a), h.api().selectors.allocate),
                calls: Cell::new(0),
            }),
        }
    }
    fn make(&self, h: Harness, value: i64) -> Oop {
        let result = h
            .send(self.factory, self.from_word, &[Value::word(value as usize)])
            .unwrap();
        assert_eq!(result.kind, OBJECT);
        assert_eq!(h.behavior(result.a), self.behavior);
        result.a
    }
    fn read(&self, h: Harness, object: Oop) -> i64 {
        let result = h.send(object, self.as_word, &[]).unwrap();
        assert_eq!(result.kind, WORD);
        result.a as i64
    }
    fn binary(&self, h: Harness, lhs: Oop, op: Oop, rhs: Oop) -> Result<i64, Status> {
        let result = h.send(lhs, op, &[Value::object(rhs)])?;
        assert_eq!(result.kind, OBJECT);
        assert_ne!(
            result.a, lhs,
            "default arithmetic should allocate a fresh result"
        );
        assert_ne!(result.a, rhs);
        Ok(self.read(h, result.a))
    }
    pub(super) fn exercise(
        &self,
        h: Harness,
        seed_create: SeedCreate,
        seed_destroy: SeedDestroy,
        path: &str,
    ) {
        case("integer-construction-read", || {
            for value in [0, 1, -1, 42, -123456789, i64::MIN, i64::MAX] {
                let first = self.make(h, value);
                let second = self.make(h, value);
                assert_ne!(first, second);
                assert_eq!(self.read(h, first), value);
                assert_eq!(self.read(h, second), value);
            }
        });
        case("integer-checked-arithmetic", || {
            // Independent widened oracle: do not reproduce checked_i64 calls.
            let values = [
                i64::MIN,
                i64::MIN + 1,
                -17,
                -1,
                0,
                1,
                19,
                i64::MAX - 1,
                i64::MAX,
            ];
            for a in values {
                for b in values {
                    let lhs = self.make(h, a);
                    let rhs = self.make(h, b);
                    for (op, wide) in [
                        (self.add, i128::from(a) + i128::from(b)),
                        (self.subtract, i128::from(a) - i128::from(b)),
                        (self.multiply, i128::from(a) * i128::from(b)),
                    ] {
                        let expected = i64::try_from(wide).map_err(|_| OVERFLOW);
                        assert_eq!(self.binary(h, lhs, op, rhs), expected, "operands {a}, {b}");
                        assert_eq!(self.read(h, lhs), a);
                        assert_eq!(self.read(h, rhs), b);
                    }
                }
            }
        });
        case("integer-overflow-no-construction", || {
            let max = self.make(h, i64::MAX);
            let min = self.make(h, i64::MIN);
            let one = self.make(h, 1);
            let neg_one = self.make(h, -1);
            let two = self.make(h, 2);
            self.wrap_constructor(h);
            let before = self.constructor_wrapper.calls.get();
            for (lhs, op, rhs) in [
                (max, self.add, one),
                (min, self.add, neg_one),
                (min, self.subtract, one),
                (max, self.subtract, neg_one),
                (max, self.multiply, two),
                (min, self.multiply, neg_one),
            ] {
                // Also ensure the failed call leaves no result in the ABI slot.
                let mut out = Value::word(123);
                let args = [Value::object(rhs)];
                assert_eq!(
                    unsafe { (h.api().send)(h.0, lhs, op, args.as_ptr(), 1, &mut out) },
                    OVERFLOW
                );
                assert_eq!(out, Value::none());
            }
            assert_eq!(self.constructor_wrapper.calls.get(), before);
            assert_eq!(self.read(h, max), i64::MAX);
            assert_eq!(self.read(h, min), i64::MIN);
            self.restore_constructor(h);
        });
        case("integer-invalid-operands", || {
            let lhs = self.make(h, 7);
            assert_eq!(h.send(self.factory, self.from_word, &[]), Err(INVALID));
            assert_eq!(
                h.send(self.factory, self.from_word, &[Value::object(lhs)]),
                Err(INVALID)
            );
            assert_eq!(h.send(lhs, self.add, &[]), Err(INVALID));
            assert_eq!(h.send(lhs, self.add, &[Value::word(3)]), Err(INVALID));
            assert_eq!(h.send(lhs, self.add, &[Value::object(1)]), Err(INVALID));
            assert_eq!(h.send(lhs, self.as_word, &[Value::none()]), Err(INVALID));
            let impostor = h.allocate(h.api().roots.object, 0);
            h.reassociate(impostor, self.behavior);
            assert_eq!(h.send(impostor, self.as_word, &[]), Err(INVALID));
            assert_eq!(
                h.send(impostor, self.add, &[Value::object(lhs)]),
                Err(INVALID)
            );
            assert_eq!(
                h.send(lhs, self.add, &[Value::object(impostor)]),
                Err(INVALID)
            );
            assert_eq!(self.read(h, lhs), 7);
        });
        case("integer-replace-arithmetic", || {
            let lhs = self.make(h, 35);
            let rhs = self.make(h, 7);
            let original = h.method(self.behavior, self.add);
            assert_eq!(self.binary(h, lhs, self.add, rhs), Ok(42));
            h.install(self.behavior, self.add, Method::plain(return_rhs));
            assert_eq!(
                h.send(lhs, self.add, &[Value::object(rhs)]),
                Ok(Value::object(rhs))
            );
            h.install(self.behavior, self.add, original);
            assert_eq!(self.binary(h, lhs, self.add, rhs), Ok(42));
            assert_eq!(self.read(h, lhs), 35);
        });
        case("integer-replace-construction-allocation", || {
            let lhs = self.make(h, 6);
            let rhs = self.make(h, 7);
            let old_meta = h.behavior(self.behavior);
            let custom_meta = h.delegate(old_meta);
            self.wrap_constructor(h);
            h.install(
                custom_meta,
                h.api().selectors.allocate,
                Method {
                    entry: wrapped,
                    data: (&*self.allocator_wrapper as *const Wrapper)
                        .cast_mut()
                        .cast(),
                },
            );
            h.reassociate(self.behavior, custom_meta);
            let constructions = self.constructor_wrapper.calls.get();
            let allocations = self.allocator_wrapper.calls.get();
            assert_eq!(self.binary(h, lhs, self.multiply, rhs), Ok(42));
            assert_eq!(self.constructor_wrapper.calls.get(), constructions + 1);
            assert_eq!(self.allocator_wrapper.calls.get(), allocations + 1);
            assert_eq!(self.read(h, self.make(h, -42)), -42);
            assert_eq!(self.constructor_wrapper.calls.get(), constructions + 2);
            assert_eq!(self.allocator_wrapper.calls.get(), allocations + 2);
            h.reassociate(self.behavior, old_meta);
            self.restore_constructor(h);
        });
        case("integer-independent-seeds", || {
            let other_h = Harness(unsafe { seed_create() });
            assert!(!other_h.0.is_null());
            let other = Self::load(other_h, path);
            let here = self.make(h, 12);
            let there = other.make(other_h, 30);
            assert_eq!(
                h.send(here, self.add, &[Value::object(there)]),
                Err(INVALID)
            );
            assert_eq!(
                other_h.send(there, other.add, &[Value::object(here)]),
                Err(INVALID)
            );
            assert_eq!(other.binary(other_h, there, other.add, there), Ok(60));
            assert_eq!(unsafe { seed_destroy(other_h.0) }, OK);
            unsafe { other.destroy() };
            assert_eq!(self.binary(h, here, self.add, here), Ok(24));
        });
    }
    fn wrap_constructor(&self, h: Harness) {
        h.install(
            h.behavior(self.factory),
            self.from_word,
            Method {
                entry: wrapped,
                data: (&*self.constructor_wrapper as *const Wrapper)
                    .cast_mut()
                    .cast(),
            },
        );
    }
    fn restore_constructor(&self, h: Harness) {
        h.install(
            h.behavior(self.factory),
            self.from_word,
            self.constructor_wrapper.original,
        );
    }
    pub(super) fn exercise_private_behavior(&self, h: Harness, ensure_private: Oop) {
        case("integer-private-behavior", || {
            let lhs = self.make(h, 35);
            let rhs = self.make(h, 7);
            let private = h.send(lhs, ensure_private, &[]).unwrap();
            assert_eq!(private.kind, OBJECT);
            assert_ne!(private.a, self.behavior);
            h.install(private.a, self.add, Method::plain(return_rhs));
            assert_eq!(
                h.send(lhs, self.add, &[Value::object(rhs)]),
                Ok(Value::object(rhs))
            );
            assert_eq!(self.binary(h, rhs, self.add, lhs), Ok(42));
            assert_eq!(self.binary(h, lhs, self.subtract, rhs), Ok(28));
            assert_eq!(self.read(h, lhs), 35);
            assert_eq!(h.send(lhs, ensure_private, &[]), Ok(private));
        });
    }
    // Seed destruction is an explicit precondition; retained wrapper state and
    // dylib handles drop only after all dictionaries containing them are gone.
    pub(super) unsafe fn destroy(self) {
        unsafe { (self.destroy)(self.handle) };
    }
}

unsafe extern "C" fn return_rhs(
    _: *const SeedApi,
    _: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if count != 1 {
        return INVALID;
    }
    unsafe { out.write(*args) };
    OK
}
