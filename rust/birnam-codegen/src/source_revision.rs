//! Existing source-revision recognition and digest rules, independent of runtime state.
use crate::{CstNode, OwnedCst, lower_program};
use sha2::{Digest, Sha256};
const MAX_CST_DEPTH: usize = 4096;
fn sha256_hex(bytes: &[u8]) -> [u8; 64] {
    let digest = Sha256::digest(bytes);
    let mut out = [0; 64];
    for (i, byte) in digest.iter().enumerate() {
        out[2 * i] = b"0123456789abcdef"[(byte >> 4) as usize];
        out[2 * i + 1] = b"0123456789abcdef"[(byte & 15) as usize];
    }
    out
}
/// Digest of the original source bytes, before embedding or lowering.
pub fn source_digest(source: &[u8]) -> [u8; 64] {
    sha256_hex(source)
}

/// Historical syntactic asset inventory: literal System embedFile: sends,
/// including nested blocks, in first-occurrence order. This does not read files
/// or decide whether a send will execute (or its receiver will be shadowed).
pub fn embedded_files(tree: &OwnedCst, source: &[u8]) -> Vec<Vec<u8>> {
    literal_paths(tree, source, b"System", b"embedFile:")
}
/// Literal native-library requirements retain their historical syntactic order.
pub fn native_requirements(tree: &OwnedCst, source: &[u8]) -> Vec<Vec<u8>> {
    literal_paths(tree, source, b"Alien", b"dlopen:")
}
fn literal_paths(
    tree: &OwnedCst,
    source: &[u8],
    receiver_name: &[u8],
    selector: &[u8],
) -> Vec<Vec<u8>> {
    fn path(
        node: &OwnedCst,
        source: &[u8],
        receiver_name: &[u8],
        selector: &[u8],
    ) -> Option<Vec<u8>> {
        if node.rule() != b"send" || node.child_count() != 2 {
            return None;
        }
        let receiver = skip_cst_wrappers(node.child(0)?)?;
        if receiver.rule() != b"ident" || source_span(receiver, source)? != receiver_name {
            return None;
        }
        let message = skip_cst_wrappers(node.child(1)?)?;
        if message.rule() != b"keywordMsg"
            || message.child_count() != 2
            || source_span(message.child(0)?, source)? != selector
        {
            return None;
        }
        let argument = skip_cst_wrappers(message.child(1)?)?;
        if argument.rule() != b"string" {
            return None;
        }
        let literal = source_span(argument, source)?;
        let body = literal.strip_prefix(b"\"")?.strip_suffix(b"\"")?;
        Some(crate::literal::decode_body(body))
    }
    let mut pending = vec![(tree, 0)];
    let mut paths = Vec::new();
    while let Some((node, depth)) = pending.pop() {
        if depth >= MAX_CST_DEPTH {
            continue;
        }
        if let Some(path) = path(node, source, receiver_name, selector)
            && !paths.contains(&path)
        {
            paths.push(path);
        }
        for i in (0..node.child_count()).rev() {
            if let Some(child) = node.child(i) {
                pending.push((child, depth + 1));
            }
        }
    }
    paths
}
fn skip_cst_wrappers(mut node: &OwnedCst) -> Option<&OwnedCst> {
    for _ in 0..MAX_CST_DEPTH {
        if matches!(node.rule(), b"expr" | b"primary" | b"message") && node.child_count() == 1 {
            node = node.child(0)?;
        } else {
            return Some(node);
        }
    }
    None
}

fn source_span<'a>(node: &OwnedCst, source: &'a [u8]) -> Option<&'a [u8]> {
    source.get(node.start()..node.end())
}

fn symbol_literal(node: &OwnedCst, source: &[u8]) -> Option<Vec<u8>> {
    let node = skip_cst_wrappers(node)?;
    source_span(node, source)?
        .strip_prefix(b"#")
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn vector_symbols(node: &OwnedCst, source: &[u8]) -> Option<Vec<Vec<u8>>> {
    let node = skip_cst_wrappers(node)?;
    if node.rule() != b"vectorLit" {
        return None;
    }
    let mut values = Vec::with_capacity(node.child_count());
    for index in 0..node.child_count() {
        values.push(symbol_literal(node.child(index)?, source)?);
    }
    Some(values)
}

fn keyword_selector_args<'a>(
    node: &'a OwnedCst,
    source: &[u8],
) -> Option<(Vec<u8>, Vec<&'a OwnedCst>)> {
    let node = skip_cst_wrappers(node)?;
    if node.rule() != b"keywordMsg" || node.child_count() == 0 || node.child_count() % 2 != 0 {
        return None;
    }
    let mut selector = Vec::new();
    let mut args = Vec::with_capacity(node.child_count() / 2);
    let mut index = 0;
    while index < node.child_count() {
        selector.extend_from_slice(source_span(node.child(index)?, source)?);
        args.push(node.child(index + 1)?);
        index += 2;
    }
    Some((selector, args))
}

