#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Literal = 0,
    Any = 1,
    Eof = 2,
    Charset = 3,
    Seq = 4,
    Choice = 5,
    Star = 6,
    Plus = 7,
    Opt = 8,
    And = 9,
    Not = 10,
    RuleRef = 11,
}

#[repr(C)]
#[derive(Clone)]
struct Range {
    lo: u8,
    hi: u8,
}

#[repr(C)]
#[derive(Clone)]
struct PatternState {
    kind: Kind,
    literal: Vec<u8>,
    children: Vec<Oop>,
    ranges: Vec<Range>,
    rule_name: Oop,
}

struct TempNode {
    rule: Oop,
    start: usize,
    end: usize,
    children: Vec<usize>,
}

#[derive(Clone)]
struct MatchResult {
    end: usize,
    children: Vec<usize>,
}

#[derive(Clone)]
struct MemoEntry {
    ok: bool,
    end: usize,
    children: Vec<usize>,
    in_progress: bool,
}

/// Hashing for the parse-local tables keyed by pattern/rule addresses and
/// memo positions. These keys are process-local integers; the default SipHash
/// cost per pattern visit measured +6.5 ms on every stdlib boot (Design 71
/// acceptance repeat, 2026-09-09). The tables are discarded after each parse,
/// so no cross-process hash stability is needed.
#[derive(Default)]
struct KeyHasher(u64);

impl std::hash::Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    }

    fn write_usize(&mut self, value: usize) {
        let mixed = (self.0.rotate_left(29) ^ value as u64).wrapping_mul(0x9e3779b97f4a7c15);
        self.0 = mixed ^ (mixed >> 29);
    }
}

type KeyMap<Key, Value> = HashMap<Key, Value, std::hash::BuildHasherDefault<KeyHasher>>;

struct Matcher {
    source: Vec<u8>,
    rules: KeyMap<usize, Oop>,
    patterns: KeyMap<usize, std::rc::Rc<PatternState>>,
    nodes: Vec<TempNode>,
    memo: KeyMap<(usize, usize), MemoEntry>,
    furthest: usize,
    undefined_rule: Oop,
    left_recursive_rule: Oop,
}

impl PatternState {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            literal: Vec::new(),
            children: Vec::new(),
            ranges: Vec::new(),
            rule_name: ptr::null_mut(),
        }
    }
}

fn note(matcher: &mut Matcher, position: usize) {
    matcher.furthest = matcher.furthest.max(position);
}

fn index_rules(rules: impl IntoIterator<Item = (Oop, Oop)>) -> KeyMap<usize, Oop> {
    let mut index = KeyMap::default();
    for (name, pattern) in rules {
        index.entry(name as usize).or_insert(pattern);
    }
    index
}

fn named_rule(rules: &KeyMap<usize, Oop>, name: Oop) -> Option<Oop> {
    rules.get(&(name as usize)).copied()
}

fn match_pattern(
    matcher: &mut Matcher,
    pattern: Oop,
    position: usize,
    read: &impl Fn(Oop) -> Option<PatternState>,
) -> Option<MatchResult> {
    // Matching never calls the floor or user code. Keep each visited pattern's
    // immutable data once per parse, while allowing recursive matching to mutate
    // memo/CST scratch. Rc clones allocate nothing and hold no native-code roots.
    // The Grammar owns the original graph throughout this synchronous match.
    if !matcher.patterns.contains_key(&(pattern as usize)) {
        matcher
            .patterns
            .insert(pattern as usize, Rc::new(read(pattern)?));
    }
    let state = Rc::clone(matcher.patterns.get(&(pattern as usize))?);
    let kind = state.kind;
    let literal = &state.literal;
    let children = &state.children;
    let ranges = &state.ranges;
    let rule_name = state.rule_name;
    match kind {
        Kind::Literal => {
            if position + literal.len() > matcher.source.len()
                || matcher.source[position..position + literal.len()] != *literal
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
            for child in children.iter().copied() {
                let result = match_pattern(matcher, child, at, read)?;
                matched.extend(result.children);
                at = result.end;
            }
            Some(MatchResult {
                end: at,
                children: matched,
            })
        }
        Kind::Choice => {
            for child in children.iter().copied() {
                if let Some(result) = match_pattern(matcher, child, position, read) {
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
            while let Some(result) = match_pattern(matcher, child, at, read) {
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
            match match_pattern(matcher, child, position, read) {
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
            match_pattern(matcher, child, position, read).map(|_| MatchResult {
                end: position,
                children: Vec::new(),
            })
        }
        Kind::Not => {
            let child = children
                .first()
                .copied()
                .unwrap_or_else(|| abort("negation has no child"));
            match match_pattern(matcher, child, position, read) {
                Some(_) => None,
                None => Some(MatchResult {
                    end: position,
                    children: Vec::new(),
                }),
            }
        }
        Kind::RuleRef => {
            let rule_pattern = named_rule(&matcher.rules, rule_name).unwrap_or_else(|| {
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
            if let Some(inner) = match_pattern(matcher, rule_pattern, position, read) {
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

fn syntax_position(source: &[u8], offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for byte in source.iter().take(offset.min(source.len())) {
        if *byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}
