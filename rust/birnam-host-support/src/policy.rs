use std::env;
#[derive(Clone, Copy, Debug, Default)]
pub struct StartupPolicy {
    pub skip_stdlib: bool,
    pub precise_gc: bool,
    pub source_bootstrap: bool,
    pub tool_bootstrap: bool,
}
impl StartupPolicy {
    pub fn from_environment() -> Self {
        Self {
            skip_stdlib: env::var_os("BIRNAM_NO_STDLIB").is_some(),
            precise_gc: env::var_os("BIRNAM_GC_PRECISE").is_some(),
            source_bootstrap: env::var_os("BIRNAM_FORCE_SOURCE").is_some(),
            tool_bootstrap: false,
        }
    }
}
