use crate::HostRuntime;
use birnam_abi::id_protocol::Operand;
use birnam_host_support::{HostError, native_error};
use std::{os::unix::ffi::OsStrExt, path::Path};
impl HostRuntime<'_> {
    pub(crate) fn bootstrap(&self, source: bool, tools: bool) -> Result<(), HostError> {
        if source {
            return self.bootstrap_source();
        }
        let path = self.resources.lib_dir.join(if tools {
            "birnam-tools.image"
        } else {
            "birnam-bootstrap.image"
        });
        self.load_image_mode(&path, false, true).map_err(|error| HostError(format!(
            "{error}; reinstall Birnam to prepare its bootstrap image (BIRNAM_FORCE_SOURCE=1 explicitly requests source bootstrap)"
        )))?;
        self.rebase_module(&self.scope(), &self.resources)
    }
    pub(crate) fn bootstrap_source(&self) -> Result<(), HostError> {
        let host = self;
        let scope = host.scope();
        if host.global(&scope, b"Module")? == host.named(b"nil")? {
            scope.evaluate(
                include_bytes!("../../../modules/modules.birnam"),
                Path::new("<modules>"),
                Some(b"modules"),
            )?;
        }
        scope
            .evaluate(
                include_bytes!("../../../modules/stdlib.birnam"),
                Path::new("<stdlib>"),
                Some(b"stdlib"),
            )
            .map_err(|e| HostError(format!("bootstrap stdlib: {e}")))?;
        scope.evaluate(b"[Module markLoaded: #stdlib]", Path::new("<host>"), None)?;
        scope.evaluate(
            include_bytes!("../../../modules/stackless-control.birnam"),
            Path::new("<stacklessControl>"),
            Some(b"stacklessControl"),
        )?;
        scope.evaluate(
            b"[Module markLoaded: #stacklessControl]",
            Path::new("<host>"),
            None,
        )?;
        scope.evaluate(
            include_bytes!("../../../modules/compiled-unit.birnam"),
            Path::new("<compiledUnit>"),
            Some(b"compiledUnit"),
        )?;
        scope.evaluate(
            include_bytes!("../../../modules/isolate.birnam"),
            Path::new("<isolate>"),
            Some(b"isolate"),
        )?;
        let globals = host.named(b"Globals")?;
        let name = unsafe { host.id.intern_name(b"Module") }
            .map_err(|e| native_error("intern Module", e))?;
        let module = scope.call(globals, b"at:", &[Operand::object(name)])?;
        for name in [
            "artifact",
            "store",
            "storage",
            "peg",
            "tcc",
            "codegen",
            "prelude",
            "collection",
            "error",
            "relational",
            "image",
            "alien",
            "isolate",
        ] {
            let symbol = unsafe { host.id.intern_name(name.as_bytes()) }
                .map_err(|e| native_error("intern kernel module", e))?;
            scope.call(module, b"kernel:", &[Operand::object(symbol)])?;
        }
        let lib = scope.string(self.resources.lib_dir.as_os_str().as_bytes())?;
        scope.call(module, b"libDir:", &[Operand::object(lib)])?;
        let lang =
            unsafe { host.id.intern_name(b"lang") }.map_err(|e| native_error("intern lang", e))?;
        scope.call(module, b"markLoaded:", &[Operand::object(lang)])?;
        Ok(())
    }
}
