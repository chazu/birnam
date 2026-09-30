//! CST lowering regressions independent of the retired C-shim emitter.
use super::{
    Expr, LowerError, OwnedCst, Program, Statement, lower_expr, lower_literal, lower_program,
};
use std::borrow::Cow;

fn node(rule: &[u8], start: usize, end: usize, children: Vec<OwnedCst>) -> OwnedCst {
    OwnedCst::new(rule, start, end, children)
}
fn message<'a>(
    receiver: Expr<'a>,
    selector: &'a [u8],
    args: Vec<Expr<'a>>,
    source: &'a [u8],
) -> Expr<'a> {
    Expr::Send {
        receiver: Box::new(receiver),
        selector: Cow::Borrowed(selector),
        args,
        source,
    }
}

#[test]
fn lowers_cst_literals_without_runtime_or_allocator_access() {
    let integer = node(b"number", 0, 3, vec![]);
    assert_eq!(
        lower_literal(&integer, b"-42"),
        Ok(Expr::SmallIntLiteral(b"-42"))
    );
    let owned = OwnedCst::new(b"number", 0, 2, Vec::new());
    assert_eq!(
        lower_literal(&owned, b"17"),
        Ok(Expr::SmallIntLiteral(b"17"))
    );
    let embedded = OwnedCst::embedded_string(0, 3, b"a\0b\xff".to_vec());
    assert_eq!(
        lower_literal(&embedded, b"foo"),
        Ok(Expr::StringOwned(b"a\0b\xff".to_vec()))
    );
    let wrapped = node(
        b"expr",
        0,
        3,
        vec![node(b"primary", 0, 3, vec![node(b"float", 0, 3, vec![])])],
    );
    assert_eq!(lower_literal(&wrapped, b"3.5"), Ok(Expr::Float(b"3.5")));

    let string = node(b"string", 0, 7, vec![]);
    assert_eq!(
        lower_literal(&string, br#""a\n\c""#),
        Ok(Expr::StringLiteral(br#""a\n\c""#))
    );

    let symbol = node(b"symbol", 0, 4, vec![]);
    assert_eq!(lower_literal(&symbol, b"#at:"), Ok(Expr::Symbol(b"at:")));

    for (name, expected) in [
        (b"nil".as_slice(), Expr::Nil),
        (b"true".as_slice(), Expr::True),
        (b"false".as_slice(), Expr::False),
    ] {
        let literal = node(b"ident", 0, name.len(), vec![]);
        assert_eq!(lower_literal(&literal, name), Ok(expected));
    }

    let global = node(b"ident", 0, 7, vec![]);
    assert_eq!(
        lower_literal(&global, b"Integer"),
        Ok(Expr::Global(b"Integer"))
    );

    let invalid = node(b"number", 1, 4, vec![]);
    assert_eq!(lower_literal(&invalid, b"42"), Err(LowerError::InvalidSpan));

    let unsupported = node(b"send", 0, 3, vec![]);
    assert_eq!(
        lower_literal(&unsupported, b"foo"),
        Err(LowerError::UnsupportedRule)
    );
}

#[test]
fn lowers_cst_sends_and_keyword_selector_parts_deterministically() {
    let binary_source = b"[3 + 4]";
    let binary = node(
        b"send",
        0,
        binary_source.len(),
        vec![
            node(
                b"expr",
                1,
                2,
                vec![node(b"primary", 1, 2, vec![node(b"number", 1, 2, vec![])])],
            ),
            node(
                b"message",
                3,
                6,
                vec![node(
                    b"binaryMsg",
                    3,
                    6,
                    vec![
                        node(b"binops", 3, 4, vec![]),
                        node(
                            b"expr",
                            5,
                            6,
                            vec![node(b"primary", 5, 6, vec![node(b"number", 5, 6, vec![])])],
                        ),
                    ],
                )],
            ),
        ],
    );
    assert_eq!(
        lower_expr(&binary, binary_source).unwrap(),
        message(
            Expr::SmallIntLiteral(b"3"),
            b"+",
            vec![Expr::SmallIntLiteral(b"4")],
            binary_source
        )
    );

    let keyword_source = b"[x foo: 1 bar: 2]";
    let keyword = node(
        b"send",
        0,
        keyword_source.len(),
        vec![
            node(
                b"expr",
                1,
                2,
                vec![node(b"primary", 1, 2, vec![node(b"ident", 1, 2, vec![])])],
            ),
            node(
                b"message",
                3,
                16,
                vec![node(
                    b"keywordMsg",
                    3,
                    16,
                    vec![
                        node(b"keyword", 3, 7, vec![]),
                        node(
                            b"expr",
                            8,
                            9,
                            vec![node(b"primary", 8, 9, vec![node(b"number", 8, 9, vec![])])],
                        ),
                        node(b"keyword", 10, 14, vec![]),
                        node(
                            b"expr",
                            15,
                            16,
                            vec![node(
                                b"primary",
                                15,
                                16,
                                vec![node(b"number", 15, 16, vec![])],
                            )],
                        ),
                    ],
                )],
            ),
        ],
    );
    assert_eq!(
        lower_expr(&keyword, keyword_source).unwrap(),
        message(
            Expr::Global(b"x"),
            b"foo:bar:",
            vec![Expr::SmallIntLiteral(b"1"), Expr::SmallIntLiteral(b"2")],
            keyword_source
        )
    );
}

#[test]
fn folds_same_category_cst_message_chains_left_to_right() {
    let source = b"[1 + 2 + 3]";
    let number = |start, end| {
        node(
            b"expr",
            start,
            end,
            vec![node(
                b"primary",
                start,
                end,
                vec![node(b"number", start, end, vec![])],
            )],
        )
    };
    let binary_message = |start, end, arg_start, arg_end| {
        node(
            b"binaryMsg",
            start,
            end,
            vec![
                node(b"binops", start, start + 1, vec![]),
                number(arg_start, arg_end),
            ],
        )
    };
    let send = node(
        b"send",
        0,
        source.len(),
        vec![
            number(1, 2),
            node(
                b"message",
                3,
                10,
                vec![binary_message(3, 6, 5, 6), binary_message(7, 10, 9, 10)],
            ),
        ],
    );
    assert_eq!(
        lower_expr(&send, source).unwrap(),
        message(
            message(
                Expr::SmallIntLiteral(b"1"),
                b"+",
                vec![Expr::SmallIntLiteral(b"2")],
                source
            ),
            b"+",
            vec![Expr::SmallIntLiteral(b"3")],
            source
        )
    );
}

#[test]
fn folds_threaded_dispatches_left_to_right_without_a_new_expr_shape() {
    let source = b"[x => foo => + 1 => bar: 2]";
    let primary = |rule: &'static [u8], start, end| {
        node(
            b"expr",
            start,
            end,
            vec![node(
                b"primary",
                start,
                end,
                vec![node(rule, start, end, vec![])],
            )],
        )
    };
    let dispatch = |start, end, message| node(b"threadDispatch", start, end, vec![message]);
    let send = node(
        b"send",
        0,
        source.len(),
        vec![
            primary(b"ident", 1, 2),
            node(
                b"message",
                3,
                26,
                vec![node(
                    b"threadedMsg",
                    3,
                    26,
                    vec![
                        dispatch(6, 9, node(b"unaryMsg", 6, 9, vec![])),
                        dispatch(
                            13,
                            16,
                            node(
                                b"binaryMsg",
                                13,
                                16,
                                vec![node(b"binops", 13, 14, vec![]), primary(b"number", 15, 16)],
                            ),
                        ),
                        dispatch(
                            20,
                            26,
                            node(
                                b"keywordMsg",
                                20,
                                26,
                                vec![node(b"keyword", 20, 24, vec![]), primary(b"number", 25, 26)],
                            ),
                        ),
                    ],
                )],
            ),
        ],
    );
    assert_eq!(
        lower_expr(&send, source).unwrap(),
        message(
            message(
                message(Expr::Global(b"x"), b"foo", vec![], source),
                b"+",
                vec![Expr::SmallIntLiteral(b"1")],
                source
            ),
            b"bar:",
            vec![Expr::SmallIntLiteral(b"2")],
            source
        )
    );
}

#[test]
fn lowers_collections_and_logic_variables_to_the_reference_send_shapes() {
    let vector_source = b"#[1 2]";
    let vector = node(
        b"vectorLit",
        0,
        vector_source.len(),
        vec![
            node(b"expr", 2, 3, vec![node(b"number", 2, 3, vec![])]),
            node(b"expr", 4, 5, vec![node(b"number", 4, 5, vec![])]),
        ],
    );
    assert_eq!(
        lower_expr(&vector, vector_source).unwrap(),
        message(
            message(
                message(Expr::Global(b"Vector"), b"new", vec![], vector_source),
                b"add:",
                vec![Expr::SmallIntLiteral(b"1")],
                vector_source
            ),
            b"add:",
            vec![Expr::SmallIntLiteral(b"2")],
            vector_source
        )
    );

    let map_source = b"{#a -> 1}";
    let map = node(
        b"mapLit",
        0,
        map_source.len(),
        vec![node(
            b"mapEntry",
            1,
            8,
            vec![
                node(b"expr", 1, 3, vec![node(b"symbol", 1, 3, vec![])]),
                node(b"expr", 7, 8, vec![node(b"number", 7, 8, vec![])]),
            ],
        )],
    );
    assert_eq!(
        lower_expr(&map, map_source).unwrap(),
        message(
            message(Expr::Global(b"Map"), b"new", vec![], map_source),
            b"at:put:",
            vec![Expr::Symbol(b"a"), Expr::SmallIntLiteral(b"1")],
            map_source
        )
    );

    let logic_source = b"?p";
    let logic = node(
        b"logicVar",
        0,
        logic_source.len(),
        vec![node(b"ident", 1, 2, vec![])],
    );
    assert_eq!(
        lower_expr(&logic, logic_source).unwrap(),
        message(
            Expr::Global(b"LogicVar"),
            b"named:",
            vec![Expr::Symbol(b"p")],
            logic_source
        )
    );
}

#[test]
fn lowers_block_parameters_and_statement_bindings() {
    let source = b"[:x | let y := x; y]";
    let block = node(
        b"block",
        0,
        source.len(),
        vec![
            node(b"ident", 2, 3, vec![]),
            node(
                b"statements",
                5,
                19,
                vec![
                    node(
                        b"statement",
                        6,
                        16,
                        vec![node(
                            b"letBinding",
                            6,
                            16,
                            vec![
                                node(b"ident", 10, 11, vec![]),
                                node(b"expr", 15, 16, vec![node(b"ident", 15, 16, vec![])]),
                            ],
                        )],
                    ),
                    node(
                        b"statement",
                        18,
                        19,
                        vec![node(b"expr", 18, 19, vec![node(b"ident", 18, 19, vec![])])],
                    ),
                ],
            ),
        ],
    );
    let lowered = lower_expr(&block, source).unwrap();
    let Expr::Block(block) = &lowered else {
        panic!("expected block expression");
    };
    assert_eq!(block.params, vec![b"x".as_slice()]);
    assert_eq!(
        block.body,
        vec![
            Statement::Let {
                name: b"y",
                value: Expr::Global(b"x"),
            },
            Statement::Expr(Expr::Global(b"y")),
        ]
    );
}

#[test]
fn lowers_program_bodies_with_local_scope() {
    let source = b"let x := 1; x";
    let program = node(
        b"program",
        0,
        source.len(),
        vec![node(
            b"statements",
            0,
            source.len(),
            vec![
                node(
                    b"statement",
                    0,
                    10,
                    vec![node(
                        b"letBinding",
                        0,
                        10,
                        vec![
                            node(b"ident", 4, 5, vec![]),
                            node(b"expr", 9, 10, vec![node(b"number", 9, 10, vec![])]),
                        ],
                    )],
                ),
                node(
                    b"statement",
                    12,
                    13,
                    vec![node(b"expr", 12, 13, vec![node(b"ident", 12, 13, vec![])])],
                ),
            ],
        )],
    );
    let lowered = lower_program(&program, source).unwrap();
    assert_eq!(
        lowered,
        Program {
            body: vec![
                Statement::Let {
                    name: b"x",
                    value: Expr::SmallIntLiteral(b"1"),
                },
                Statement::Expr(Expr::Global(b"x")),
            ],
        }
    );
}
