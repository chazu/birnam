//! Completed mixed-number Methods capture the Float object as their context.
use super::*;
unsafe fn run(
    n: &Number,
    float: Oop,
    r: Oop,
    a: &[Oop],
    op: usize,
    visitor: Option<Oop>,
) -> Result<Oop, Oop> {
    unsafe {
        if a.len() != if op == 10 { 0 } else { 1 } {
            return Err(invalid());
        }
        let left = n.read(r)?;
        if op == 10 {
            return match left {
                Some(v) => n.send(float, b"fromBits:", &[n.word((v as f64).to_bits())?]),
                None => Ok(n.nil),
            };
        }
        let read_float = |v| -> Result<Option<f64>, Oop> {
            let bits = n.send(float, b"bitsOf:", &[v])?;
            if bits.is_null() {
                Ok(None)
            } else {
                Ok(Some(f64::from_bits(n.word_bits(bits)?)))
            }
        };
        let left_float = match left {
            Some(v) => v as f64,
            None => match read_float(r)? {
                Some(v) => v,
                None => return Ok(n.nil),
            },
        };
        let right = a[0];
        if n.is_error(right)? {
            return Ok(right);
        }
        let integer = n.read(right)?;
        if let (Some(a), Some(b)) = (left, integer) {
            return n.calculate_with(a, b, op, visitor);
        }
        let right_float = match integer {
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
        if op == 3 && right_float == 0.0 {
            return n.error(b"zeroDivide", b"division by zero");
        }
        let value = match op {
            0 => left_float + right_float,
            1 => left_float - right_float,
            2 => left_float * right_float,
            3 => left_float / right_float,
            6 => return Ok(n.boolean(left_float < right_float)),
            7 => return Ok(n.boolean(left_float <= right_float)),
            8 => return Ok(n.boolean(left_float > right_float)),
            9 => return Ok(n.boolean(left_float >= right_float)),
            _ => return Err(invalid()),
        };
        n.send(float, b"fromBits:", &[n.word(value.to_bits())?])
    }
}
macro_rules! mixed {
    ($name:ident,$op:expr) => {
        pub(super) unsafe extern "C" fn $name(
            _: Oop,
            c: Oop,
            r: Oop,
            a: *const Oop,
            count: usize,
        ) -> Oop {
            done((|| unsafe {
                run(
                    instance()?,
                    c,
                    r,
                    borrowed_slice(a, count).ok_or_else(invalid)?,
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
    _: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    count: usize,
) -> Oop {
    done((|| unsafe {
        let n = state(c)?;
        let [float] = borrowed_slice(a, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let float = object(*float)?;
        if r != n.prototype || n.extended.get() {
            return Err(invalid());
        }
        n.word_bits(n.send(float, b"bitsOf:", &[float])?)?;
        for (i, (name, implementation)) in [
            (b"+".as_slice(), b"mixed::add".as_slice()),
            (b"-", b"mixed::sub"),
            (b"*", b"mixed::mul"),
            (b"/", b"mixed::div"),
            (b"<", b"mixed::lt"),
            (b"<=", b"mixed::le"),
            (b">", b"mixed::gt"),
            (b">=", b"mixed::ge"),
            (b"asFloat", b"mixed::as_float"),
        ]
        .into_iter()
        .enumerate()
        {
            let selected = n.send(n.behavior, b"lookup", &[n.names[name]])?;
            let replace = if i == 8 {
                selected.is_null()
            } else {
                n.same_method(selected, n.witnesses[if i < 4 { i } else { i + 2 }].get())?
            };
            if replace {
                let method = n.method(implementation, float)?;
                n.send(n.behavior, b"addMethod", &[n.names[name], method])?;
                if i < 8 {
                    n.mixed_witnesses[i].set(method);
                }
            }
        }
        n.extended.set(true);
        Ok(ptr::null_mut())
    })())
}
