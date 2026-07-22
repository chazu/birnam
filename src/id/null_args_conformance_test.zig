//! Negative child-process probe for the IdApi send-buffer contract.

const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const NullableSend = *const fn (
    idapi.Oop,
    idapi.Oop,
    ?[*]const idapi.Oop,
    usize,
) callconv(.c) idapi.Oop;

pub fn main() void {
    const api = floor.api();
    const send: NullableSend = @ptrCast(api.send);
    _ = send(api.nilObj, api.intern("conformanceNullArgsRejected"), null, 1);
    @panic("IdApi.send accepted null args with nonzero count");
}
