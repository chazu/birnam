//! Default bulk call preparation owns its frame copies; the operation stays live.
#[allow(dead_code)]
#[path = "support/minimal.rs"] mod minimal;
#[allow(dead_code)]
#[path = "support/oop.rs"] mod oop;
use birnam_abi::size;
#[test]
#[ignore = "requires production native modules and Lang seed"]
fn default_preparation_does_not_send_scalar_frame_messages() {
    let h = minimal::native_floor(); let q = h.scope();
    for name in ["peg", "lang_seed"] {
        h.load_native(&q, &h.resources.lib_dir.join(format!("libbirnam_{name}.dylib")), &[h.namespace]).unwrap();
    }
    let hooks = oop::hooks::Hooks::new(&h);
    let path = std::env::temp_dir().join(format!("birnam-prepare-{}.dylib", std::process::id()));
    assert!(std::process::Command::new("clang").args(["-dynamiclib","-O2","-Wall","-Wextra","-Werror", "tests/fixtures/oop-host/prepared-call.c", "-o"])
        .arg(&path).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).status().unwrap().success());
    let fixture = h.load_native(&q, &path, &[]).unwrap(); std::fs::remove_file(path).unwrap();
    let method = q.call(h.named(b"Codegen").unwrap(), b"compileResumable:", &[q.string(b"[| let x := 37; let y := x; [y word]]").unwrap()]).unwrap();
    let wrapper = q.call(h.named(b"Block").unwrap(), b"fromMethod:captureOperands:", &[method]).unwrap();
    let block = q.call(wrapper, b"value", &[]).unwrap();
    let machine = h.named(b"Machine").unwrap();
    let vt = q.call(h.id.root, b"behaviorOf:", &[machine]).unwrap();
    let selector = hooks.symbol(b"prepareCall:selector:");
    let original = q.call(vt,b"lookup",&[selector]).unwrap();
    let denied = hooks.context(&[hooks.fault(71),size(0).unwrap()]);
    let state = hooks.context(&[original,hooks.method(b"deny",denied),std::ptr::null_mut(),std::ptr::null_mut(),size(0).unwrap()]);
    let callback = q.call(fixture,b"methodAt:",&[hooks.symbol(b"noScalar")]).unwrap();
    let callback = q.call(callback,b"withContext:",&[state]).unwrap();
    q.call(vt,b"addMethod",&[selector,callback]).unwrap();
    let result=q.call(block,b"value",&[]);
    q.call(vt,b"addMethod",&[selector,original]).unwrap();
    assert_eq!(result.unwrap(),size(37).unwrap());
    assert!(hooks.count(state,4)>0); assert_eq!(hooks.count(denied,1),0);
    // Replacing the bulk operation itself still changes execution and propagates errors.
    q.call(vt,b"addMethod",&[selector,hooks.method(b"deny",denied)]).unwrap();
    assert!(q.call(block,b"value",&[]).is_err());
    q.call(vt,b"addMethod",&[selector,original]).unwrap();
    assert_eq!(hooks.count(denied,1),1);
}
