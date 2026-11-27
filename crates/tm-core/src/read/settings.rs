//! `settings.show` (section 10; 11.19; item 78): every setting with its
//! value, default, and the event that set it.

use serde_json::Value;

use crate::ops::Core;

/// `settings.show`.
#[must_use]
pub fn settings_show(core: &Core) -> Value {
    core.settings.show()
}
