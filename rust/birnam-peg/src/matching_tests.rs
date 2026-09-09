use super::*;

struct LegacyMatcher {
    source: Vec<u8>,
    rules: Vec<(Oop, Oop)>,
    nodes: Vec<TempNode>,
    memo: HashMap<(usize, usize), MemoEntry>,
    furthest: usize,
    undefined_rule: Oop,
    left_recursive_rule: Oop,
}

fn note(matcher: &mut LegacyMatcher, position: usize) {
    matcher.furthest = matcher.furthest.max(position);
}
fn legacy_named_rule(rules: &[(Oop, Oop)], name: Oop) -> Option<Oop> {
    rules
        .iter()
        .find(|(rule, _)| *rule == name)
        .map(|(_, pattern)| *pattern)
}

fn legacy_match(matcher: &mut LegacyMatcher, pattern: Oop, position: usize) -> Option<MatchResult> {
    let state = unsafe { state_ref(pattern) };
    let kind = state.kind;
    let literal = state.literal.clone();
    let children = state.children.clone();
    let ranges = state.ranges.clone();
    let rule_name = state.rule_name;
    match kind {
        Kind::Literal => {
            if position + literal.len() > matcher.source.len()
                || matcher.source[position..position + literal.len()] != literal
            {
                note(matcher, position);
                return None;
            }
            Some(MatchResult {
                end: position + literal.len(),
                children: Vec::new(),
            })
        }
        Kind::Any => {
            if position >= matcher.source.len() {
                note(matcher, position);
                return None;
            }
            Some(MatchResult {
                end: position + 1,
                children: Vec::new(),
            })
        }
        Kind::Eof => {
            if position != matcher.source.len() {
                note(matcher, position);
                return None;
            }
            Some(MatchResult {
                end: position,
                children: Vec::new(),
            })
        }
        Kind::Charset => {
            let byte = matcher.source.get(position).copied();
            let Some(byte) = byte else {
                note(matcher, position);
                return None;
            };
            if ranges
                .iter()
                .any(|range| byte >= range.lo && byte <= range.hi)
            {
                Some(MatchResult {
                    end: position + 1,
                    children: Vec::new(),
                })
            } else {
                note(matcher, position);
                None
            }
        }
        Kind::Seq => {
            let mut at = position;
            let mut matched = Vec::new();
            for child in children {
                let result = legacy_match(matcher, child, at)?;
                matched.extend(result.children);
                at = result.end;
            }
            Some(MatchResult {
                end: at,
                children: matched,
            })
        }
        Kind::Choice => {
            for child in children {
                if let Some(result) = legacy_match(matcher, child, position) {
                    return Some(result);
                }
            }
            None
        }
        Kind::Star | Kind::Plus => {
            let child = children
                .first()
                .copied()
                .unwrap_or_else(|| abort("repetition has no child"));
            let mut at = position;
            let mut count = 0;
            let mut matched = Vec::new();
            while let Some(result) = legacy_match(matcher, child, at) {
                matched.extend(result.children);
                if result.end == at {
                    break;
                }
                at = result.end;
                count += 1;
            }
            if kind == Kind::Plus && count == 0 {
                return None;
            }
            Some(MatchResult {
                end: at,
                children: matched,
            })
        }
        Kind::Opt => {
            let child = children
                .first()
                .copied()
                .unwrap_or_else(|| abort("optional has no child"));
            match legacy_match(matcher, child, position) {
                Some(result) => Some(result),
                None => Some(MatchResult {
                    end: position,
                    children: Vec::new(),
                }),
            }
        }
        Kind::And => {
            let child = children
                .first()
                .copied()
                .unwrap_or_else(|| abort("lookahead has no child"));
            legacy_match(matcher, child, position).map(|_| MatchResult {
                end: position,
                children: Vec::new(),
            })
        }
        Kind::Not => {
            let child = children
                .first()
                .copied()
                .unwrap_or_else(|| abort("negation has no child"));
            match legacy_match(matcher, child, position) {
                Some(_) => None,
                None => Some(MatchResult {
                    end: position,
                    children: Vec::new(),
                }),
            }
        }
        Kind::RuleRef => {
            let rule_pattern = legacy_named_rule(&matcher.rules, rule_name).unwrap_or_else(|| {
                matcher.undefined_rule = rule_name;
                ptr::null_mut()
            });
            if rule_pattern.is_null() {
                return None;
            }
            let key = (rule_pattern as usize, position);
            if let Some(entry) = matcher.memo.get(&key) {
                if entry.in_progress {
                    matcher.left_recursive_rule = rule_name;
                    return None;
                }
                return if entry.ok {
                    Some(MatchResult {
                        end: entry.end,
                        children: entry.children.clone(),
                    })
                } else {
                    None
                };
            }
            matcher.memo.insert(
                key,
                MemoEntry {
                    ok: false,
                    end: position,
                    children: Vec::new(),
                    in_progress: true,
                },
            );
            if let Some(inner) = legacy_match(matcher, rule_pattern, position) {
                let node = matcher.nodes.len();
                matcher.nodes.push(TempNode {
                    rule: rule_name,
                    start: position,
                    end: inner.end,
                    children: inner.children,
                });
                let wrapped = vec![node];
                matcher.memo.insert(
                    key,
                    MemoEntry {
                        ok: true,
                        end: inner.end,
                        children: wrapped.clone(),
                        in_progress: false,
                    },
                );
                Some(MatchResult {
                    end: inner.end,
                    children: wrapped,
                })
            } else {
                matcher.memo.insert(
                    key,
                    MemoEntry {
                        ok: false,
                        end: position,
                        children: Vec::new(),
                        in_progress: false,
                    },
                );
                None
            }
        }
    }
}

