//! Digla-local accelerator for reflection and source/display projection.
//!
//! This application module preserves Birnam-level contracts while moving two
//! measured hot paths behind the existing IdApi: method reflection uses a
//! basis-keyed reverse-global index, terminal frames scan source bytes once, and
//! evaluation-local System output becomes copied session events. Opt-in deadline
//! checks wrap the canonical Block and Integer loop protocols. It deliberately
//! lives outside the language/runtime floor.

const std = @import("std");
const idapi = @import("idapi");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

var g_api: *idapi.IdApi = undefined;
var g_store: Oop = null;
var g_globals: Oop = null;
var sel_current_value: Oop = null;
var sel_raw_current_value: Oop = null;
var sel_live_triples_do: Oop = null;
var global_collector_vt: Oop = null;
var global_names: std.AutoHashMapUnmanaged(Oop, Oop) = .{};
var global_names_basis: i64 = -1;

const max_output_bytes = 256 * 1024;
const output_truncated_suffix = "\n[Digla output truncated at 262144 bytes]\n";
var output_capture_active = false;
var output_bytes: std.ArrayListUnmanaged(u8) = .{};
var output_truncated = false;
var output_sink: Oop = null;
var output_sequence: usize = 0;
var evaluation_deadline_active = false;
var evaluation_deadline_start: std.time.Instant = undefined;
var evaluation_deadline_ns: u64 = 0;
var error_vt: Oop = null;

fn send0(receiver: Oop, selector: [*:0]const u8) Oop {
    return g_api.send(receiver, g_api.intern(selector), &.{}, 0);
}

fn send1(receiver: Oop, selector: [*:0]const u8, arg: Oop) Oop {
    return g_api.send(receiver, g_api.intern(selector), &[_]Oop{arg}, 1);
}

fn mapPut(map: Oop, key: Oop, value: Oop) Oop {
    return g_api.send(map, g_api.intern("at:put:"), &[_]Oop{ key, value }, 2);
}

fn string(value: []const u8) Oop {
    return g_api.mkString(value.ptr, value.len);
}

fn mapPutString(map: Oop, key: []const u8, value: Oop) Oop {
    return mapPut(map, string(key), value);
}

fn newMap() Oop {
    return send0(g_api.import("Map"), "new");
}

fn newVector() Oop {
    return send0(g_api.import("Vector"), "new");
}

fn vectorAdd(vector: Oop, value: Oop) Oop {
    return send1(vector, "add:", value);
}

fn integer(value: usize) Oop {
    return g_api.mkSmallInt(@intCast(value));
}

fn clearOutput() void {
    output_bytes.clearRetainingCapacity();
    output_truncated = false;
    output_sequence = 0;
}

fn printableBytes(value: Oop) []const u8 {
    const rendered = if (value != null and g_api.vtableOf(value) == g_api.string_vt)
        value
    else
        send0(value, "printString");
    if (rendered == null or g_api.vtableOf(rendered) != g_api.string_vt) return "";
    var len: usize = 0;
    return g_api.stringBytes(rendered, &len)[0..len];
}

fn captureOutput(value: Oop, newline: bool) []const u8 {
    if (output_truncated) return &.{};
    const start = output_bytes.items.len;
    const bytes = printableBytes(value);
    const requested = bytes.len + @intFromBool(newline);
    const content_limit = max_output_bytes - output_truncated_suffix.len;
    const available = content_limit - output_bytes.items.len;
    if (requested <= available) {
        output_bytes.appendSlice(ca, bytes) catch @panic("digla-reflect: oom");
        if (newline) output_bytes.append(ca, '\n') catch @panic("digla-reflect: oom");
        return output_bytes.items[start..];
    }

    const content_available = content_limit - output_bytes.items.len;
    output_bytes.appendSlice(ca, bytes[0..@min(bytes.len, content_available)]) catch
        @panic("digla-reflect: oom");
    if (newline and bytes.len < content_available)
        output_bytes.append(ca, '\n') catch @panic("digla-reflect: oom");
    output_bytes.appendSlice(ca, output_truncated_suffix) catch @panic("digla-reflect: oom");
    output_truncated = true;
    return output_bytes.items[start..];
}

