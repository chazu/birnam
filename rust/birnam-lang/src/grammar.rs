use super::*;
unsafe fn literal(api: &Build<'_>, peg: Oop, bytes: &[u8]) -> Result<Oop, Outcome> {
    let string = unsafe { string(api, bytes)? };
    unsafe { send1(api, peg, b"literal:\0", string) }
}

unsafe fn rule_ref(api: &Build<'_>, peg: Oop, name: &[u8]) -> Result<Oop, Outcome> {
    let symbol = unsafe { intern(api, name)? };
    unsafe { send1(api, peg, b"ruleRef:\0", symbol) }
}

unsafe fn any(api: &Build<'_>, peg: Oop) -> Result<Oop, Outcome> {
    unsafe { send0(api, peg, b"any\0") }
}

unsafe fn eof(api: &Build<'_>, peg: Oop) -> Result<Oop, Outcome> {
    unsafe { send0(api, peg, b"eof\0") }
}

unsafe fn star(api: &Build<'_>, pattern: Oop) -> Result<Oop, Outcome> {
    unsafe { send0(api, pattern, b"star\0") }
}

unsafe fn plus(api: &Build<'_>, pattern: Oop) -> Result<Oop, Outcome> {
    unsafe { send0(api, pattern, b"plus\0") }
}

unsafe fn optional(api: &Build<'_>, pattern: Oop) -> Result<Oop, Outcome> {
    unsafe { send0(api, pattern, b"opt\0") }
}

unsafe fn negate(api: &Build<'_>, pattern: Oop) -> Result<Oop, Outcome> {
    unsafe { send0(api, pattern, b"negate\0") }
}

unsafe fn seq(api: &Build<'_>, peg: Oop, children: &[Oop]) -> Result<Oop, Outcome> {
    let result = unsafe { send0(api, peg, b"seq\0")? };
    for child in children {
        let _ = unsafe { send1(api, result, b"add:\0", *child)? };
    }
    Ok(result)
}

unsafe fn choice(api: &Build<'_>, peg: Oop, children: &[Oop]) -> Result<Oop, Outcome> {
    let result = unsafe { send0(api, peg, b"choice\0")? };
    for child in children {
        let _ = unsafe { send1(api, result, b"add:\0", *child)? };
    }
    Ok(result)
}

unsafe fn charset(api: &Build<'_>, peg: Oop, ranges: &[(u8, u8)]) -> Result<Oop, Outcome> {
    let result = unsafe { send0(api, peg, b"charset\0")? };
    for (lo, hi) in ranges {
        let _ = unsafe {
            send2(
                api,
                result,
                b"from:to:\0",
                integer(api, *lo as usize)?,
                integer(api, *hi as usize)?,
            )?
        };
    }
    Ok(result)
}

unsafe fn define_rule(
    api: &Build<'_>,
    grammar: Oop,
    name: &[u8],
    pattern: Oop,
) -> Result<(), Outcome> {
    let symbol = unsafe { intern(api, name)? };
    let _ = unsafe { send2(api, grammar, b"rule:is:\0", symbol, pattern)? };
    Ok(())
}

