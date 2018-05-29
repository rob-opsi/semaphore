use serde::de::DeserializeOwned;
use serde::ser::Serialize;

/// An auxiliary trait for cachable items.
pub trait Cachable: Serialize + DeserializeOwned {
    /// Returns the cache version.
    ///
    /// If the layout of the cachable object changes the version
    /// needs to be bumped.
    fn cache_version() -> u32;
}
