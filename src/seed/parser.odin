package seed

import "core:fmt"
import "core:strings"
import "../abi"

// Token kinds exist only in the disposable bootstrap parser. Parsed syntax
// and runtime values are objects, evaluated through their current Methods.
Token_Kind :: enum { End, Name, Keyword, Number, Text, Symbol, Open, Close, Vector, Colon, Bar, Assign, Separator, Operator }
Token :: struct { kind: Token_Kind, text: string, start: int }
Parser :: struct { source: Oop, text: string, offset: int, token: Token, error: Oop, depth: int }

parser_error :: proc(p: ^Parser, kind, message: string, offset: int) {
	if p.error != nil { return }
	line, column := 1, 1
	for ch in (transmute([]u8)p.text)[:min(offset, len(p.text))] {
		if ch == '\n' { line += 1; column = 1 } else { column += 1 }
	}
	description := fmt.aprintf("%s at byte %d (line %d, byte column %d)", message, offset, line, column)
	defer delete(description)
	p.error = fail(kind, description)
	p.token = Token{kind=.End, start=offset}
}
alpha :: proc "contextless" (ch: u8) -> bool { return (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || ch == '_' }
digit :: proc "contextless" (ch: u8) -> bool { return ch >= '0' && ch <= '9' }
operator :: proc "contextless" (ch: u8) -> bool {
	for op in "+-*/<>=~!&,%@^|" { if ch == u8(op) { return true } }
	return false
}
advance :: proc(p: ^Parser) {
	if p.error != nil { return }
	n := len(p.text)
	for p.offset < n {
		ch := p.text[p.offset]
		if ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n' { p.offset += 1; continue }
		if ch == '/' && p.offset+1 < n && p.text[p.offset+1] == '/' {
			for p.offset < n && p.text[p.offset] != '\n' { p.offset += 1 }
			continue
		}
		break
	}
	start := p.offset
	if start == n { p.token = Token{kind=.End, start=start}; return }
	ch := p.text[start]
	p.offset += 1
	kind: Token_Kind
	switch ch {
	case '[': kind = .Open
	case ']': kind = .Close
	case '|': kind = .Bar
	case ';': kind = .Separator
	case ':':
		kind = .Colon
		if p.offset < n && p.text[p.offset] == '=' { p.offset += 1; kind = .Assign }
	case '#':
		if p.offset < n && p.text[p.offset] == '[' { p.offset += 1; kind = .Vector; break }
		if p.offset < n && alpha(p.text[p.offset]) {
			for p.offset < n && (alpha(p.text[p.offset]) || digit(p.text[p.offset]) || p.text[p.offset] == ':') { p.offset += 1 }
		} else {
			for p.offset < n && operator(p.text[p.offset]) { p.offset += 1 }
		}
		if p.offset == start+1 { parser_error(p, "syntaxError", "expected Symbol name", start); return }
		p.token = Token{.Symbol, p.text[start+1:p.offset], start}
		return
	case '"':
		content := p.offset
		for p.offset < n {
			if p.text[p.offset] == '"' {
				p.token = Token{.Text, p.text[content:p.offset], start}
				p.offset += 1
				return
			}
			if p.text[p.offset] == '\\' { p.offset += 1 }
			if p.offset < n { p.offset += 1 }
		}
		parser_error(p, "syntaxError", "unterminated String", start)
		return
	case '{', '}', '?':
		parser_error(p, "unsupportedSyntax", "form is outside the seed syntax subset", start)
		return
	case:
		if digit(ch) || (ch == '-' && p.offset < n && digit(p.text[p.offset])) {
			for p.offset < n && digit(p.text[p.offset]) { p.offset += 1 }
			if p.offset < n && p.text[p.offset] == '.' {
				parser_error(p, "unsupportedSyntax", "Float literals are not supported by the seed", start)
				return
			}
			if p.offset < n && alpha(p.text[p.offset]) {
				parser_error(p, "unsupportedSyntax", "numeric prefixes, suffixes, and exponents are outside the seed subset", start)
				return
			}
			kind = .Number
		} else if alpha(ch) {
			for p.offset < n && (alpha(p.text[p.offset]) || digit(p.text[p.offset])) { p.offset += 1 }
			kind = .Name
			if p.offset < n && p.text[p.offset] == ':' && !(p.offset+1 < n && p.text[p.offset+1] == '=') {
				p.offset += 1; kind = .Keyword
			}
		} else if operator(ch) {
			for p.offset < n && operator(p.text[p.offset]) { p.offset += 1 }
			kind = .Operator
		} else {
			parser_error(p, "syntaxError", "unexpected source byte", start)
			return
		}
	}
	p.token = Token{kind, p.text[start:p.offset], start}
}
expect :: proc(p: ^Parser, kind: Token_Kind, message: string) -> bool {
	if p.error != nil { return false }
	if p.token.kind != kind { parser_error(p, "syntaxError", message, p.token.start); return false }
	advance(p)
	return p.error == nil
}

decode_string :: proc(p: ^Parser, token: Token) -> Oop {
	result := allocate(birnam_bytes_behavior(), uintptr(len(token.text)))
	dest := cast([^]u8)result
	i, out := 0, 0
	for i < len(token.text) {
		ch := token.text[i]
		i += 1
		if ch == '\\' {
			if i == len(token.text) { parser_error(p, "syntaxError", "unfinished escape", token.start+i); return abi.known.nil_value }
			ch = token.text[i]; i += 1
			switch ch {
			case 'n': ch = '\n'
			case 'r': ch = '\r'
			case 't': ch = '\t'
			case '"', '\\':
			case: parser_error(p, "syntaxError", "unsupported String escape", token.start+i); return abi.known.nil_value
			}
		}
		dest[out] = ch; out += 1
	}
	header(result).payload_bytes = uintptr(out)
	return result
}

parse_sequence :: proc(p: ^Parser, closing: bool) -> Oop {
	forms: List
	for p.error == nil && p.token.kind != .End && !(closing && p.token.kind == .Close) {
		if p.token.kind == .Separator { advance(p); continue }
		if p.token.kind == .Name && p.token.text == "let" {
			advance(p)
			if p.token.kind != .Name { parser_error(p, "syntaxError", "let requires a name", p.token.start); break }
			name := symbol(p.token.text)
			advance(p)
			if !expect(p, .Assign, "let requires :=") { break }
			expression := parse_expression(p)
			if p.error != nil { break }
			node := allocate(state.let_vt, size_of(Declaration))
			(cast(^Declaration)node)^ = Declaration{name, expression}
			append(&forms, node)
		} else {
			node := parse_expression(p)
			if p.error != nil { break }
			append(&forms, node)
		}
	}
	result := allocate(state.sequence_vt, size_of(Sequence))
	(cast(^Sequence)result)^ = Sequence{vector(forms), p.source}
	return result
}

parse_expression :: proc(p: ^Parser) -> Oop {
	if p.error != nil { return abi.known.nil_value }
	p.depth += 1
	defer p.depth -= 1
	if p.depth > 512 { parser_error(p, "sourceTooDeep", "seed source nesting limit exceeded", p.token.start); return abi.known.nil_value }
	token := p.token
	advance(p)
	#partial switch token.kind {
	case .Number:
		_, ok := parse_decimal(token.text)
		if !ok { parser_error(p, "integerRange", "Integer literal exceeds signed 64-bit range", token.start); return abi.known.nil_value }
		return literal(state.integer, new_bytes(token.text))
	case .Text:
		return literal(state.text, decode_string(p, token))
	case .Symbol:
		return literal(state.symbol_factory, new_bytes(token.text))
	case .Name:
		result := allocate(state.name_vt, size_of(Name))
		(cast(^Name)result).name = symbol(token.text)
		return result
	case .Vector:
		elements: List
		for p.error == nil && p.token.kind != .Close && p.token.kind != .End {
			append(&elements, parse_expression(p))
		}
		if !expect(p, .Close, "unterminated Vector") { return abi.known.nil_value }
		result := allocate(state.vector_expr_vt, size_of(Vector_Expression))
		(cast(^Vector_Expression)result).elements = vector(elements)
		return result
	case .Open:
		if p.token.kind == .Colon || p.token.kind == .Bar {
			parameters: List
			for p.error == nil && p.token.kind == .Colon {
				advance(p)
				if p.token.kind != .Name { parser_error(p, "syntaxError", "expected Block parameter", p.token.start); return abi.known.nil_value }
				name := symbol(p.token.text)
				for row := parameters.head; row != nil; row = row.next {
					if row.value == name { parser_error(p, "duplicateBinding", "duplicate Block parameter", p.token.start); return abi.known.nil_value }
				}
				append(&parameters, name)
				advance(p)
			}
			if !expect(p, .Bar, "Block requires | after parameters") { return abi.known.nil_value }
			body := parse_sequence(p, true)
			if !expect(p, .Close, "unterminated Block") { return abi.known.nil_value }
			result := allocate(state.block_expr_vt, size_of(Block_Expression))
			(cast(^Block_Expression)result)^ = Block_Expression{vector(parameters), body}
			return result
		}
		receiver := parse_expression(p)
		if p.error != nil { return abi.known.nil_value }
		selector: Oop
		values: List
		#partial switch p.token.kind {
		case .Name:
			selector = symbol(p.token.text)
			advance(p)
		case .Operator, .Bar:
			if p.token.text == "=>" { parser_error(p, "unsupportedSyntax", "result-threaded sends are not supported by the seed", p.token.start); return abi.known.nil_value }
			selector = symbol(p.token.text)
			advance(p)
			append(&values, parse_expression(p))
		case .Keyword:
			builder := strings.builder_make()
			defer strings.builder_destroy(&builder)
			for p.error == nil && p.token.kind == .Keyword {
				strings.write_string(&builder, p.token.text)
				advance(p)
				append(&values, parse_expression(p))
			}
			selector = symbol(strings.to_string(builder))
		case:
			parser_error(p, "syntaxError", "send requires a selector", p.token.start)
			return abi.known.nil_value
		}
		if !expect(p, .Close, "send must end after one dispatch") { return abi.known.nil_value }
		result := allocate(state.send_vt, size_of(Send))
		(cast(^Send)result)^ = Send{receiver, selector, vector(values)}
		return result
	case:
		parser_error(p, "syntaxError", "expected expression", token.start)
		return abi.known.nil_value
	}
}

parse_source :: proc(source: Oop) -> Oop {
	data := as_bytes(source)
	if is_error(data) { return data }
	p := Parser{source=data, text=native_text(data)}
	if offset := utf8_error_offset(p.text); offset >= 0 {
		parser_error(&p, "invalidUtf8", "source is not valid UTF-8", offset)
		return p.error
	}
	advance(&p)
	result := parse_sequence(&p, false)
	return p.error if p.error != nil else result
}