pub(super) unsafe fn build_grammar(api: &Build<'_>, peg: Oop) -> Result<Oop, Outcome> {
    let grammar = unsafe { send0(api, peg, b"grammar\0")? };

    let newline = unsafe { charset(api, peg, &[(b'\n', b'\n')])? };
    let comment = unsafe {
        seq(
            api,
            peg,
            &[
                literal(api, peg, b"//")?,
                star(
                    api,
                    seq(api, peg, &[negate(api, newline)?, any(api, peg)?])?,
                )?,
            ],
        )?
    };
    let space_char = unsafe {
        charset(
            api,
            peg,
            &[(b' ', b' '), (b'\t', b'\t'), (b'\n', b'\n'), (b'\r', b'\r')],
        )?
    };
    let whitespace = unsafe { star(api, choice(api, peg, &[space_char, comment])?)? };
    let spacing1 = unsafe { plus(api, choice(api, peg, &[space_char, comment])?)? };
    let alpha = unsafe { charset(api, peg, &[(b'a', b'z'), (b'A', b'Z'), (b'_', b'_')])? };
    let alnum = unsafe {
        charset(
            api,
            peg,
            &[(b'a', b'z'), (b'A', b'Z'), (b'0', b'9'), (b'_', b'_')],
        )?
    };
    let digit = unsafe { charset(api, peg, &[(b'0', b'9')])? };

    unsafe {
        define_rule(
            api,
            grammar,
            b"ident\0",
            seq(api, peg, &[alpha, star(api, alnum)?])?,
        )?;
        define_rule(
            api,
            grammar,
            b"number\0",
            seq(
                api,
                peg,
                &[optional(api, literal(api, peg, b"-")?)?, plus(api, digit)?],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"float\0",
            seq(
                api,
                peg,
                &[
                    optional(api, literal(api, peg, b"-")?)?,
                    plus(api, digit)?,
                    literal(api, peg, b".")?,
                    plus(api, digit)?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"string\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"\"")?,
                    star(
                        api,
                        choice(
                            api,
                            peg,
                            &[
                                seq(api, peg, &[literal(api, peg, b"\\")?, any(api, peg)?])?,
                                seq(
                                    api,
                                    peg,
                                    &[negate(api, literal(api, peg, b"\"")?)?, any(api, peg)?],
                                )?,
                            ],
                        )?,
                    )?,
                    literal(api, peg, b"\"")?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"binop\0",
            charset(
                api,
                peg,
                &[
                    (b'=', b'='),
                    (b'<', b'<'),
                    (b'>', b'>'),
                    (b'&', b'&'),
                    (b'|', b'|'),
                    (b',', b','),
                    (b'+', b'+'),
                    (b'-', b'-'),
                    (b'*', b'*'),
                    (b'/', b'/'),
                    (b'~', b'~'),
                    (b'%', b'%'),
                    (b'@', b'@'),
                    (b'!', b'!'),
                    (b'^', b'^'),
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"symbol\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"#")?,
                    choice(
                        api,
                        peg,
                        &[
                            plus(
                                api,
                                seq(
                                    api,
                                    peg,
                                    &[rule_ref(api, peg, b"ident\0")?, literal(api, peg, b":")?],
                                )?,
                            )?,
                            rule_ref(api, peg, b"ident\0")?,
                            plus(api, rule_ref(api, peg, b"binop\0")?)?,
                        ],
                    )?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"primary\0",
            choice(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"float\0")?,
                    rule_ref(api, peg, b"number\0")?,
                    rule_ref(api, peg, b"string\0")?,
                    rule_ref(api, peg, b"symbol\0")?,
                    rule_ref(api, peg, b"ident\0")?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"keyword\0",
            seq(
                api,
                peg,
                &[rule_ref(api, peg, b"ident\0")?, literal(api, peg, b":")?],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"keywordMsg\0",
            plus(
                api,
                seq(
                    api,
                    peg,
                    &[
                        rule_ref(api, peg, b"keyword\0")?,
                        whitespace,
                        rule_ref(api, peg, b"expr\0")?,
                        whitespace,
                    ],
                )?,
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"binops\0",
            plus(api, rule_ref(api, peg, b"binop\0")?)?,
        )?;
        define_rule(
            api,
            grammar,
            b"binaryMsg\0",
            seq(
                api,
                peg,
                &[
                    negate(
                        api,
                        choice(
                            api,
                            peg,
                            &[
                                seq(api, peg, &[literal(api, peg, b"=>")?, spacing1])?,
                                seq(
                                    api,
                                    peg,
                                    &[
                                        literal(api, peg, b"=>")?,
                                        negate(api, rule_ref(api, peg, b"binop\0")?)?,
                                    ],
                                )?,
                            ],
                        )?,
                    )?,
                    rule_ref(api, peg, b"binops\0")?,
                    whitespace,
                    rule_ref(api, peg, b"expr\0")?,
                ],
            )?,
        )?;
        define_rule(api, grammar, b"unaryMsg\0", rule_ref(api, peg, b"ident\0")?)?;
        define_rule(
            api,
            grammar,
            b"threadDispatch\0",
            choice(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"keywordMsg\0")?,
                    rule_ref(api, peg, b"binaryMsg\0")?,
                    rule_ref(api, peg, b"unaryMsg\0")?,
                ],
            )?,
        )?;
        let thread_leg = seq(
            api,
            peg,
            &[
                literal(api, peg, b"=>")?,
                spacing1,
                rule_ref(api, peg, b"threadDispatch\0")?,
            ],
        )?;
        define_rule(
            api,
            grammar,
            b"threadedMsg\0",
            seq(
                api,
                peg,
                &[
                    thread_leg,
                    star(api, seq(api, peg, &[whitespace, thread_leg])?)?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"message\0",
            choice(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"threadedMsg\0")?,
                    rule_ref(api, peg, b"keywordMsg\0")?,
                    plus(
                        api,
                        seq(api, peg, &[rule_ref(api, peg, b"binaryMsg\0")?, whitespace])?,
                    )?,
                    plus(
                        api,
                        seq(api, peg, &[rule_ref(api, peg, b"unaryMsg\0")?, whitespace])?,
                    )?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"send\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"[")?,
                    whitespace,
                    rule_ref(api, peg, b"expr\0")?,
                    whitespace,
                    rule_ref(api, peg, b"message\0")?,
                    whitespace,
                    literal(api, peg, b"]")?,
                ],
            )?,
        )?;
        let block_header = seq(
            api,
            peg,
            &[
                star(
                    api,
                    seq(
                        api,
                        peg,
                        &[
                            literal(api, peg, b":")?,
                            rule_ref(api, peg, b"ident\0")?,
                            whitespace,
                        ],
                    )?,
                )?,
                literal(api, peg, b"|")?,
            ],
        )?;
        define_rule(
            api,
            grammar,
            b"block\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"[")?,
                    whitespace,
                    block_header,
                    whitespace,
                    optional(api, rule_ref(api, peg, b"statements\0")?)?,
                    whitespace,
                    literal(api, peg, b"]")?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"vectorLit\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"#[")?,
                    whitespace,
                    star(
                        api,
                        seq(api, peg, &[rule_ref(api, peg, b"expr\0")?, whitespace])?,
                    )?,
                    literal(api, peg, b"]")?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"mapEntry\0",
            seq(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"expr\0")?,
                    whitespace,
                    literal(api, peg, b"->")?,
                    whitespace,
                    rule_ref(api, peg, b"expr\0")?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"mapLit\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"{")?,
                    whitespace,
                    star(
                        api,
                        seq(
                            api,
                            peg,
                            &[
                                rule_ref(api, peg, b"mapEntry\0")?,
                                whitespace,
                                optional(api, literal(api, peg, b",")?)?,
                                whitespace,
                            ],
                        )?,
                    )?,
                    literal(api, peg, b"}")?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"logicVar\0",
            seq(
                api,
                peg,
                &[literal(api, peg, b"?")?, rule_ref(api, peg, b"ident\0")?],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"expr\0",
            choice(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"block\0")?,
                    rule_ref(api, peg, b"send\0")?,
                    rule_ref(api, peg, b"vectorLit\0")?,
                    rule_ref(api, peg, b"mapLit\0")?,
                    rule_ref(api, peg, b"logicVar\0")?,
                    rule_ref(api, peg, b"primary\0")?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"letBinding\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"let")?,
                    negate(api, alnum)?,
                    whitespace,
                    rule_ref(api, peg, b"ident\0")?,
                    whitespace,
                    literal(api, peg, b":=")?,
                    whitespace,
                    rule_ref(api, peg, b"expr\0")?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"statement\0",
            choice(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"letBinding\0")?,
                    rule_ref(api, peg, b"expr\0")?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"statements\0",
            seq(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"statement\0")?,
                    star(
                        api,
                        seq(
                            api,
                            peg,
                            &[
                                whitespace,
                                // The separator is optional: every expr form is
                                // self-delimiting (bracketed group or single
                                // token) and nothing juxtaposes at statement
                                // level, so whitespace alone already separates.
                                optional(api, literal(api, peg, b";")?)?,
                                whitespace,
                                rule_ref(api, peg, b"statement\0")?,
                            ],
                        )?,
                    )?,
                    optional(api, seq(api, peg, &[whitespace, literal(api, peg, b";")?])?)?,
                ],
            )?,
        )?;
        define_rule(
            api,
            grammar,
            b"program\0",
            seq(
                api,
                peg,
                &[
                    whitespace,
                    optional(api, rule_ref(api, peg, b"statements\0")?)?,
                    whitespace,
                    eof(api, peg)?,
                ],
            )?,
        )?;
    }
    Ok(grammar)
}
