fn main() {
 let args: Vec<_> = std::env::args().collect();
 match birnam_host::load_floor(std::path::Path::new(&args[1]),None,None) {
  Ok(_) => assert_eq!(args[2],"ok"),
  Err(birnam_host::FloorError::StorageObservation { reason, .. }) => assert_eq!(reason,args[2]),
  Err(error) => panic!("unexpected host admission: {error:?}"),
 }
}
