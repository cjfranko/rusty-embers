//! User-defined callback trait for Ember+ provider events.

use crate::glow::GlowValue;
use crate::Result;

/// Application-provided handler for provider events.
pub trait Handler: Send + Sync {
    /// Called when a consumer writes a new value to a parameter.
    fn on_value_change(&self, _path: &[u32], _value: &GlowValue) -> Result<()> {
        Ok(())
    }

    /// Called when a consumer invokes a Function.
    fn on_invoke(&self, _path: &[u32], _args: &[GlowValue]) -> Result<Vec<GlowValue>> {
        Ok(Vec::new())
    }

    /// Called to read the current value of a parameter.
    fn get_value(&self, _path: &[u32]) -> Option<GlowValue> {
        None
    }
}