fn streamCaptured(bytes: []const u8) void {
    if (output_sink == null or bytes.len == 0) return;
    output_sequence += 1;
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);
    var record = newMap();
    record = mapPutString(record, "kind", string("output"));
    record = mapPutString(record, "channel", string("stdout"));
    record = mapPutString(record, "text", string(bytes));
    record = mapPutString(record, "sequence", integer(output_sequence));
    record = mapPutString(record, "truncated", if (output_truncated) g_api.trueObj else g_api.falseObj);
    _ = send1(output_sink, "value:", record);
}

fn writeOutput(value: Oop, newline: bool) void {
    const bytes = printableBytes(value);
    std.fs.File.stdout().writeAll(bytes) catch {};
    if (newline) std.fs.File.stdout().writeAll("\n") catch {};
}

fn mSystemPrint(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    if (output_capture_active) streamCaptured(captureOutput(args[0], false)) else writeOutput(args[0], false);
    return self;
}

fn mSystemPrintln(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    if (output_capture_active) streamCaptured(captureOutput(args[0], true)) else writeOutput(args[0], true);
    return self;
}

fn mOutputCaptureAvailable(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return g_api.trueObj;
}

fn mBeginOutputCapture(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    clearOutput();
    output_capture_active = true;
    return self;
}

fn mEndOutputCapture(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    output_capture_active = false;
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);
    defer clearOutput();
    var result = newVector();
    if (output_bytes.items.len != 0) {
        var record = newMap();
        record = mapPutString(record, "kind", string("output"));
        record = mapPutString(record, "channel", string("stdout"));
        record = mapPutString(record, "text", string(output_bytes.items));
        record = mapPutString(record, "sequence", integer(1));
        record = mapPutString(record, "truncated", if (output_truncated) g_api.trueObj else g_api.falseObj);
        result = vectorAdd(result, record);
    }
    return result;
}

fn mOutputStreamingAvailable(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return g_api.trueObj;
}

fn mBeginOutputStreaming(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    output_sink = args[0];
    return self;
}

fn mEndOutputStreaming(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    output_sink = null;
    return self;
}

fn mEvaluationDeadlineAvailable(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return g_api.trueObj;
}

fn mBeginEvaluationDeadline(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    evaluation_deadline_active = false;
    const timeout_ms = g_api.smallIntValue(args[0]);
    if (timeout_ms <= 0) return self;
    evaluation_deadline_start = std.time.Instant.now() catch return self;
    const max_ms = std.math.maxInt(u64) / std.time.ns_per_ms;
    evaluation_deadline_ns = @min(@as(u64, @intCast(timeout_ms)), max_ms) * std.time.ns_per_ms;
    evaluation_deadline_active = true;
    return self;
}

fn mEndEvaluationDeadline(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    evaluation_deadline_active = false;
    return self;
}

fn deadlineExpired(iteration: usize) bool {
    if (!evaluation_deadline_active or iteration & 255 != 0) return false;
    const now = std.time.Instant.now() catch return false;
    return now.since(evaluation_deadline_start) >= evaluation_deadline_ns;
}

fn isError(value: Oop) bool {
    return error_vt != null and value != null and g_api.vtableOf(value) == error_vt;
}

fn evaluationDeadlineError() Oop {
    return g_api.send(
        g_api.import("Error"),
        g_api.intern("kind:message:"),
        &[_]Oop{
            g_api.intern("evaluationDeadline"),
            string("evaluation exceeded its cooperative loop deadline"),
        },
        2,
    );
}

