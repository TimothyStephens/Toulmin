//! `settings.set`, `settings.show`, and `settings.unset` (section 9; 11.19;
//! item 74).

use serde_json::{Value, json};
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::typed_value::TypedValue;
use tm_owner_cap::OwnerCap;

use crate::Result;
use crate::ops::{Core, owner};
use crate::settings;

/// The `settings.changed` event for `key` going from the current value to
/// `value` (`None` for an unset).
fn changed(core: &Core, key: &str, value: Option<&TypedValue>) -> Result<KnownEvent> {
    let previous = core
        .settings
        .set_by(key)
        .and_then(|_| core.settings.get(key));
    Ok(KnownEvent::new(
        EventType::SettingsChanged,
        &json!({
            "key": key,
            "value": value.map_or(Value::Null, TypedValue::to_json),
            "previous": previous.map_or(Value::Null, |v| v.to_json()),
        }),
    )?)
}

/// `tm settings set KEY VALUE...` (list-valued keys take repeated values
/// and replace the whole list).
///
/// # Errors
/// `E7001` unknown key; `E7002` bad value; the lock's and the append's.
pub fn set(core: &mut Core, cap: &OwnerCap, key: &str, values: &[String]) -> Result<TypedValue> {
    let value = settings::parse_value(key, values)?;
    let ev = changed(core, key, Some(&value))?;
    let mut locked = core.lock(|_, _| {})?;
    locked.commit(&owner(cap), &[ev])?;
    locked.release()?;
    core.reload_settings()?;
    Ok(value)
}

/// `tm settings unset KEY`: back to the default.
///
/// # Errors
/// `E7001` unknown key; the lock's and the append's.
pub fn unset(core: &mut Core, cap: &OwnerCap, key: &str) -> Result<()> {
    settings::def(key)?;
    let ev = changed(core, key, None)?;
    let mut locked = core.lock(|_, _| {})?;
    locked.commit(&owner(cap), &[ev])?;
    locked.release()?;
    core.reload_settings()
}

/// `tm settings show` (section 10's `settings.show`): every key with its
/// value, default, and the event that set it.
#[must_use]
pub fn show(core: &Core) -> Value {
    core.settings.show()
}
