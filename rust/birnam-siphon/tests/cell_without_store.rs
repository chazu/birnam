//! Actual Cell initialization and exported owner programs on an Oop-only floor.
#[path = "support/minimal.rs"]
#[allow(dead_code)]
mod minimal;
use birnam_abi::{Oop, as_size, size};
use minimal::H;
use std::path::{Path, PathBuf};
fn current_accessor_dispatch(h: &H<'_>, fixture: Oop, cell: Oop, model: Oop, gc: Oop) {
    let method = |n: &[u8], c| {
        h.call(
            h.call(fixture, b"methodAt:", &[h.key(n)]),
            b"withContext:",
            &[c],
        )
    };
    let nil = h.named(b"nil");
    let watched = h.call(cell, b"with:", &[nil]);
    let result = h.integer(93);
    let vt = h.call(h.host.id.root, b"behaviorOf:", &[model]);
    let selector = h.key(b"slot:in:");
    let alias = h.key(b"cellOriginalRead:in:");
    let prior = h.call(vt, b"lookup", &[selector]);
    h.call(vt, b"addMethod", &[alias, prior]);
    let state = h.call(
        h.host.id.root,
        b"sequence",
        &[alias, gc, watched, result, size(0).unwrap()],
    );
    for _ in 0..128 {
        assert_eq!(h.call(watched, b"value", &[]), nil);
    }
    h.call(vt, b"addMethod", &[selector, method(b"read", state)]);
    assert_eq!(h.call(watched, b"value", &[]), result);
    assert_eq!(as_size(h.call(state, b"at:", &[size(4).unwrap()])), Some(1));
    h.call(vt, b"addMethod", &[selector, prior]);
    h.call(vt, b"removeMethod:", &[alias]);
    assert_eq!(h.call(watched, b"value", &[]), nil);
    let factory = h.call(cell, b"clone", &[]);
    let target = h.call(cell, b"clone", &[]);
    let fv = h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[factory]);
    let tv = h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[target]);
    let state = h.call(
        h.host.id.root,
        b"sequence",
        &[target, nil, size(0).unwrap(), size(0).unwrap()],
    );
    h.call(
        fv,
        b"addMethod",
        &[h.key(b"clone"), method(b"clone", state)],
    );
    h.call(tv, b"addMethod", &[h.key(b"set:"), method(b"set", state)]);
    assert_eq!(h.call(factory, b"with:", &[result]), target);
    assert_eq!(h.call(state, b"at:", &[size(1).unwrap()]), result);
    for index in [2, 3] {
        assert_eq!(
            as_size(h.call(state, b"at:", &[size(index).unwrap()])),
            Some(1)
        );
    }
    h.call(fv, b"removeMethod:", &[h.key(b"clone")]);
    h.call(tv, b"removeMethod:", &[h.key(b"set:")]);
}
#[test]
#[ignore = "scripts/cell-without-store-test.py supplies the isolated artifact floor and exported programs"]
fn authored_cells_mutate_and_collect_without_store() {
    let directory = PathBuf::from(std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let programs = PathBuf::from(std::env::var_os("BIRNAM_CELL_PROGRAMS").unwrap());
    for name in ["store", "relational", "bootstrap", "lang", "generation"] {
        assert!(!directory.join(format!("libbirnam_{name}.dylib")).exists());
    }
    let host = minimal::native_floor();
    let h = H::new(&host);
    let q = host.scope();
    let ns = host.namespace;
    let prototype = h.named(b"Object");
    let vt = h.named(b"ObjectVt");
    let collection = h.named(b"Collection");
    let model = h.named(b"ObjectModel");
    let gc = h.named(b"GC");
    let nil = h.named(b"nil");
    h.artifact("error");
    let globals = h.artifact("globalsOwner");
    let cell = h.artifact("cell");
    assert_eq!(h.call(globals, b"at:", &[h.key(b"Cell")]), cell);
    assert_eq!(h.artifact("cell"), cell);
    let load_fixture = |name: &str| {
        let path = std::env::temp_dir().join(format!("birnam-{name}-{}.dylib", std::process::id()));
        assert!(
            std::process::Command::new("clang")
                .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join(format!("../../tests/fixtures/oop-host/{name}.c"))
                )
                .arg("-o")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let module = host.load_native(&q, &path, &[]).unwrap();
        std::fs::remove_file(path).unwrap();
        module
    };
    let fixture = load_fixture("cell-access");
    let observer = load_fixture("allocation-presence");
    let monitor = q.call(prototype, b"clone", &[]).unwrap();
    let mv = q
        .call(host.id.root, b"ensurePrivateVtableFor:", &[monitor])
        .unwrap();
    for n in [b"allocation:bytes:identity:".as_slice(), b"present:"] {
        let key = q.keep(host.intern(n).unwrap()).unwrap();
        q.call(
            mv,
            b"addMethod",
            &[key, q.call(observer, b"methodAt:", &[key]).unwrap()],
        )
        .unwrap();
    }
    let identity = |o| q.call(host.id.root, b"identityOf:", &[o]).unwrap();
    let present = |id| q.call(monitor, b"present:", &[id]).unwrap() == size(1).unwrap();
    let values = h.call(h.resolved(b"Vector"), b"new", &[]);
    let values = h.call(values, b"add:", &[nil]);
    let captures = h.call(h.resolved(b"Vector"), b"new", &[]);
    let with_slots = std::env::var_os("BIRNAM_CELL_SLOTS").is_some();
    let with_registry = std::env::var_os("BIRNAM_MODULE_REGISTRY").is_some();
    let manager = if with_slots {
        let fields = h.call(values, b"add:", &[h.call(h.resolved(b"Map"), b"new", &[])]);
        let manager = h.call(model, b"newSlots:parent:", &[fields, vt]);
        h.publish(b"Module", manager);
        h.publish(b"SymbolExemplarVt", h.resolved(b"SymbolVt"));
        manager
    } else {
        nil
    };
    let mut requests = Vec::new();
    if with_slots {
        let reflect = h.resolved(b"Reflect");
        requests.extend([
            (prototype, b"slotNames".as_slice(), 0),
            (reflect, b"isSymbol:", 1),
            (reflect, b"slotsOf:", 1),
            (reflect, b"slotIndex:in:", 2),
            (reflect, b"slot:in:", 2),
            (reflect, b"slot:in:put:", 3),
            (manager, b"slotNames", 0),
            (manager, b"libDir", 0),
            (manager, b"libDir:", 1),
        ]);
    }
    if with_registry {
        requests.extend([
            (manager, b"registry".as_slice(), 0),
            (manager, b"registry:", 1),
            (manager, b"recordNames", 0),
            (manager, b"recordOf:", 1),
            (manager, b"registryMutation:", 1),
            (manager, b"record:put:", 2),
            (manager, b"field:of:", 2),
            (manager, b"field:of:put:", 3),
            (manager, b"removeRecord:", 1),
            (manager, b"named:requires:", 2),
            (manager, b"kernel:", 1),
            (manager, b"requiresOf:", 1),
            (manager, b"kindOf:", 1),
            (manager, b"isRegistered:", 1),
            (manager, b"isLoaded:", 1),
            (manager, b"markLoaded:", 1),
            (manager, b"loadOrder:", 1),
            (manager, b"visit:order:path:", 3),
            (manager, b"publishRegistry:from:", 2),
        ]);
    }
    let mut records = Vec::new();
    if std::env::var_os("BIRNAM_ORDINARY_RECORDS").is_some() {
        for (name, fields, methods) in [
            (
                b"Logger".as_slice(),
                vec![b"name".as_slice(), b"level", b"sink"],
                vec![
                    (b"slotNames".as_slice(), 0),
                    (b"name", 0),
                    (b"thresholdRank", 0),
                    (b"sinkPath", 0),
                    (b"named:", 1),
                    (b"toFile:", 1),
                    (b"toStdout", 0),
                ],
            ),
            (
                b"Config",
                vec![b"configPath".as_slice()],
                vec![(b"slotNames".as_slice(), 0), (b"path", 0)],
            ),
            (
                b"HttpRequest",
                vec![b"method".as_slice(), b"path", b"raw"],
                vec![
                    (b"slotNames".as_slice(), 0),
                    (b"method", 0),
                    (b"path", 0),
                    (b"raw", 0),
                ],
            ),
            (
                b"ProcResult",
                vec![b"stdout".as_slice(), b"exit"],
                vec![
                    (b"slotNames".as_slice(), 0),
                    (b"stdout", 0),
                    (b"exitCode", 0),
                ],
            ),
            (
                b"AppConfig",
                vec![b"values".as_slice(), b"origins", b"configPath"],
                vec![
                    (b"slotNames".as_slice(), 0),
                    (b"values", 0),
                    (b"origins", 0),
                    (b"path", 0),
                    (b"at:", 1),
                    (b"originAt:", 1),
                ],
            ),
        ] {
            let mut initial = h.call(h.resolved(b"Vector"), b"new", &[]);
            for _ in &fields {
                initial = h.call(initial, b"add:", &[nil]);
            }
            let record = h.call(model, b"newSlots:parent:", &[initial, vt]);
            h.publish(name, record);
            let getters = methods
                .iter()
                .skip(1)
                .take(fields.len())
                .map(|(selector, _)| *selector)
                .collect::<Vec<_>>();
            requests.extend(
                methods
                    .into_iter()
                    .map(|(selector, arity)| (record, selector, arity)),
            );
            records.push((record, fields, getters));
        }
    }
    // The manifest names actual exported programs; no method body is rewritten.
    let harness_manifest = if std::env::var_os("BIRNAM_HARNESS_OWNERS").is_some() {
        Some(
            serde_json::from_slice::<Vec<(String, Vec<String>, Vec<String>)>>(
                &std::fs::read(programs.join("harness.json")).unwrap(),
            )
            .unwrap(),
        )
    } else {
        None
    };
    if let Some(manifest) = &harness_manifest {
        for (name, fields, methods) in manifest {
            let mut initial = h.call(h.resolved(b"Vector"), b"new", &[]);
            for field in fields {
                let value = if name == "Test" {
                    h.call(h.resolved(b"Vector"), b"new", &[])
                } else if name == "ConsoleRunner" || (name == "Bench" && field == "failures") {
                    h.integer(0)
                } else {
                    nil
                };
                initial = h.call(initial, b"add:", &[value]);
            }
            let owner = h.call(model, b"newSlots:parent:", &[initial, vt]);
            h.publish(name.as_bytes(), owner);
            requests.extend(
                methods
                    .iter()
                    .map(|method| (owner, method.as_bytes(), method.matches(':').count())),
            );
            records.push((
                owner,
                fields.iter().map(|f| f.as_bytes()).collect(),
                methods
                    .iter()
                    .skip(1)
                    .take(fields.len())
                    .map(|m| m.as_bytes())
                    .collect(),
            ));
        }
        requests.extend(
            [b"anySatisfy:".as_slice(), b"count:", b"select:", b"reject:"]
                .into_iter()
                .map(|method| (collection, method, 1)),
        );
    }
    for (index, (target, selector, arity)) in requests.into_iter().enumerate() {
        let behavior = if target == collection {
            collection
        } else {
            h.call(host.id.root, b"behaviorOf:", &[target])
        };
        let bytes = std::fs::read(programs.join(format!("{index}.bin"))).unwrap();
        let bytes = h.keep(h.call(
            h.resolved(b"Bytes"),
            b"fromBytes:",
            &[q.byte_values(&bytes).unwrap()],
        ));
        let code = h.call(h.resolved(b"CodeObject"), b"fromCanonicalBytes:", &[bytes]);
        let block = h.call(
            h.resolved(b"Codegen"),
            b"materializeResumable:captures:",
            &[code, captures],
        );
        let template = h.call(
            h.resolved(b"Codegen"),
            b"methodTemplateForArity:synchronous:",
            &[h.integer(arity), h.resolved(b"true")],
        );
        q.check_result(template).unwrap();
        let method = h.call(template, b"withImplementation:", &[block]);
        // The existing Method and dictionary protocols support this bootstrap path. No Store
        // MOP, global facts, source parser or source-metadata registry is loaded.
        let selector = h.keep(h.key(selector));
        h.call(behavior, b"addMethod:as:", &[method, selector]);
    }
    for name in [
        b"Store".as_slice(),
        b"db",
        b"Relational",
        b"Lang",
        b"Generation",
    ] {
        assert!(h.call(ns, b"at:", &[h.key(name)]).is_null());
    }
    for (record, fields, getters) in &records {
        let mark = h.mark();
        let reflect = h.resolved(b"Reflect");
        let value = h.call(prototype, b"clone", &[]);
        let instance = h.call(*record, b"clone", &[]);
        let rows = h.call(reflect, b"slotsOf:", &[instance]);
        assert_eq!(h.number(h.call(rows, b"size", &[])), fields.len());
        for (index, (&field, &getter)) in fields.iter().zip(getters).enumerate() {
            let field = h.keep(h.key(field));
            let field_value = if index == 0 {
                value
            } else {
                h.call(prototype, b"clone", &[])
            };
            h.call(reflect, b"slot:in:put:", &[field, instance, field_value]);
            assert_eq!(
                h.call(reflect, b"slot:in:", &[field, instance]),
                field_value
            );
            assert_eq!(h.call(instance, getter, &[]), field_value);
        }
        let clone = h.call(instance, b"clone", &[]);
        let field = h.keep(h.key(fields[0]));
        h.call(reflect, b"slot:in:put:", &[field, clone, nil]);
        h.collect();
        assert_eq!(h.call(reflect, b"slot:in:", &[field, instance]), value);
        assert_eq!(h.call(reflect, b"slot:in:", &[field, clone]), nil);
        let ids = [identity(instance), identity(clone), identity(value)];
        h.clear_from(mark);
        h.collect();
        assert!(ids.iter().all(|id| !present(*id)));
    }
    if std::env::var_os("BIRNAM_ORDINARY_RECORDS").is_some() {
        let logger = h.call(h.resolved(b"Logger"), b"named:", &[h.integer(42)]);
        assert_eq!(h.number(h.call(logger, b"name", &[])), 42);
        assert_eq!(h.number(h.call(logger, b"thresholdRank", &[])), 1);
        h.call(logger, b"toFile:", &[logger]);
        h.collect();
        assert_eq!(h.call(logger, b"sinkPath", &[]), logger);
        h.call(logger, b"toStdout", &[]);
        assert_eq!(h.call(logger, b"sinkPath", &[]), nil);
        let app = h.resolved(b"AppConfig");
        let key = h.keep(h.key(b"setting"));
        let map = h.call(h.resolved(b"Map"), b"new", &[]);
        let map = h.call(map, b"at:put:", &[key, logger]);
        h.call(
            h.resolved(b"Reflect"),
            b"slot:in:put:",
            &[h.key(b"values"), app, map],
        );
        assert_eq!(h.call(app, b"at:", &[key]), logger);
        h.call(
            h.resolved(b"Reflect"),
            b"slot:in:put:",
            &[h.key(b"origins"), app, map],
        );
        assert_eq!(h.call(app, b"originAt:", &[key]), logger);
    }
    if harness_manifest.is_some() {
        let start = h.mark();
        let registry = h.resolved(b"Test");
        let suite_proto = h.resolved(b"TestSuite");
        let module = h.keep(h.key(b"harnessModule"));
        let other = h.keep(h.key(b"otherModule"));
        let a = h.call(suite_proto, b"named:module:", &[module, module]);
        let b = h.call(suite_proto, b"named:module:", &[other, module]);
        h.call(registry, b"register:", &[a]);
        let snapshot = h.call(registry, b"suites", &[]);
        assert_eq!(h.number(h.call(snapshot, b"size", &[])), 2);
        let selected = h.call(registry, b"suitesForModule:", &[module]);
        assert_eq!(h.call(selected, b"at:", &[h.integer(0)]), a);
        assert_eq!(h.call(selected, b"at:", &[h.integer(1)]), b);
        h.call(a, b"test:do:", &[module, nil]);
        let cases = h.call(a, b"cases", &[]);
        let case = h.call(cases, b"at:", &[h.integer(0)]);
        assert_eq!(h.call(case, b"name", &[]), module);
        assert_eq!(h.call(case, b"body", &[]), nil);
        h.call(registry, b"unregister:", &[a]);
        h.call(registry, b"unregister:", &[a]);
        assert_eq!(h.number(h.call(snapshot, b"size", &[])), 2);
        let remaining = h.call(registry, b"suites", &[]);
        assert_eq!(h.call(remaining, b"at:", &[h.integer(0)]), b);
        h.call(
            h.resolved(b"Reflect"),
            b"slot:in:put:",
            &[h.key(b"testsModule"), b, other],
        );
        assert_eq!(
            h.number(h.call(
                h.call(registry, b"suitesForModule:", &[module]),
                b"size",
                &[]
            )),
            0
        );
        h.call(registry, b"unregister:", &[b]);
        let suite = h.call(h.resolved(b"BenchSuite"), b"named:", &[module]);
        h.call(suite, b"fixedCase:iters:do:", &[module, h.integer(7), nil]);
        let bc = h.call(h.call(suite, b"cases", &[]), b"at:", &[h.integer(0)]);
        assert_eq!(h.number(h.call(bc, b"fixedN", &[])), 7);
        assert_eq!(h.number(h.call(bc, b"k", &[])), 1);
        let context = h.call(h.resolved(b"TestContext"), b"for:", &[suite]);
        assert_eq!(h.call(context, b"runner", &[]), suite);
        let ids = [a, b, case, suite, bc, context].map(identity);
        h.clear_from(start);
        h.collect();
        assert!(
            ids.iter().all(|id| !present(*id)),
            "unregistered harness objects must collect"
        );
    }
    if with_slots {
        let reflect = h.resolved(b"Reflect");
        let key = h.keep(h.key(b"libDir"));
        let rows = h.call(reflect, b"slotsOf:", &[manager]);
        assert_eq!(h.number(h.call(rows, b"size", &[])), 2);
        let row = h.call(rows, b"at:", &[h.integer(0)]);
        assert_eq!(h.call(row, b"at:", &[h.key(b"name")]), key);
        assert_eq!(h.call(reflect, b"slot:in:", &[key, manager]), nil);
        let path = h.keep(h.call(
            h.resolved(b"String"),
            b"fromBytes:",
            &[q.byte_values(b"/owned").unwrap()],
        ));
        assert_eq!(
            h.call(reflect, b"slot:in:put:", &[key, manager, path]),
            manager
        );
        h.collect();
        assert_eq!(h.call(manager, b"libDir", &[]), path);
        h.call(manager, b"libDir:", &[nil]);
        assert_eq!(h.call(reflect, b"slot:in:", &[h.integer(0), manager]), nil);
        let default = h.call(manager, b"libDir", &[]);
        assert_eq!(
            q.read_bytes(h.call(default, b"bytes", &[])).unwrap(),
            b"lib"
        );
        // Foreign native storage is explicitly unsupported, never decoded.
        assert_eq!(h.call(reflect, b"slotsOf:", &[path]), nil);
    }
    if with_registry {
        let a = h.keep(h.key(b"a"));
        let b = h.keep(h.key(b"b"));
        let empty = h.call(h.resolved(b"Vector"), b"new", &[]);
        let deps = h.call(empty, b"add:", &[b]);
        h.call(manager, b"markLoaded:", &[a]);
        h.call(manager, b"named:requires:", &[a, deps]);
        h.call(manager, b"kernel:", &[b]);
        assert_eq!(h.call(manager, b"isLoaded:", &[a]), h.resolved(b"true"));
        assert_eq!(h.call(manager, b"requiresOf:", &[a]), deps);
        let names = h.call(empty, b"add:", &[a]);
        let order = h.call(manager, b"loadOrder:", &[names]);
        assert_eq!(h.call(order, b"at:", &[h.integer(0)]), b);
        assert_eq!(h.call(order, b"at:", &[h.integer(1)]), a);
        let snapshot = h.call(manager, b"registry", &[]);
        let clone = h.call(manager, b"clone", &[]);
        h.call(manager, b"removeRecord:", &[a]);
        h.collect();
        assert_eq!(h.call(manager, b"recordOf:", &[a]), nil);
        assert_ne!(h.call(snapshot, b"at:", &[a]), nil);
        assert_eq!(h.call(clone, b"isRegistered:", &[a]), h.resolved(b"true"));
        // Registry ownership is ordinary reachability, not a database root.
        let start = h.mark();
        let private = h.call(manager, b"clone", &[]);
        let key = h.keep(h.key(b"privateRecord"));
        h.call(private, b"named:requires:", &[key, empty]);
        let record = h.call(private, b"recordOf:", &[key]);
        let ids = [identity(private), identity(record)];
        h.clear_from(start);
        h.collect();
        assert!(ids.iter().all(|id| !present(*id)));
    }
    current_accessor_dispatch(&h, fixture, cell, model, gc);
    let floor = h.mark();
    let value = h.call(prototype, b"clone", &[]);
    let local = h.call(cell, b"with:", &[value]);
    let local_id = identity(local);
    let value_id = identity(value);
    let local_token = host.send(host.roots, b"retain:", &[local]).unwrap();
    h.clear_from(floor);
    h.collect();
    assert!(present(local_id) && present(value_id));
    assert_eq!(h.call(local, b"value", &[]), value);
    assert_eq!(
        h.call(local, b"replace:with:", &[nil, nil]),
        h.resolved(b"false")
    );
    assert_eq!(h.call(local, b"value", &[]), value);
    assert_eq!(
        h.call(local, b"replace:with:", &[value, nil]),
        h.resolved(b"true")
    );
    assert_eq!(h.call(local, b"value", &[]), nil);
    h.call(local, b"set:", &[h.integer(41)]);
    assert_eq!(h.number(h.call(local, b"inc", &[])), 42);
    assert_eq!(h.number(h.call(local, b"dec", &[])), 41);
    assert_eq!(h.number(h.call(local, b"history", &[])), 0);
    let clone = h.call(local, b"clone", &[]);
    assert_ne!(clone, local);
    h.call(clone, b"set:", &[nil]);
    assert_eq!(h.number(h.call(local, b"value", &[])), 41);
    assert_eq!(h.call(clone, b"value", &[]), nil);
    let clone_id = identity(clone);
    h.clear_from(floor);
    host.send(host.roots, b"release:", &[local_token]).unwrap();
    h.collect();
    assert!(!present(local_id) && !present(clone_id) && !present(value_id));
    h.clear_from(0);
}