fn cooperativeWhile(condition: Oop, body: Oop, continue_on: Oop) Oop {
    var iteration: usize = 0;
    while (true) : (iteration +%= 1) {
        if (deadlineExpired(iteration)) return evaluationDeadlineError();
        const value = send0(condition, "value");
        if (isError(value)) return value;
        if (value != g_api.trueObj and value != g_api.falseObj) {
            return g_api.send(
                g_api.import("Error"),
                g_api.intern("kind:message:"),
                &[_]Oop{
                    g_api.intern("mustBeBoolean"),
                    string("while loop condition must answer true or false"),
                },
                2,
            );
        }
        if (value != continue_on) return g_api.nilObj;
        const result = send0(body, "value");
        if (isError(result)) return result;
    }
}

fn mBlockWhileTrue(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    return cooperativeWhile(self, args[0], g_api.trueObj);
}

fn mBlockWhileFalse(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    return cooperativeWhile(self, args[0], g_api.falseObj);
}

fn mIntegerToDo(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    const end = g_api.smallIntValue(args[0]);
    const block = args[1];
    var value = g_api.smallIntValue(self);
    var iteration: usize = 0;
    while (value <= end) : ({
        value += 1;
        iteration +%= 1;
    }) {
        if (deadlineExpired(iteration)) return evaluationDeadlineError();
        const result = send1(block, "value:", g_api.mkSmallInt(value));
        if (isError(result)) return result;
    }
    return self;
}

fn utf8Width(first: u8) usize {
    if (first < 0x80) return 1;
    if (first < 0xe0) return 2;
    if (first < 0xf0) return 3;
    return 4;
}

fn codepointAt(bytes: []const u8, start: usize) u21 {
    const first = bytes[start];
    const width = @min(utf8Width(first), bytes.len - start);
    if (width == 1) return first;
    if (width == 2)
        return @intCast((@as(u32, first & 0x1f) << 6) | @as(u32, bytes[start + 1] & 0x3f));
    if (width == 3)
        return @intCast((@as(u32, first & 0x0f) << 12) |
            (@as(u32, bytes[start + 1] & 0x3f) << 6) |
            @as(u32, bytes[start + 2] & 0x3f));
    return @intCast((@as(u32, first & 0x07) << 18) |
        (@as(u32, bytes[start + 1] & 0x3f) << 12) |
        (@as(u32, bytes[start + 2] & 0x3f) << 6) |
        @as(u32, bytes[start + 3] & 0x3f));
}

fn inRange(cp: u21, low: u21, high: u21) bool {
    return cp >= low and cp <= high;
}

fn codepointCellWidth(bytes: []const u8, start: usize, column: usize) usize {
    const first = bytes[start];
    if (first == '\t') return 4 - (column % 4);
    if (first < 32) return 0;
    const cp = codepointAt(bytes, start);
    const combining = inRange(cp, 768, 879) or inRange(cp, 6832, 6911) or
        inRange(cp, 7424, 7679) or inRange(cp, 8400, 8431) or
        inRange(cp, 65056, 65071);
    if (combining) return 0;
    const wide = inRange(cp, 4352, 4607) or inRange(cp, 9001, 9002) or
        inRange(cp, 11904, 42191) or inRange(cp, 44032, 55203) or
        inRange(cp, 63744, 64255) or inRange(cp, 65040, 65071) or
        inRange(cp, 65281, 65376) or inRange(cp, 65504, 65510) or
        inRange(cp, 127744, 129791);
    return if (wide) 2 else 1;
}

fn displayWidth(bytes: []const u8, start: usize, end: usize) usize {
    var byte = start;
    var column: usize = 0;
    while (byte < end) {
        column += codepointCellWidth(bytes, byte, column);
        byte += @min(utf8Width(bytes[byte]), end - byte);
    }
    return column;
}

fn isWordByte(byte: u8) bool {
    return (byte >= '0' and byte <= '9') or (byte >= 'A' and byte <= 'Z') or
        (byte >= 'a' and byte <= 'z') or byte == '_' or byte == '?';
}

