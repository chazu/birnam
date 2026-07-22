//! Process-isolated proof that image-style Store replacement rebuilds the MOP cache.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const store = @import("store.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

fn mirrorValue(api: *idapi.IdApi, vtable: Oop, selector: Oop) Oop {
    return api.send(
        store.store_instance,
        api.intern("rawCurrentValue:attr:"),
        &.{ vtable, selector },
        2,
    );
}

test "image-style Store replacement rebuilds primitive MOP presence" {
    const api = floor.api();
    store.bringUp(api);
    const clone = api.intern("clone");
    const primitive = api.intern("primitive");

    store.backfillMirror(api);
    try std.testing.expect(mirrorValue(api, api.object_vt, clone) == primitive);

    _ = api.send(store.store_instance, api.intern("imageLoadBegin"), &.{}, 0);
    try std.testing.expect(mirrorValue(api, api.object_vt, clone) == null);
    _ = api.send(
        store.store_instance,
        api.intern("imageLoadEnd:raw:"),
        &.{ api.mkSmallInt(0), api.falseObj },
        2,
    );

    store.backfillMirror(api);
    try std.testing.expect(mirrorValue(api, api.object_vt, clone) == primitive);
}
