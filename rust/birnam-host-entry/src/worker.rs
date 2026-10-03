use crate::HostRuntime;
use birnam_abi::id_protocol::Operand;
use birnam_host_support::{HostError, native_error};
use std::path::Path;
impl HostRuntime<'_> {
    /// Enter a restored worker without evaluating stdlib or global main.
    pub fn run_worker(
        &self,
        path: &Path,
        receiver: &str,
        selector: &str,
        fd: i32,
    ) -> Result<(), HostError> {
        if fd < 0 {
            return Err(HostError("worker channel must be nonnegative".into()));
        }
        let host = self;
        let scope = host.scope();
        let isolate = host.named(b"Isolate")?;
        let admission = (|| {
            host.load_image(path)?;
            host.rebase_module(&scope, &host.resources)?;
            let mut target = host.global(&scope, receiver.as_bytes())?;
            if target == host.named(b"nil")? {
                target = host.named(receiver.as_bytes())?;
            }
            scope.keep(Operand::object(target))?;
            let selector = unsafe { host.id.intern_name(selector.as_bytes()) }
                .map_err(|e| native_error("intern worker selector", e))?;
            scope.keep(Operand::object(selector))?;
            let behavior =
                scope.call(host.id.receiver, b"behaviorOf:", &[Operand::object(target)])?;
            if host
                .send(behavior, b"lookup", &[Operand::object(selector)])?
                .as_entry()
                .is_none()
            {
                return Err(HostError("worker selector is not callable".into()));
            }
            if std::env::var("BIRNAM_TASK_SERVICE").is_ok_and(|s| s == "1")
                && host.global(&scope, b"TaskService")? == host.named(b"nil")?
            {
                return Err(HostError("image has no TaskService".into()));
            }
            Ok((target, selector))
        })();
        let (target, selector) = match admission {
            Ok(v) => v,
            Err(e) => {
                let kind = scope.string(b"imageAdmission")?;
                let message = scope.string(e.0.as_bytes())?;
                let _ = host.send(
                    isolate,
                    b"workerBootError:kind:message:",
                    &[
                        Operand::word(fd as usize),
                        Operand::object(kind),
                        Operand::object(message),
                    ],
                );
                return Err(e);
            }
        };
        host.enter_running()?;
        let result = host.send(
            isolate,
            b"runWorker:receiver:selector:",
            &[
                Operand::word(fd as usize),
                Operand::object(target),
                Operand::object(selector),
            ],
        )?;
        if result.as_word() == Some(0) {
            Ok(())
        } else {
            Err(HostError("worker channel failed".into()))
        }
    }
}