fn wordEnd(bytes: []const u8, start: usize) usize {
    var end = start;
    while (end < bytes.len and isWordByte(bytes[end])) : (end += 1) {}
    return end;
}

fn highlightKind(word: []const u8, colon: bool) []const u8 {
    const keywords = [_][]const u8{ "let", "ifTrue", "ifFalse", "do", "value", "on", "as", "doc" };
    for (keywords) |keyword| if (std.mem.eql(u8, word, keyword)) return "keyword";
    if (std.mem.eql(u8, word, "true") or std.mem.eql(u8, word, "false") or
        std.mem.eql(u8, word, "nil")) return "literal";
    return if (colon) "selector" else "identifier";
}

fn highlightEntry(kind: []const u8, start: usize, end: usize) Oop {
    var entry = newMap();
    entry = mapPutString(entry, "kind", string(kind));
    entry = mapPutString(entry, "start", integer(start));
    entry = mapPutString(entry, "end", integer(end));
    return entry;
}

fn highlightsFor(bytes: []const u8) Oop {
    var out = newVector();
    var i: usize = 0;
    while (i < bytes.len) {
        const start = i;
        const byte = bytes[i];
        if (byte == '/' and i + 1 < bytes.len and bytes[i + 1] == '/') {
            i += 2;
            while (i < bytes.len and bytes[i] != '\n') : (i += 1) {}
            out = vectorAdd(out, highlightEntry("comment", start, i));
        } else if (byte == '"') {
            i += 1;
            while (i < bytes.len) {
                if (bytes[i] == '\\') {
                    i = @min(i + 2, bytes.len);
                } else if (bytes[i] == '"') {
                    i += 1;
                    break;
                } else if (bytes[i] == '\n') {
                    i = bytes.len;
                    break;
                } else {
                    i += 1;
                }
            }
            out = vectorAdd(out, highlightEntry("string", start, i));
        } else if (byte == '#') {
            i = if (i + 1 < bytes.len) wordEnd(bytes, i + 1) else i + 1;
            out = vectorAdd(out, highlightEntry("symbol", start, i));
        } else if (byte >= '0' and byte <= '9') {
            var dot = false;
            while (i < bytes.len) {
                const current = bytes[i];
                if (current >= '0' and current <= '9') {
                    i += 1;
                } else if (current == '.' and !dot) {
                    dot = true;
                    i += 1;
                } else break;
            }
            out = vectorAdd(out, highlightEntry("number", start, i));
        } else if (isWordByte(byte)) {
            const finish = wordEnd(bytes, i);
            const colon = finish < bytes.len and bytes[finish] == ':';
            i = finish + @intFromBool(colon);
            out = vectorAdd(out, highlightEntry(highlightKind(bytes[start..finish], colon), start, i));
        } else if (byte == ' ' or byte == '\t' or byte == '\n' or byte == '\r') {
            i += 1;
        } else {
            i += 1;
            out = vectorAdd(out, highlightEntry("punctuation", start, i));
        }
    }
    return out;
}

fn layoutRow(bytes: []const u8, line: usize, start: usize, end: usize, prompt_width: usize) Oop {
    var row = newMap();
    row = mapPutString(row, "line", integer(line));
    row = mapPutString(row, "sourceStart", integer(start));
    row = mapPutString(row, "sourceEnd", integer(end));
    row = mapPutString(row, "text", string(bytes[start..end]));
    row = mapPutString(row, "promptWidth", integer(prompt_width));
    row = mapPutString(row, "cellWidth", integer(displayWidth(bytes, start, end)));
    return row;
}

fn layoutFor(bytes: []const u8, prompt_width: usize) Oop {
    var rows = newVector();
    var line: usize = 0;
    var start: usize = 0;
    while (true) {
        var end = start;
        while (end < bytes.len and bytes[end] != '\n') : (end += 1) {}
        rows = vectorAdd(rows, layoutRow(bytes, line, start, end, prompt_width));
        if (end == bytes.len) break;
        start = end + 1;
        line += 1;
    }
    var layout = newMap();
    layout = mapPutString(layout, "lines", rows);
    layout = mapPutString(layout, "promptWidth", integer(prompt_width));
    layout = mapPutString(layout, "sourceSize", integer(bytes.len));
    return layout;
}

