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
    fn path(node: &OwnedCst, source: &[u8]) -> Option<Vec<u8>> {
        if node.rule() != b"send" || node.child_count() != 2 {
            return None;
        }
        let receiver = skip_cst_wrappers(node.child(0)?)?;
        if receiver.rule() != b"ident" || source_span(receiver, source)? != b"System" {
            return None;
        }
        let message = skip_cst_wrappers(node.child(1)?)?;
        if message.rule() != b"keywordMsg"
            || message.child_count() != 2
            || source_span(message.child(0)?, source)? != b"embedFile:"
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
        if let Some(path) = path(node, source)
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

#[derive(Clone)]
struct RevisionOperation {
    target: Vec<u8>,
    selector: Vec<u8>,
    install: bool,
    source: Vec<u8>,
}

#[derive(Clone)]
pub struct RevisionMetadata {
    pub name: Vec<u8>,
    pub requires: Vec<Vec<u8>>,
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
        });
    }
    let name = name?;
    lower_program(tree, source).ok()?;
    let mut non_method_input = b"birnam.non-method.v1\0".to_vec();
    for (_, span) in non_method_spans {
        append_digest_field(&mut non_method_input, &span);
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
        non_method_digest,
        method_digest,
        revision_digest,
    })
}
