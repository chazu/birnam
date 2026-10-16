//! Live heap restoration, Store/source exchange, generation adoption and partial-load reporting.
mod publication;
pub use birnam_image_codec::{behavior_links, native_code, retained_facts, v8};
pub mod support {
    pub use crate::publication::publish_atomic;
    pub use birnam_image_codec::support::*;
}
#[allow(unused_imports)]
pub use support::*;
#[cfg(feature = "module")]
mod native;