fn wrappedEnd(bytes: []const u8, start: usize, end: usize, requested_width: usize) usize {
    const width = @max(requested_width, 1);
    var byte = start;
    var column: usize = 0;
    while (byte < end) {
        const glyph_width = codepointCellWidth(bytes, byte, column);
        if (column > 0 and column + glyph_width > width) break;
        column += glyph_width;
        byte += @min(utf8Width(bytes[byte]), end - byte);
    }
    return byte;
}

fn wrappedLayoutRow(
    bytes: []const u8,
    row_number: usize,
    line_number: usize,
    start: usize,
    end: usize,
    line_start: usize,
    line_end: usize,
    prompt_width: usize,
) Oop {
    var row = newMap();
    row = mapPutString(row, "row", integer(row_number));
    row = mapPutString(row, "line", integer(line_number));
    row = mapPutString(row, "sourceStart", integer(start));
    row = mapPutString(row, "sourceEnd", integer(end));
    row = mapPutString(row, "text", string(bytes[start..end]));
    row = mapPutString(row, "promptWidth", integer(prompt_width));
    row = mapPutString(row, "cellWidth", integer(displayWidth(bytes, start, end)));
    row = mapPutString(row, "continuation", if (start > line_start) g_api.trueObj else g_api.falseObj);
    row = mapPutString(row, "wrapsAfter", if (end < line_end) g_api.trueObj else g_api.falseObj);
    return row;
}

const WrappedLayout = struct { layout: Oop, position: Oop };

fn wrappedLayoutFor(
    bytes: []const u8,
    requested_cursor: usize,
    prompt_width: usize,
    terminal_width: usize,
) WrappedLayout {
    const cursor = @min(requested_cursor, bytes.len);
    const content_width = if (terminal_width > prompt_width) terminal_width - prompt_width else 1;
    var rows = newVector();
    var position: Oop = null;
    var row_number: usize = 0;
    var line_number: usize = 0;
    var line_start: usize = 0;
    var last_start: usize = 0;
    var last_end: usize = 0;
    while (true) {
        var line_end = line_start;
        while (line_end < bytes.len and bytes[line_end] != '\n') : (line_end += 1) {}
        if (line_start == line_end) {
            rows = vectorAdd(rows, wrappedLayoutRow(
                bytes,
                row_number,
                line_number,
                line_start,
                line_end,
                line_start,
                line_end,
                prompt_width,
            ));
            if (position == null and cursor == line_start) {
                position = newMap();
                position = mapPutString(position, "line", integer(row_number));
                position = mapPutString(position, "x", integer(prompt_width));
            }
            last_start = line_start;
            last_end = line_end;
            row_number += 1;
        } else {
            var chunk_start = line_start;
            while (chunk_start < line_end) {
                const chunk_end = wrappedEnd(bytes, chunk_start, line_end, content_width);
                const wraps_after = chunk_end < line_end;
                rows = vectorAdd(rows, wrappedLayoutRow(
                    bytes,
                    row_number,
                    line_number,
                    chunk_start,
                    chunk_end,
                    line_start,
                    line_end,
                    prompt_width,
                ));
                const belongs = cursor >= chunk_start and
                    (cursor < chunk_end or (cursor == chunk_end and !wraps_after));
                if (position == null and belongs) {
                    position = newMap();
                    position = mapPutString(position, "line", integer(row_number));
                    position = mapPutString(
                        position,
                        "x",
                        integer(prompt_width + displayWidth(bytes, chunk_start, cursor)),
                    );
                }
                last_start = chunk_start;
                last_end = chunk_end;
                row_number += 1;
                chunk_start = chunk_end;
            }
        }
        if (line_end == bytes.len) break;
        line_start = line_end + 1;
        line_number += 1;
    }
    if (position == null) {
        position = newMap();
        position = mapPutString(position, "line", integer(row_number - 1));
        position = mapPutString(position, "x", integer(prompt_width + displayWidth(bytes, last_start, last_end)));
    }
    var layout = newMap();
    layout = mapPutString(layout, "lines", rows);
    layout = mapPutString(layout, "promptWidth", integer(prompt_width));
    layout = mapPutString(layout, "sourceSize", integer(bytes.len));
    layout = mapPutString(layout, "width", integer(terminal_width));
    return .{ .layout = layout, .position = position };
}

