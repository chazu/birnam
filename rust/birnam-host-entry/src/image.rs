use crate::HostRuntime;
use birnam_abi::id_protocol::Operand;
use birnam_host_support::{HostError, Resources, Scope};
use std::{fs, os::unix::ffi::OsStrExt, path::Path};
impl HostRuntime<'_> {
    /// Restore into the native runtime without evaluating source bootstrap.
    /// Images own their saved methods, globals and source registry.

    pub(crate) fn rebase_module(
        &self,
        scope: &Scope<'_>,
        resources: &Resources,
    ) -> Result<(), HostError> {
        // The runtime library directory belongs to this executable, not the saving process.
        let module = self.global(scope, b"Module")?;
        if module != self.named(b"nil")? {
            let lib = scope.string(resources.lib_dir.as_os_str().as_bytes())?;
            scope.call(module, b"libDir:", &[Operand::object(lib)])?;
        }
        Ok(())
    }
    pub(crate) fn load_image(&self, path: &Path) -> Result<(), HostError> {
        self.load_image_mode(path, false, false)
    }
    pub(crate) fn load_image_mode(
        &self,
        path: &Path,
        rebuild: bool,
        mapped_only: bool,
    ) -> Result<(), HostError> {
        let load = || -> Result<(), HostError> {
            let metadata = fs::metadata(path).map_err(|e| HostError(e.to_string()))?;
            if !metadata.is_file() {
                return Err(HostError("image must be a regular file".into()));
            }
            let scope = self.scope();
            let filename = scope.string(path.as_os_str().as_bytes())?;
            let system = self.named(b"System")?;
            let result = scope.call(
                system,
                if mapped_only {
                    b"loadImageMapped:"
                } else {
                    b"loadImage:rebuild:"
                },
                &[
                    Operand::object(filename),
                    Operand::object(self.named(if rebuild { b"true" } else { b"false" })?),
                ][..if mapped_only { 1 } else { 2 }],
            )?;
            if result == self.named(b"true")? || result == self.named(b"false")? {
                return Ok(());
            }
            scope.check_result(result)?;
            let report = scope.call(system, b"imageSaveReport", &[])?;
            let status = scope.call(report, b"status", &[])?;
            let status = scope.call(status, b"printString", &[])?;
            let status = scope.text(status)?;
            let detail = scope.call(report, b"detail", &[])?;
            Err(HostError(format!(
                "image load {status}: {}",
                scope.text(detail)?
            )))
        };
        load().map_err(|e| HostError(format!("{}: {e}", path.display())))
    }
    /// Enter saved main, fulfill a returned intent after its frame exits, and
    /// checkpoint Isolate on success or failure. Nil and false remain successes.
    pub fn run_image_main(&self) -> Result<(), HostError> {
        self.enter_running()?;
        let run = || -> Result<(), HostError> {
            let scope = self.scope();
            let main = self.global(&scope, b"main")?;
            if main == self.named(b"nil")? {
                return Err(HostError("image has no global main".into()));
            }
            let result = scope.call(main, b"value", &[])?;
            scope.check_result(result)?;
            let result = scope.fulfill_intent(result)?;
            scope.check_result(result)
        };
        let result = run();
        let checkpoint = self.send(self.named(b"Isolate")?, b"checkpoint", &[]);
        result
            .and_then(|()| checkpoint.map(|_| ()))
            .map_err(|e| HostError(format!("image main: {e}")))
    }
}
impl HostRuntime<'_> {
    pub fn load_image_bytes(&self, bytes: &[u8]) -> Result<(), HostError> {
        let q = self.scope();
        let bytes = q.string(bytes)?;
        let result = q.call(
            self.named(b"System")?,
            b"loadImageBytes:rebuild:",
            &[
                Operand::object(bytes),
                Operand::object(self.named(b"false")?),
            ],
        )?;
        if result != self.named(b"true")? && result != self.named(b"false")? {
            q.check_result(result)?;
            return Err(HostError("baked image rejected".into()));
        }
        self.rebase_module(&q, &self.resources)
    }
}
