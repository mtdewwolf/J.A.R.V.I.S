//! Identifiers. Prefixed ULID-style strings keep IDs sortable and greppable in
//! logs and events.

use uuid::Uuid;

/// New identifier with the given short prefix, e.g. `new_id("cmd")` → `cmd_…`.
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}
