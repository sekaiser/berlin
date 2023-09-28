mod base;
mod oauth;

pub use base::BaseClient;
pub use oauth::OAuthClient;

use crate::ClientResult;

use serde::Deserialize;

/// Converts a JSON response from LinkedIn into its model.
pub(crate) fn convert_result<'a, T: Deserialize<'a>>(input: &'a str) -> ClientResult<T> {
    libs::serde_json::from_str::<T>(input).map_err(Into::into)
}
