//! Reassociation must advance only after successful existing header mutation.
use super::*;
use crate::dispatch::build_bootstrap;
unsafe extern "C" fn identity(receiver: Oop, _: *const Oop, _: usize) -> Oop { receiver }
unsafe extern "C" fn nil(_: Oop, _: *const Oop, _: usize) -> Oop { with_active(|r| r.canonical().nil) }
#[test]
fn behavior_reassociation_epoch_transcript() {
    let bootstrap=build_bootstrap();let r=&bootstrap.runtime;
    let parent=r.create_vtable(r.globals().object_vt);let child=r.create_vtable(parent);
    let selector=r.intern(b"epochReassociationAnswer").unwrap();let receiver=r.allocate_instance(parent,0);
    r.add_method(parent,selector,identity);r.add_method(child,selector,nil);
    let before=unsafe { (*r.state.get()).vtables.mutation_epoch() };
    assert_eq!(r.enter(|| r.send_slice(receiver,selector,&[])),receiver);
    r.reassociate_behavior(receiver,child);
    assert_eq!(unsafe { (*r.state.get()).vtables.mutation_epoch() },before+1);
    assert_eq!(r.enter(|| r.send_slice(receiver,selector,&[])),r.canonical().nil);
    r.reassociate_behavior(receiver,child);
    assert_eq!(unsafe { (*r.state.get()).vtables.mutation_epoch() },before+2);
    r.reassociate_behavior(receiver,parent);
    assert_eq!(unsafe { (*r.state.get()).vtables.mutation_epoch() },before+3);
    assert_eq!(r.enter(|| r.send_slice(receiver,selector,&[])),receiver);
    println!("epoch Runtime reassociation: {before}, {}, {}, {}; answers identity/nil/identity",before+1,before+2,before+3);
}