fn positionFor(bytes: []const u8, requested_cursor: usize, prompt_width: usize) Oop {
    const cursor = @min(requested_cursor, bytes.len);
    var line: usize = 0;
    var start: usize = 0;
    for (bytes[0..cursor], 0..) |byte, i| {
        if (byte == '\n') {
            line += 1;
            start = i + 1;
        }
    }
    var position = newMap();
    position = mapPutString(position, "line", integer(line));
    position = mapPutString(position, "x", integer(prompt_width + displayWidth(bytes, start, cursor)));
    return position;
}

fn mRenderFrame(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);
    var len: usize = 0;
    const bytes = g_api.stringBytes(args[0], &len)[0..len];
    const cursor: usize = @intCast(@max(g_api.smallIntValue(args[1]), 0));
    const prompt_width: usize = @intCast(@max(g_api.smallIntValue(args[2]), 0));
    var frame = newMap();
    frame = mapPutString(frame, "layout", layoutFor(bytes, prompt_width));
    frame = mapPutString(frame, "position", positionFor(bytes, cursor, prompt_width));
    frame = mapPutString(frame, "highlights", highlightsFor(bytes));
    return frame;
}

fn mRenderWrappedFrame(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);
    var len: usize = 0;
    const bytes = g_api.stringBytes(args[0], &len)[0..len];
    const cursor: usize = @intCast(@max(g_api.smallIntValue(args[1]), 0));
    const prompt_width: usize = @intCast(@max(g_api.smallIntValue(args[2]), 0));
    const terminal_width: usize = @intCast(@max(g_api.smallIntValue(args[3]), 1));
    const wrapped = wrappedLayoutFor(bytes, cursor, prompt_width, terminal_width);
    var frame = newMap();
    frame = mapPutString(frame, "layout", wrapped.layout);
    frame = mapPutString(frame, "position", wrapped.position);
    frame = mapPutString(frame, "highlights", highlightsFor(bytes));
    return frame;
}

fn mDisplayColumn(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    var len: usize = 0;
    const bytes = g_api.stringBytes(args[0], &len)[0..len];
    const requested_cursor: usize = @intCast(@max(g_api.smallIntValue(args[1]), 0));
    const cursor = @min(requested_cursor, bytes.len);
    var start = cursor;
    while (start > 0 and bytes[start - 1] != '\n') : (start -= 1) {}
    return integer(displayWidth(bytes, start, cursor));
}

fn mDisplayWidth(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    var len: usize = 0;
    const bytes = g_api.stringBytes(args[0], &len)[0..len];
    const requested_start: usize = @intCast(@max(g_api.smallIntValue(args[1]), 0));
    const requested_end: usize = @intCast(@max(g_api.smallIntValue(args[2]), 0));
    const start = @min(requested_start, bytes.len);
    const end = @max(start, @min(requested_end, bytes.len));
    return integer(displayWidth(bytes, start, end));
}

