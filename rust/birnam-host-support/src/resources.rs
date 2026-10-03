use std::path::PathBuf;
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resources {
    pub prefix: PathBuf,
    pub lib_dir: PathBuf,
    pub rust_workspace_dir: PathBuf,
    pub guide_dir: PathBuf,
    pub id_dylib: PathBuf,
}
