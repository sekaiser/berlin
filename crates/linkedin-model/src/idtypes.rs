use enum_dispatch::enum_dispatch;
use libs::strum::Display;
use thiserror::Error;

use std::fmt::Debug;

/// Spotify ID or URI parsing error
///
/// See also [`Id`](crate::idtypes::Id) for details.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Display, Error)]
pub enum IdError {
    /// Spotify URI prefix is not `spotify:` or `spotify/`.
    InvalidPrefix,
    /// Spotify URI can't be split into type and id parts (e.g., it has invalid
    /// separator).
    InvalidFormat,
    /// Spotify URI has invalid type name, or id has invalid type in a given
    /// context (e.g. a method expects a track id, but artist id is provided).
    InvalidType,
    /// Spotify id is invalid (empty or contains invalid characters).
    InvalidId,
}

/// The main interface for an ID.
///
/// See the [module level documentation] for more information.
///
/// [module level documentation]: [`crate::idtypes`]
#[enum_dispatch]
pub trait Id {
    /// Returns the inner Spotify object ID, which is guaranteed to be valid for
    /// its type.
    fn id(&self) -> &str;
}
