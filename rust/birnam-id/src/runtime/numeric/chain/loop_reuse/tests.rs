use super::*;
use birnam_codegen::numeric_chain::{Operation, Operator, Plan, Value};

#[test]
fn native_reuse_preserves_transitive_values_and_earliest_arithmetic_failure() {
    let operators = [
        Operator::Add,
        Operator::Subtract,
        Operator::Multiply,
        Operator::Divide,
        Operator::Remainder,
    ];
    let edges = [
        i64::MIN,
        i64::MIN + 1,
        -17,
        -1,
        0,
        1,
        17,
        i64::MAX - 1,
        i64::MAX,
    ];
    for left in operators {
        for right in operators {
            let operations = vec![
                Operation {
                    operator: left,
                    left: Value::Input(0),
                    right: Value::Input(1),
                },
                Operation {
                    operator: right,
                    left: Value::Input(2),
                    right: Value::Input(3),
                },
                Operation {
                    operator: Operator::Add,
                    left: Value::Result(0),
                    right: Value::Result(1),
                },
                Operation {
                    operator: Operator::Multiply,
                    left: Value::Result(1),
                    right: Value::Input(3),
                },
            ];
            let plan = Plan::new(4, operations.clone()).unwrap();
            assert_eq!(plan.input_dependencies(), [3, 12, 15, 12]);
            let source = CString::new(plan.emit_loop_reuse()).unwrap();
            let unit = unsafe { Unit::compile(&source, &[]) }.unwrap();
            let ordinary: unsafe extern "C" fn(*const i64, *mut i64) -> usize =
                unsafe { core::mem::transmute(unit.symbol(c"numeric_chain").unwrap().as_ptr()) };
            let reuse: Kernel = unsafe {
                core::mem::transmute(unit.symbol(c"numeric_chain_reuse").unwrap().as_ptr())
            };
            let previous = vec![11i64, 3, 5, 2];
            let mut old = vec![0i64; 4];
            assert_eq!(unsafe { ordinary(previous.as_ptr(), old.as_mut_ptr()) }, 0);
            for index in 0..4 {
                for edge in edges {
                    let mut inputs = previous.clone();
                    inputs[index] = edge;
                    let mut expected = vec![0; 4];
                    let mut failure = 0;
                    for (n, operation) in operations.iter().enumerate() {
                        let value = |v| match v {
                            Value::Input(i) => inputs[i as usize],
                            Value::Result(i) => expected[i as usize],
                        };
                        match operation
                            .operator
                            .evaluate(value(operation.left), value(operation.right))
                        {
                            Some(value) => expected[n] = value,
                            None => {
                                failure = n + 1;
                                break;
                            }
                        }
                    }
                    let mut actual = old.clone();
                    let mut baseline = vec![0; 4];
                    assert_eq!(
                        unsafe { ordinary(inputs.as_ptr(), baseline.as_mut_ptr()) },
                        failure
                    );
                    assert_eq!(
                        unsafe { reuse(inputs.as_ptr(), actual.as_mut_ptr(), 1u64 << index) },
                        failure
                    );
                    let prefix = if failure == 0 { 4 } else { failure - 1 };
                    assert_eq!(actual[..prefix], expected[..prefix]);
                    assert_eq!(baseline[..prefix], expected[..prefix]);
                }
            }
            let mut unchanged = old.clone();
            assert_eq!(
                unsafe { reuse(previous.as_ptr(), unchanged.as_mut_ptr(), 0) },
                0
            );
            assert_eq!(unchanged, old);
        }
    }
    let plan = Plan::new(
        64,
        vec![Operation {
            operator: Operator::Add,
            left: Value::Input(63),
            right: Value::Input(0),
        }],
    )
    .unwrap();
    assert_eq!(plan.input_dependencies(), [0x8000000000000001]);
    let unit =
        unsafe { Unit::compile(&CString::new(plan.emit_loop_reuse()).unwrap(), &[]) }.unwrap();
    let reuse: Kernel =
        unsafe { core::mem::transmute(unit.symbol(c"numeric_chain_reuse").unwrap().as_ptr()) };
    let mut inputs = vec![0i64; 64];
    inputs[63] = 17;
    let mut output = vec![11i64];
    assert_eq!(
        unsafe { reuse(inputs.as_ptr(), output.as_mut_ptr(), 1u64 << 63) },
        0
    );
    assert_eq!(output, [17]);
}
