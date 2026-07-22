//! Collector metadata retained while R3 keeps marking and sweeping disabled.

use crate::abi::RootProvider;
use crate::boundary;

pub(crate) const MAX_ROOT_PROVIDERS: usize = 32;

pub(crate) struct MetadataShell {
    stack_bottom: usize,
    auto_requested: bool,
    precise: bool,
    root_providers: Vec<RootProvider>,
}

impl MetadataShell {
    pub(crate) fn new() -> Self {
        Self {
            stack_bottom: 0,
            auto_requested: false,
            precise: false,
            root_providers: Vec::new(),
        }
    }

    pub(crate) fn set_stack_bottom(&mut self, address: usize) {
        self.stack_bottom = address;
    }

    pub(crate) fn set_auto_requested(&mut self, enabled: bool) {
        self.auto_requested = enabled;
    }

    pub(crate) fn set_precise(&mut self, enabled: bool) {
        self.precise = enabled;
    }

    pub(crate) fn register_root_provider(&mut self, provider: RootProvider) {
        boundary::require_invariant(
            self.root_providers.len() < MAX_ROOT_PROVIDERS,
            "gc: too many root providers",
        );
        self.root_providers.push(provider);
    }

    /// R3 retains all metadata but deliberately performs no mark or sweep.
    pub(crate) fn collect_disabled(&self) -> usize {
        0
    }

    #[cfg(test)]
    pub(crate) fn stack_bottom(&self) -> usize {
        self.stack_bottom
    }

    #[cfg(test)]
    pub(crate) fn auto_requested(&self) -> bool {
        self.auto_requested
    }

    #[cfg(test)]
    pub(crate) fn precise(&self) -> bool {
        self.precise
    }

    #[cfg(test)]
    pub(crate) fn root_provider_count(&self) -> usize {
        self.root_providers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::MarkFn;

    unsafe extern "C" fn provider(_: MarkFn) {}

    #[test]
    fn shell_retains_settings_and_maximum_provider_mix_without_collecting() {
        let mut metadata = MetadataShell::new();
        metadata.set_stack_bottom(0x1234_5678);
        metadata.set_auto_requested(true);
        metadata.set_precise(true);
        for _ in 0..MAX_ROOT_PROVIDERS {
            metadata.register_root_provider(provider);
        }
        assert_eq!(metadata.stack_bottom(), 0x1234_5678);
        assert!(metadata.auto_requested());
        assert!(metadata.precise());
        assert_eq!(metadata.root_provider_count(), MAX_ROOT_PROVIDERS);
        assert_eq!(metadata.collect_disabled(), 0);
    }
}