fn mWrapTranscriptLine(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);
    var len: usize = 0;
    const bytes = g_api.stringBytes(args[0], &len)[0..len];
    const requested_width = g_api.smallIntValue(args[1]);
    const width: usize = @intCast(@max(requested_width, 1));
    var lines = newVector();
    var start: usize = 0;
    var byte: usize = 0;
    var column: usize = 0;
    while (byte < bytes.len) {
        const glyph_width = codepointCellWidth(bytes, byte, column);
        if (column > 0 and column + glyph_width > width) {
            lines = vectorAdd(lines, string(bytes[start..byte]));
            start = byte;
            column = 0;
        } else {
            byte += @min(utf8Width(bytes[byte]), bytes.len - byte);
            column += glyph_width;
        }
    }
    lines = vectorAdd(lines, string(bytes[start..bytes.len]));
    return lines;
}

fn mRuntimeNamed(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    var len: usize = 0;
    const bytes = g_api.stringBytes(args[0], &len);
    const name = ca.allocSentinel(u8, len, 0) catch @panic("digla-reflect: oom");
    defer ca.free(name);
    @memcpy(name[0..len], bytes[0..len]);

    const symbol = g_api.intern(name.ptr);
    const global = g_api.send(g_store, sel_raw_current_value, &[_]Oop{ g_globals, symbol }, 2);
    if (global != null) return global;
    return g_api.import(name.ptr) orelse g_api.nilObj;
}

fn mCollectGlobal(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    if (args[0] == g_globals and args[2] != g_api.nilObj and !global_names.contains(args[2]))
        global_names.put(ca, args[2], args[1]) catch @panic("digla-reflect: oom");
    return g_api.nilObj;
}

fn refreshGlobalNames() void {
    global_names.clearRetainingCapacity();
    const collector = g_api.alloc(global_collector_vt, 0);
    _ = g_api.send(g_store, sel_live_triples_do, &[_]Oop{collector}, 1);
    global_names_basis = g_api.smallIntValue(send0(g_store, "basis"));
}

fn globalNameForValue(value: Oop) Oop {
    if (value == null or value == g_api.nilObj) return g_api.nilObj;
    const basis = g_api.smallIntValue(send0(g_store, "basis"));
    if (basis != global_names_basis) refreshGlobalNames();
    return global_names.get(value) orelse g_api.nilObj;
}

fn exportedSymbolForValue(value: Oop) Oop {
    if (value == null) return g_api.nilObj;
    if (g_api.exportedNameOf(value)) |name| return g_api.intern(name);
    return g_api.nilObj;
}

fn methodTargetName(vtable: Oop) Oop {
    const owner = g_api.vtableOwner(vtable);
    if (owner != null) {
        const exported_owner = exportedSymbolForValue(owner);
        if (exported_owner != g_api.nilObj) return exported_owner;
        const global_owner = globalNameForValue(owner);
        if (global_owner != g_api.nilObj) return global_owner;
    }

    const exported_vtable = exportedSymbolForValue(vtable);
    if (exported_vtable != g_api.nilObj) return exported_vtable;
    return globalNameForValue(vtable);
}

fn methodDescriptor(vtable: Oop, selector: Oop, target_name: Oop) Oop {
    const mirror = g_api.send(g_store, sel_current_value, &[_]Oop{ vtable, selector }, 2);
    const primitive = g_api.intern("primitive");
    const method = if (mirror == g_api.nilObj)
        primitive
    else if (mirror == primitive or g_api.objectPayloadSize(mirror) != 0)
        mirror
    else
        g_api.intern("compiled");

    var descriptor = send0(g_api.import("Map"), "new");
    descriptor = mapPut(descriptor, g_api.intern("selector"), selector);
    descriptor = mapPut(descriptor, g_api.intern("owner"), g_api.vtableOwner(vtable) orelse g_api.nilObj);
    descriptor = mapPut(descriptor, g_api.intern("target"), target_name);
    descriptor = mapPut(descriptor, g_api.intern("method"), method);
    return descriptor;
}

