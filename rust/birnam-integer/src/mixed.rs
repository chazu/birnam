//! A complete extension created only after Float exists; no future dependency slot.
use super::*;
pub(super) struct Mixed {
    integer: *const Number,
    float: Oop,
}
unsafe fn run(
    m: &Mixed,
    r: Oop,
    args: &[Operand],
    op: usize,
    visitor: Option<NativeEntry>,
) -> Result<Operand, Outcome> {
    unsafe {
        let n = state(m.integer.cast_mut().cast())?;
        if args.len() != if op == 10 { 0 } else { 1 } {
            return Err(invalid());
        }
        let mut values = vec![r];
        for arg in args {
            values.push(object(*arg)?);
        }
        n.rooted(&values, || {
            let left = n.read(r)?;
            if op == 10 {
                return match left {
                    Some(v) => n.send(
                        m.float,
                        b"fromBits:",
                        &[Operand::word((v as f64).to_bits() as usize)],
                    ),
                    None => Ok(Operand::object(n.nil)),
                };
            }
            let read_float = |value| -> Result<Option<f64>, Outcome> {
                let result = n.send(m.float, b"bitsOf:", &[Operand::object(value)])?;
                if result.kind() == native::UNIT {
                    return Ok(None);
                }
                Ok(Some(f64::from_bits(
                    result.as_word().ok_or_else(invalid)? as u64
                )))
            };
            let a = match left {
                Some(v) => v as f64,
                None => match read_float(r)? {
                    Some(v) => v,
                    None => return Ok(Operand::object(n.nil)),
                },
            };
            let right = values[1];
            if n.is_error(right)? {
                return Ok(Operand::object(right));
            }
            let integer = n.read(right)?;
            if let (Some(a), Some(b)) = (left, integer) {
                return n.calculate_with(a, b, op, visitor);
            }
            let b = match integer {
                Some(v) => v as f64,
                None => match read_float(right)? {
                    Some(v) => v,
                    None => {
                        return n.error(
                            b"typeMismatch",
                            if op > 5 {
                                b"comparison needs a number"
                            } else {
                                b"Integer arithmetic needs a number"
                            },
                        );
                    }
                },
            };
            if op == 3 && b == 0.0 {
                return n.error(b"zeroDivide", b"division by zero");
            }
            let v = match op {
                0 => a + b,
                1 => a - b,
                2 => a * b,
                3 => a / b,
                6 => return Ok(n.boolean(a < b)),
                7 => return Ok(n.boolean(a <= b)),
                8 => return Ok(n.boolean(a > b)),
                9 => return Ok(n.boolean(a >= b)),
                _ => return Err(invalid()),
            };
            n.send(
                m.float,
                b"fromBits:",
                &[Operand::word(v.to_bits() as usize)],
            )
        })
    }
}
macro_rules! mixed {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
            done((|| unsafe {
                run(
                    &*c.cast::<Mixed>(),
                    r,
                    borrowed_slice(a, n).ok_or_else(invalid)?,
                    $op,
                    None,
                )
            })())
        }
    };
}
mixed!(add, 0);
mixed!(sub, 1);
mixed!(mul, 2);
mixed!(div, 3);
mixed!(lt, 6);
mixed!(le, 7);
mixed!(gt, 8);
mixed!(ge, 9);
mixed!(as_float, 10);
pub(super) unsafe extern "C" fn install(
    context: *mut c_void,
    r: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let n = state(context)?;
        let Some([float]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let float = object(*float)?;
        if r != n.prototype || n.extended.get() {
            return Err(invalid());
        }
        n.rooted(&[float], || {
            let zero = n.send(float, b"bitsOf:", &[Operand::object(float)])?;
            zero.as_word().ok_or_else(invalid)?;
            // The completed extension retains Float for the installed methods' lifetime.
            n.send(n.roots, b"retain:", &[Operand::object(float)])?;
            let m = Box::into_raw(Box::new(Mixed { integer: n, float }));
            for (name, method) in [
                (b"+".as_slice(), add as Method),
                (b"-", sub),
                (b"*", mul),
                (b"/", div),
                (b"<", lt),
                (b"<=", le),
                (b">", gt),
                (b">=", ge),
                (b"asFloat", as_float),
            ] {
                let selected = n.send(n.behavior, b"lookup", &[Operand::object(n.names[name])])?;
                let replace = if name == b"asFloat" {
                    selected.kind() == native::UNIT
                } else {
                    METHODS.iter().any(|(base, method)| {
                        *base == name
                            && selected.as_entry() == Some(NativeEntry::new(*method, context))
                    })
                };
                if replace {
                    n.send(
                        n.behavior,
                        b"addMethod",
                        &[
                            Operand::object(n.names[name]),
                            Operand::entry(NativeEntry::new(method, m.cast())),
                        ],
                    )?;
                }
            }
            n.mixed.set(m);
            n.extended.set(true);
            Ok(Operand::unit())
        })
    })())
}

pub(super) fn operation(n: &Number, selected: NativeEntry) -> Option<usize> {
    let m = n.mixed.get();
    if m.is_null() {
        return None;
    }
    [add as Method, sub, mul, div, lt, le, gt, ge]
        .iter()
        .position(|method| selected == NativeEntry::new(*method, m.cast()))
        .map(|i| if i < 4 { i } else { i + 2 })
}

pub(super) unsafe fn evaluate(
    n: &Number,
    selected: NativeEntry,
    left: Operand,
    right: Operand,
    visitor: Operand,
) -> Option<Result<Operand, Outcome>> {
    unsafe {
        let m = n.mixed.get();
        if m.is_null() {
            return None;
        }
        let op = operation(n, selected).filter(|op| *op < 4)?;
        Some((|| {
            let visitor = visitor
                .as_entry()
                .filter(|v| v.method.is_some())
                .ok_or_else(invalid)?;
            run(&*m, object(left)?, &[right], op, Some(visitor))
        })())
    }
}
