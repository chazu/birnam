//! Actual bridge callbacks and address-fenced teardown in a fresh process.
use super::*;
use std::mem::{ManuallyDrop, MaybeUninit};

#[test]
#[ignore = "requires an installed prefix and fresh owner-thread process"]
fn native_bridge_and_address_fence() {
    unsafe {
        let prefix = PathBuf::from(env::var_os("BIRNAM_HOST_TEST_PREFIX").unwrap());
        let resources = resolve_from_exe_dir(&prefix.join("bin"), Overrides::default()).unwrap();
        assert!(PROJECT_BRIDGE_RUNTIME.load(Ordering::Acquire).is_null());
        let mut missing = resources.clone();
        missing.id_dylib = prefix.join("missing-floor-for-bridge-contract.dylib");
        assert!(matches!(
            start(missing, StartupPolicy::from_flags(false, false)),
            Err(RuntimeStartError::Floor(_))
        ));
        assert!(PROJECT_BRIDGE_RUNTIME.load(Ordering::Acquire).is_null());

        // Construct before installation and destroy in place: the production
        // object remains unboxed and no relocation/pinning policy is introduced.
        let mut runtime =
            ManuallyDrop::new(start(resources, StartupPolicy::from_flags(false, false)).unwrap());
        let mut anchor = 0usize;
        runtime.prepare_execution(&mut anchor);
        let address = &mut *runtime as *mut HostRuntime;
        runtime.install_project_bridge().unwrap();
        assert_eq!(PROJECT_BRIDGE_RUNTIME.load(Ordering::Acquire), address);
        runtime.install_project_bridge().unwrap();
        let api = runtime.stack.api();
        let project = global_named(api, "Project").unwrap();
        let uri = b"file:///tmp/birnam%20bridge/example.birnam";
        let argument = (api.mkString)(uri.as_ptr(), uri.len());
        let result = (api.send)(project, (api.intern)(c"pathForUri:".as_ptr()), &argument, 1);
        assert_eq!(
            bridge_string(api, result).as_deref(),
            Some("/tmp/birnam bridge/example.birnam")
        );
        assert_eq!(PROJECT_BRIDGE_RUNTIME.load(Ordering::Acquire), address);

        let mismatch = env::var("BIRNAM_HOST_TEST_MISMATCH").unwrap() == "1";
        // An address token is sufficient to test compare-exchange fencing.
        // It is never dereferenced, and no callback runs with this token stored.
        let mut other = MaybeUninit::<HostRuntime>::uninit();
        if mismatch {
            PROJECT_BRIDGE_RUNTIME.store(other.as_mut_ptr(), Ordering::Release);
            assert_eq!(
                runtime.install_project_bridge(),
                Err(ProjectBridgeError::AlreadyOwned)
            );
        }
        ManuallyDrop::drop(&mut runtime);
        assert_eq!(
            PROJECT_BRIDGE_RUNTIME.load(Ordering::Acquire),
            if mismatch {
                other.as_mut_ptr()
            } else {
                std::ptr::null_mut()
            }
        );
        PROJECT_BRIDGE_RUNTIME.store(std::ptr::null_mut(), Ordering::Release);
        println!("host-bridge-contract: native callback and mismatch={mismatch} passed");
    }
}