fn mAllMethodsOf(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);

    var result = send0(g_api.import("Vector"), "new");
    var seen_selectors: std.AutoHashMapUnmanaged(Oop, void) = .{};
    defer seen_selectors.deinit(ca);
    var seen_vtables: std.AutoHashMapUnmanaged(Oop, void) = .{};
    defer seen_vtables.deinit(ca);

    var vtable = g_api.vtableOf(args[0]);
    while (vtable != null and !seen_vtables.contains(vtable)) : (vtable = g_api.vtableParent(vtable)) {
        seen_vtables.put(ca, vtable, {}) catch @panic("digla-reflect: oom");
        const target_name = methodTargetName(vtable);

        var i: usize = 0;
        while (g_api.vtableSelectorAt(vtable, i)) |selector| : (i += 1) {
            if (seen_selectors.contains(selector)) continue;
            seen_selectors.put(ca, selector, {}) catch @panic("digla-reflect: oom");
            result = send1(result, "add:", methodDescriptor(vtable, selector, target_name));
        }
    }
    return result;
}

fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    g_store = api.import("Store");
    g_globals = api.intern("__globals__");
    sel_current_value = api.intern("currentValue:attr:");
    sel_raw_current_value = api.intern("rawCurrentValue:attr:");
    sel_live_triples_do = api.intern("liveTriplesDo:");
    error_vt = api.import("ErrorVt");
    global_collector_vt = api.proto(api.object_vt);
    api.method(global_collector_vt, api.intern("value:value:value:"), &mCollectGlobal);
    const reflect = api.import("Reflect");
    api.method(api.vtableOf(reflect), api.intern("allMethodsOf:"), &mAllMethodsOf);
    const system = api.import("System");
    api.method(api.vtableOf(system), api.intern("print:"), &mSystemPrint);
    api.method(api.vtableOf(system), api.intern("println:"), &mSystemPrintln);
    api.method(api.import("BlockVt"), api.intern("whileTrue:"), &mBlockWhileTrue);
    api.method(api.import("BlockVt"), api.intern("whileFalse:"), &mBlockWhileFalse);
    api.method(api.smallint_vt, api.intern("to:do:"), &mIntegerToDo);
    const digla = api.send(g_store, sel_current_value, &[_]Oop{ g_globals, api.intern("Digla") }, 2);
    api.method(api.vtableOf(digla), api.intern("outputCaptureAvailable"), &mOutputCaptureAvailable);
    api.method(api.vtableOf(digla), api.intern("beginEvaluationOutputCapture"), &mBeginOutputCapture);
    api.method(api.vtableOf(digla), api.intern("endEvaluationOutputCapture"), &mEndOutputCapture);
    api.method(api.vtableOf(digla), api.intern("evaluationOutputStreamingAvailable"), &mOutputStreamingAvailable);
    api.method(api.vtableOf(digla), api.intern("beginEvaluationOutputStreaming:"), &mBeginOutputStreaming);
    api.method(api.vtableOf(digla), api.intern("endEvaluationOutputStreaming"), &mEndOutputStreaming);
    api.method(api.vtableOf(digla), api.intern("evaluationDeadlineAvailable"), &mEvaluationDeadlineAvailable);
    api.method(api.vtableOf(digla), api.intern("beginEvaluationDeadline:"), &mBeginEvaluationDeadline);
    api.method(api.vtableOf(digla), api.intern("endEvaluationDeadline"), &mEndEvaluationDeadline);
    api.method(api.vtableOf(digla), api.intern("runtimeNamed:"), &mRuntimeNamed);
    api.method(api.vtableOf(digla), api.intern("renderFrameFor:cursor:promptWidth:"), &mRenderFrame);
    api.method(api.vtableOf(digla), api.intern("renderFrameFor:cursor:promptWidth:width:"), &mRenderWrappedFrame);
    api.method(api.vtableOf(digla), api.intern("displayColumnIn:at:"), &mDisplayColumn);
    api.method(api.vtableOf(digla), api.intern("displayWidthIn:from:to:"), &mDisplayWidth);
    api.method(api.vtableOf(digla), api.intern("wrapTranscriptLine:width:"), &mWrapTranscriptLine);
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}

comptime {
    @export(&moduleInit, .{ .name = "module_init" });
}
