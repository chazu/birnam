fn main() {
 let args: Vec<_> = std::env::args().collect();
 match birnam_host::load_floor(std::path::Path::new(&args[1]),None,None) {
  Ok(_) => assert_eq!(args[2],"ok"),
  Err(birnam_host::FloorError::GuardVerdict { reason, .. }) => assert_eq!(reason,args[2]),
  // Both factories share a library: absence fails at the earlier table factory.
  Err(birnam_host::FloorError::DispatchCache { reason, .. }) => {
   assert_eq!(args[2],"shared library missing");assert_eq!(reason,"open failed");
  }
  Err(error) => panic!("unexpected host admission: {error:?}"),
 }
}