fn matcher(source: &[u8], rules: &[(Oop, Oop)]) -> Matcher {
    Matcher {
        source: source.to_vec(),
        rules: index_rules(rules.iter().copied()),
        patterns: super::KeyMap::default(),
        nodes: Vec::new(),
        memo: super::KeyMap::default(),
        furthest: 0,
        undefined_rule: ptr::null_mut(),
        left_recursive_rule: ptr::null_mut(),
    }
}

fn compare(source: &[u8], root: Oop, rules: &[(Oop, Oop)]) -> Matcher {
    let mut actual = matcher(source, rules);
    let mut expected = LegacyMatcher {
        source: source.to_vec(),
        rules: rules.to_vec(),
        nodes: Vec::new(),
        memo: HashMap::new(),
        furthest: 0,
        undefined_rule: ptr::null_mut(),
        left_recursive_rule: ptr::null_mut(),
    };
    let a = match_pattern(&mut actual, root, 0).map(|r| (r.end, r.children));
    let b = legacy_match(&mut expected, root, 0).map(|r| (r.end, r.children));
    assert_eq!(a, b);
    assert_eq!(actual.furthest, expected.furthest);
    assert_eq!(actual.undefined_rule, expected.undefined_rule);
    assert_eq!(actual.left_recursive_rule, expected.left_recursive_rule);
    // Compare every allocated node, including discarded lookahead/choice nodes.
    let nodes = |nodes: &[TempNode]| {
        nodes
            .iter()
            .map(|n| (n.rule as usize, n.start, n.end, n.children.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(nodes(&actual.nodes), nodes(&expected.nodes));
    actual
}

// Boxes stand in for exact Pattern payloads. They never move, and outlive
// matching; these tests make no floor calls and do not fabricate heap objects.
fn pattern(
    arena: &mut Vec<Box<PatternState>>,
    kind: Kind,
    children: Vec<Oop>,
    literal: &[u8],
    name: usize,
) -> Oop {
    let mut value = Box::new(PatternState::new(kind));
    value.children = children;
    value.literal = literal.to_vec();
    value.rule_name = name as Oop;
    if kind == Kind::Charset {
        value.ranges.push(Range { lo: b'a', hi: b'b' });
    }
    let pointer = (&mut *value as *mut PatternState).cast();
    arena.push(value);
    pointer
}

#[test]
fn frozen_matcher_agrees_on_combinators_errors_and_aliases() {
    let mut arena = Vec::new();
    let a = pattern(&mut arena, Kind::Literal, vec![], b"a", 0);
    let b = pattern(&mut arena, Kind::Literal, vec![], b"b", 0);
    let empty = pattern(&mut arena, Kind::Literal, vec![], b"", 0);
    let any = pattern(&mut arena, Kind::Any, vec![], b"", 0);
    let eof = pattern(&mut arena, Kind::Eof, vec![], b"", 0);
    let charset = pattern(&mut arena, Kind::Charset, vec![], b"", 0);
    let undefined = pattern(&mut arena, Kind::RuleRef, vec![], b"", 99);
    let recursive = pattern(&mut arena, Kind::RuleRef, vec![], b"", 4);
    let alias1 = pattern(&mut arena, Kind::RuleRef, vec![], b"", 1);
    let alias2 = pattern(&mut arena, Kind::RuleRef, vec![], b"", 2);
    let rules = [(1 as Oop, a), (2 as Oop, a), (4 as Oop, recursive)];
    let leaves = [
        a, b, empty, any, eof, charset, undefined, recursive, alias1, alias2,
    ];
    let mut roots = leaves.to_vec();
    for child in leaves {
        for kind in [Kind::Star, Kind::Plus, Kind::Opt, Kind::And, Kind::Not] {
            roots.push(pattern(&mut arena, kind, vec![child], b"", 0));
        }
        for other in leaves {
            for kind in [Kind::Seq, Kind::Choice] {
                roots.push(pattern(&mut arena, kind, vec![child, other], b"", 0));
            }
        }
    }
    // Ordered choice/lookahead reuses the same pattern at the same position
    // through distinct rule names: preserve the legacy memo-label behavior.
    let look = pattern(&mut arena, Kind::And, vec![alias1], b"", 0);
    roots.push(pattern(&mut arena, Kind::Seq, vec![look, alias2], b"", 0));
    for root in roots {
        for source in [
            b"".as_slice(),
            b"a",
            b"b",
            b"ab",
            b"aa",
            b"ba",
            b"x",
            b"aba",
        ] {
            compare(source, root, &rules);
        }
    }
    let index = index_rules([(1 as Oop, a), (1 as Oop, b)]);
    assert_eq!(named_rule(&index, 1 as Oop), Some(a));
}

#[test]
fn snapshots_copy_only_visited_pattern_data_once_per_parse() {
    let mut arena = Vec::new();
    let literal = pattern(&mut arena, Kind::Literal, vec![], b"a", 0);
    let atom = pattern(&mut arena, Kind::RuleRef, vec![], b"", 1);
    let root = pattern(&mut arena, Kind::Star, vec![atom], b"", 0);
    let unused = pattern(&mut arena, Kind::Literal, vec![], &vec![b'z'; 4096], 0);
    let rules = [(1 as Oop, literal), (2 as Oop, unused)];
    for n in [1, 100, 1000] {
        let actual = compare(&vec![b'a'; n], root, &rules);
        assert_eq!(actual.patterns.len(), 3);
        assert_eq!(
            actual
                .patterns
                .values()
                .map(|p| p.literal.len())
                .sum::<usize>(),
            1
        );
        assert_eq!(
            actual
                .patterns
                .values()
                .map(|p| p.children.len())
                .sum::<usize>(),
            1
        );
        assert!(!actual.patterns.contains_key(&(unused as usize)));
    }
    // Mutation between invocations must be observed; a snapshot isn't a cache
    // on the long-lived Grammar or Pattern object.
    arena[0].literal[0] = b'b';
    let mut next = matcher(b"b", &rules);
    assert_eq!(match_pattern(&mut next, literal, 0).unwrap().end, 1);
    let mut next = matcher(b"a", &rules);
    assert!(match_pattern(&mut next, literal, 0).is_none());
}

#[test]
fn deep_acyclic_rules_keep_spans_and_names() {
    let mut arena = Vec::new();
    let mut rules = Vec::new();
    for i in 1..128 {
        let child = pattern(&mut arena, Kind::RuleRef, vec![], b"", i + 1);
        rules.push((i as Oop, child));
    }
    let leaf = pattern(&mut arena, Kind::Literal, vec![], b"x", 0);
    rules.push((128 as Oop, leaf));
    compare(b"x", rules[0].1, &rules);
}
