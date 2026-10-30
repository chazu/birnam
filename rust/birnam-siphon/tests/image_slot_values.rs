//! Images accept explicit owner-approved Oop slots, not arbitrary native payloads.
use std::{path::PathBuf,process::Command};
#[test] #[ignore="requires installed Image and the current ImageFormat"]
fn queues_and_shared_slot_values_roundtrip_without_a_family_catalog_entry(){
 let lib=PathBuf::from(std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());let bin=lib.parent().unwrap().join("bin/birnam");
 let dir=std::env::temp_dir().join(format!("birnam-slot-image-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();let image=dir.join("values.image");
 let prelude=std::env::var_os("BIRNAM_IMAGE_FORMAT_SOURCE").map(|p|format!("[System load: {:?}];\n",PathBuf::from(p).to_string_lossy())).unwrap_or_default();
 let run=|name:&str,source:String,marker:&str|{let path=dir.join(name);std::fs::write(&path,prelude.clone()+&source).unwrap();let out=Command::new(&bin).arg(&path).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));assert!(String::from_utf8_lossy(&out.stdout).contains(marker),"stdout={} stderr={}",String::from_utf8_lossy(&out.stdout),String::from_utf8_lossy(&out.stderr));};
 run("save.birnam",format!(r#"
 let assertSlots := [:ok | [[ok == true] ifFalse: [| [System exit: 1]]]];
 let proto := [ObjectModel newSlots: #[nil] parent: ObjectVt];
 [Namespace at: #ReviewSlotsVt put: [Reflect vtableOf: proto]];
 let record := #[#[9 nil "ReviewSlotsVt"] #[#[0 nil nil]]];
 [assertSlots value: [[ImageFormat family: record] isError]];
 [Reflect defineMethod: #imageAcceptsSlots: on: proto as: [:self :words | [[Integer fromWord: words] = 1]]];
 [assertSlots value: [[ImageFormat family: record] = "ReviewSlotsVt"]];
 [assertSlots value: [[ImageFormat family: #[#[9 nil "QueueVt"] #[#[0 nil nil] #[0 nil nil]]]] isError]];
 [assertSlots value: [[ImageFormat family: #[#[9 nil "QueueVt"] #[#[3 nil nil] #[0 nil nil] #[0 nil nil]]]] isError]];
 // Keep the test-only family out of the image's exported identities.
 [Namespace at: #ReviewSlotsVt put: nil];
 [Globals at: #proto put: nil]; [Globals at: #record put: nil];
 let shared := [ObjectModel newSlots: #["shared"] parent: ObjectVt];
 let one := [[Queue new] add: shared];let two := [one add: "tail"];
 [Reflect defineMethod: #marker on: one as: [:self | #restored]];
 [Globals at: #SavedQueueOne put: one];[Globals at: #SavedQueueTwo put: two];[Globals at: #SavedQueueItem put: shared];
 [assertSlots value: [System saveImage: {:?}]];[System println: "SLOT_IMAGE_SAVED"];
 "#,image.to_string_lossy()),"SLOT_IMAGE_SAVED");
 run("load.birnam",format!(r#"
 let assertSlots := [:ok | [[ok == true] ifFalse: [| [System exit: 1]]]];
 [assertSlots value: [System loadImage: {:?}]];
 [assertSlots value: [[[SavedQueueOne asVector] at: 0] == SavedQueueItem]];
 [assertSlots value: [[[SavedQueueTwo asVector] at: 0] == SavedQueueItem]];
 [assertSlots value: [[SavedQueueOne marker] = #restored]];
 let rest := [[SavedQueueTwo takeFirst] at: 1];
 [assertSlots value: [[rest asVector] = #["tail"]]];
 [assertSlots value: [[[SavedQueueOne add: "new"] size] = 2]];
 [System gc];[assertSlots value: [[SavedQueueOne size] = 1]];
 [System println: "SLOT_IMAGE_RESTORED"];
 "#,image.to_string_lossy()),"SLOT_IMAGE_RESTORED");
 std::fs::remove_dir_all(dir).unwrap();
}
