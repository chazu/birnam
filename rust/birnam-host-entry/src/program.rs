use crate::HostRuntime;
use birnam_host_support::{HostError, read_source_file};
use std::{fs, os::unix::ffi::OsStrExt, path::Path};
impl HostRuntime<'_> {
    pub fn eval_script(&self, path: &Path) -> Result<(), HostError> {
        let source = read_source_file(path)?;
        self.scope()
            .evaluate(&source, path, None)
            .map_err(|e| HostError(format!("{}: {e}", path.display())))
    }
    /// Compile to a durable native-object artifact without executing source.
    pub fn compile_module(&self, path: &Path, output: &Path) -> Result<(), HostError> {
        let source = read_source_file(path)?;
        let compile = || {
            let scope = self.scope();
            let source = scope.string(&source)?;
            let output = scope.string(output.as_os_str().as_bytes())?;
            let filename = scope.string(path.as_os_str().as_bytes())?;
            let cg = self.named(b"Codegen")?;
            let result = scope.call(
                cg,
                b"compile:output:from:",
                &[
                    source,
                    output,
                    filename,
                ],
            )?;
            scope.check(cg, result)?;
            if result == self.named(b"nil")? {
                return Err(HostError("artifact compilation failed".into()));
            }
            Ok(())
        };
        compile().map_err(|e: HostError| HostError(format!("{}: {e}", path.display())))
    }
    /// Let the authored Module manager preflight and activate the closure.
    pub fn load_artifact(&self, path: &Path) -> Result<(), HostError> {
        let load = || {
            let path = fs::canonicalize(path).map_err(|e| HostError(e.to_string()))?;
            let scope = self.scope();
            let manager = self.global(&scope, b"Module")?;
            if manager == self.named(b"nil")? {
                return Err(HostError(
                    "artifact loading requires the Module manager".into(),
                ));
            }
            let directory = scope.string(
                path.parent()
                    .unwrap_or(Path::new("."))
                    .as_os_str()
                    .as_bytes(),
            )?;
            let configured = scope.call(manager, b"libDir:", &[directory])?;
            scope.check_result(configured)?;
            let path = scope.string(path.as_os_str().as_bytes())?;
            let result = scope.call(manager, b"loadArtifact:", &[path])?;
            scope.check(self.named(b"Codegen")?, result)?;
            if result == self.named(b"nil")? {
                return Err(HostError("artifact activation failed".into()));
            }
            Ok(())
        };
        load().map_err(|e: HostError| HostError(format!("{}: {e}", path.display())))
    }
}