/// Historical artifact inventory, separate from strict revision admission.
/// The last syntactic literal Module declaration determines artifact identity,
/// including declarations nested in Blocks; source registration stays stricter.
pub fn artifact_module(tree: &OwnedCst, source: &[u8]) -> Option<(Vec<u8>, Vec<Vec<u8>>)> {
    let mut pending = vec![(tree, 0)];
    let mut module = None;
    while let Some((node, depth)) = pending.pop() {
        if depth >= MAX_CST_DEPTH {
            continue;
        }
        if node.rule() == b"send"
            && node.child_count() == 2
            && let Some(receiver) = node.child(0).and_then(skip_cst_wrappers)
            && receiver.rule() == b"ident"
            && source_span(receiver, source) == Some(b"Module".as_slice())
            && let Some((selector, args)) =
                node.child(1).and_then(|n| keyword_selector_args(n, source))
            && selector == b"named:requires:"
            && args.len() == 2
            && let (Some(name), Some(requires)) = (
                symbol_literal(args[0], source),
                vector_symbols(args[1], source),
            )
        {
            module = Some((name, requires));
        }
        for index in (0..node.child_count()).rev() {
            if let Some(child) = node.child(index) {
                pending.push((child, depth + 1));
            }
        }
    }
    module
}

#[derive(Clone)]
pub struct RevisionOperation {
    pub target: Vec<u8>,
    pub selector: Vec<u8>,
    pub install: bool,
    pub source: Vec<u8>,
    /// Derived source semantics; excluded from the frozen revision digest.
    pub program: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct ProjectedGlobal {
    pub name: Vec<u8>,
    /// Derived canonical body; excluded from frozen revision digests.
    pub program: Vec<u8>,
}

#[derive(Clone)]
pub struct RevisionMetadata {
    pub name: Vec<u8>,
    pub requires: Vec<Vec<u8>>,
    pub operations: Vec<RevisionOperation>,
    /// Capture-free top-level Block Globals eligible for generation journals.
    pub projected_globals: Vec<ProjectedGlobal>,
    pub inventory: Vec<crate::resumable::ImplementationKey>,
    pub initializer_residual_digest: [u8; 64],
    pub non_method_digest: [u8; 64],
    pub method_digest: [u8; 64],
    pub revision_digest: [u8; 64],
}

fn append_digest_field(input: &mut Vec<u8>, bytes: &[u8]) {
    input.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    input.extend_from_slice(bytes);
}

use birnam_artifact::selector::arity as selector_arity;

fn block_parameter_count(node: &OwnedCst) -> Option<usize> {
    let node = skip_cst_wrappers(node)?;
    if node.rule() != b"block" {
        return None;
    }
    Some(
        (0..node.child_count())
            .filter_map(|index| node.child(index))
            .filter(|child| child.rule() == b"ident")
            .count(),
    )
}

/// Top-level declarative method definitions, retained verbatim for documentation.
pub fn method_sources(tree: &OwnedCst, source: &[u8]) -> Vec<Vec<u8>> {
    let mut result = Vec::new();
    let Some(program) = skip_cst_wrappers(tree) else {
        return result;
    };
    let Some(statements) = program.child(0) else {
        return result;
    };
    if program.rule() != b"program" || statements.rule() != b"statements" {
        return result;
    }
    for index in 0..statements.child_count() {
        let Some(node) = statements
            .child(index)
            .and_then(|s| s.child(0))
            .and_then(skip_cst_wrappers)
        else {
            continue;
        };
        if node.rule() != b"send" {
            continue;
        }
        let Some(receiver) = node.child(0).and_then(skip_cst_wrappers) else {
            continue;
        };
        if receiver.rule() != b"ident" || source_span(receiver, source) != Some(b"Reflect") {
            continue;
        }
        let Some((selector, args)) = node
            .child(1)
            .and_then(skip_cst_wrappers)
            .and_then(|m| keyword_selector_args(m, source))
        else {
            continue;
        };
        if (selector == b"defineMethod:on:as:" && args.len() == 3)
            || (selector == b"defineMethod:on:as:doc:" && args.len() == 4)
        {
            if let Some(bytes) = source_span(node, source) {
                result.push(bytes.to_vec());
            }
        }
    }
    result
}

pub fn inspect(tree: &OwnedCst, source: &[u8]) -> Option<RevisionMetadata> {
    let program = skip_cst_wrappers(tree)?;
    if program.rule() != b"program" || program.child_count() != 1 {
        return None;
    }
    let statements = program.child(0)?;
    if statements.rule() != b"statements" {
        return None;
    }
    let mut name = None;
    let mut requires = Vec::new();
    let mut operations = Vec::new();
    let mut non_method_spans = Vec::new();
    for index in 0..statements.child_count() {
        let statement = statements.child(index)?;
        if statement.child_count() == 0 {
            continue;
        }
        let whole = skip_cst_wrappers(statement.child(0)?)?;
        let whole_source = source_span(whole, source)?;
        if whole.rule() != b"send" || whole.child_count() != 2 {
            non_method_spans.push((index, whole_source.to_vec()));
            continue;
        }
        let receiver = skip_cst_wrappers(whole.child(0)?)?;
        let message = skip_cst_wrappers(whole.child(1)?)?;
        let Some((selector, args)) = keyword_selector_args(message, source) else {
            non_method_spans.push((index, whole_source.to_vec()));
            continue;
        };
        if receiver.rule() == b"ident"
            && source_span(receiver, source) == Some(b"Module".as_slice())
            && selector == b"named:requires:"
            && args.len() == 2
        {
            if name.is_some() {
                return None;
            }
            name = Some(symbol_literal(args[0], source)?);
            requires = vector_symbols(args[1], source)?;
            non_method_spans.push((index, whole_source.to_vec()));
            continue;
        }
        if receiver.rule() != b"ident"
            || source_span(receiver, source) != Some(b"Reflect".as_slice())
        {
            non_method_spans.push((index, whole_source.to_vec()));
            continue;
        }
        let is_define = (selector == b"defineMethod:on:as:" && args.len() == 3)
            || (selector == b"defineMethod:on:as:doc:" && args.len() == 4);
        let is_remove = selector == b"removeMethod:on:" && args.len() == 2;
        if !is_define && !is_remove {
            non_method_spans.push((index, whole_source.to_vec()));
            continue;
        }
        let operation_selector = symbol_literal(args[0], source)?;
        let target = skip_cst_wrappers(args[1])?;
        if target.rule() != b"ident" {
            return None;
        }
        let target = source_span(target, source)?.to_vec();
        if is_define {
            let parameters = block_parameter_count(args[2])?;
            if parameters != selector_arity(&operation_selector) + 1 {
                return None;
            }
        }
        if operations.iter().any(|operation: &RevisionOperation| {
            operation.target == target && operation.selector == operation_selector
        }) {
            return None;
        }
        operations.push(RevisionOperation {
            target,
            selector: operation_selector,
            install: is_define,
            source: whole_source.to_vec(),
            program: None,
        });
    }
    let name = name?;
    let normalized = lower_program(tree, source).ok()?;
    for (index, operation) in operations.iter_mut().enumerate() {
        if operation.install {
            let key = crate::resumable::ImplementationKey {
                root_kind: crate::resumable::InventoryRootKind::MethodOperation,
                root_index: u32::try_from(index).ok()?,
                path: vec![0],
                implementation_kind: crate::resumable::InventoryImplementationKind::BlockBody,
            };
            let resolved = crate::resumable::resolve_block_implementation(&normalized, &key)?;
            operation.program =
                Some(crate::portable::encode_block(resolved.block, &resolved.captures).ok()?);
        }
    }
    let projected = crate::resumable::projected_block_bindings(&normalized)
        .into_iter()
        .filter(|binding| binding.captures.is_empty())
        .map(|binding| {
            let resolved =
                crate::resumable::resolve_block_implementation(&normalized, &binding.key)?;
            Some((
                binding.statement_index,
                ProjectedGlobal {
                    name: binding.name,
                    program: crate::portable::encode_block(resolved.block, &resolved.captures)
                        .ok()?,
                },
            ))
        })
        .collect::<Option<std::collections::BTreeMap<_, _>>>()?;
    let mut residual_input = b"birnam.initializer-residual.v1\0".to_vec();
    let mut non_method_input = b"birnam.non-method.v1\0".to_vec();
    for (index, span) in non_method_spans {
        append_digest_field(&mut non_method_input, &span);
        if !projected.contains_key(&index) {
            append_digest_field(&mut residual_input, &span);
        }
    }
    let non_method_digest = sha256_hex(&non_method_input);

    let mut method_input = b"birnam.methods.v1\0".to_vec();
    for operation in &operations {
        method_input.push(if operation.install { b'I' } else { b'R' });
        append_digest_field(&mut method_input, &operation.target);
        append_digest_field(&mut method_input, &operation.selector);
        append_digest_field(&mut method_input, &operation.source);
    }
    let method_digest = sha256_hex(&method_input);

    let mut revision_input = b"birnam.source-revision.v1\0".to_vec();
    append_digest_field(&mut revision_input, &name);
    revision_input.extend_from_slice(&(requires.len() as u64).to_be_bytes());
    for requirement in &requires {
        append_digest_field(&mut revision_input, requirement);
    }
    append_digest_field(&mut revision_input, source);
    let revision_digest = sha256_hex(&revision_input);

    Some(RevisionMetadata {
        name,
        requires,
        operations,
        projected_globals: projected.into_values().collect(),
        inventory: crate::resumable::implementation_inventory(&normalized),
        initializer_residual_digest: sha256_hex(&residual_input),
        non_method_digest,
        method_digest,
        revision_digest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    // The normalized source has one module declaration, one ordinary initializer
    // and one projected Block. Change each independently at the revision boundary.
    fn revision(seed: &str, body: &str) -> RevisionMetadata {
        let source = format!(
            "[Module named: #unit requires: #[]]; let seed := {seed}; let saved := [| {body}]"
        );
        let node = |rule: &[u8], start, end, children| OwnedCst::new(rule, start, end, children);
        let token = |rule: &[u8], text: &str| {
            let i = source.find(text).unwrap();
            node(rule, i, i + text.len(), vec![])
        };
        let expr = |child: OwnedCst| node(b"expr", child.start(), child.end(), vec![child]);
        let stmt = |child: OwnedCst| node(b"statement", child.start(), child.end(), vec![child]);
        let end = source.find(';').unwrap();
        let declaration = stmt(expr(node(
            b"send",
            0,
            end,
            vec![
                token(b"ident", "Module"),
                node(
                    b"keywordMsg",
                    8,
                    end - 1,
                    vec![
                        token(b"keyword", "named:"),
                        token(b"symbol", "#unit"),
                        token(b"keyword", "requires:"),
                        token(b"vectorLit", "#[]"),
                    ],
                ),
            ],
        )));
        let seed_start = source.find("let seed").unwrap();
        let seed_value = source.find(":= ").unwrap() + 3;
        let binding = stmt(node(
            b"letBinding",
            seed_start,
            seed_value + seed.len(),
            vec![
                token(b"ident", "seed"),
                expr(node(b"number", seed_value, seed_value + seed.len(), vec![])),
            ],
        ));
        let saved_start = source.find("let saved").unwrap();
        let block_start = source.find("[|").unwrap();
        let body_start = block_start + 3;
        let value = node(
            if body.as_bytes()[0].is_ascii_digit() {
                b"number"
            } else {
                b"ident"
            },
            body_start,
            body_start + body.len(),
            vec![],
        );
        let block = node(
            b"block",
            block_start,
            source.len(),
            vec![node(
                b"statements",
                body_start,
                body_start + body.len(),
                vec![stmt(expr(value))],
            )],
        );
        let projected = stmt(node(
            b"letBinding",
            saved_start,
            source.len(),
            vec![token(b"ident", "saved"), expr(block)],
        ));
        let tree = node(
            b"program",
            0,
            source.len(),
            vec![node(
                b"statements",
                0,
                source.len(),
                vec![declaration, binding, projected],
            )],
        );
        inspect(&tree, source.as_bytes()).unwrap()
    }
    #[test]
    fn residual_digest_excludes_only_capture_free_projected_blocks() {
        let baseline = revision("7", "11");
        let changed = revision("7", "99");
        assert_ne!(baseline.revision_digest, changed.revision_digest);
        assert_ne!(baseline.non_method_digest, changed.non_method_digest);
        assert_eq!(
            baseline.initializer_residual_digest,
            changed.initializer_residual_digest
        );
        assert_eq!(baseline.inventory.len(), 1);
        assert!(crate::generation::transition_allowed(&baseline, &changed));
        assert!(!crate::generation::transition_allowed(
            &baseline,
            &revision("8", "11")
        ));
        let captured = revision("7", "seed");
        assert!(!crate::generation::transition_allowed(&captured, &changed));
    }
}
