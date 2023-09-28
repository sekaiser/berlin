mod auth_code;
mod client_creds;
pub mod clients;
pub mod sync;
mod util;

pub use linkedin_http as http;
pub use linkedin_macros as macros;
pub use linkedin_model as model;

pub use auth_code::AuthCodeLinkedIn;
pub use client_creds::ClientCredsLinkedIn;

use crate::{http::HttpError, model::Id};
pub use macros::scopes;
pub use model::Token;

use std::{
    collections::{HashMap, HashSet},
    env,
    path::PathBuf,
};

use getrandom::getrandom;
use thiserror::Error;

pub mod prelude {
    pub use crate::clients::{BaseClient, OAuthClient};
    pub use crate::model::idtypes::Id;
}

/// Common headers as constants.
pub(crate) mod params {
    pub const CLIENT_ID: &str = "client_id";
    pub const CLIENT_SECRET: &str = "client_secret";
    pub const CODE: &str = "code";
    pub const GRANT_TYPE: &str = "grant_type";
    pub const GRANT_TYPE_AUTH_CODE: &str = "authorization_code";
    pub const GRANT_TYPE_CLIENT_CREDS: &str = "client_credentials";
    pub const GRANT_TYPE_REFRESH_TOKEN: &str = "refresh_token";
    pub const REDIRECT_URI: &str = "redirect_uri";
    pub const REFRESH_TOKEN: &str = "refresh_token";
    pub const RESPONSE_TYPE_CODE: &str = "code";
    pub const RESPONSE_TYPE: &str = "response_type";
    pub const SCOPE: &str = "scope";
    pub const SHOW_DIALOG: &str = "show_dialog";
    pub const STATE: &str = "state";
    pub const CODE_CHALLENGE: &str = "code_challenge";
    pub const CODE_VERIFIER: &str = "code_verifier";
    pub const CODE_CHALLENGE_METHOD: &str = "code_challenge_method";
    pub const CODE_CHALLENGE_METHOD_S256: &str = "S256";
}

/// Common alphabets for random number generation and similars
pub(crate) mod alphabets {
    pub const ALPHANUM: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    /// From <https://datatracker.ietf.org/doc/html/rfc7636#section-4.1>
    pub const PKCE_CODE_VERIFIER: &[u8] =
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~";
}

pub(crate) mod auth_urls {
    pub const AUTHORIZE: &str = "authorization";
    pub const TOKEN: &str = "accessToken";
}

// Possible errors returned from the `rspotify` client.
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("json parse error: {0}")]
    ParseJson(#[from] libs::serde_json::Error),

    #[error("url parse error: {0}")]
    ParseUrl(#[from] libs::url::ParseError),

    // Note that this type is boxed because its size might be very large in
    // comparison to the rest. For more information visit:
    // https://rust-lang.github.io/rust-clippy/master/index.html#large_enum_variant
    #[error("http error: {0}")]
    Http(Box<HttpError>),

    #[error("input/output error: {0}")]
    Io(#[from] std::io::Error),

    #[cfg(feature = "cli")]
    #[error("cli error: {0}")]
    Cli(String),

    #[error("cache file error: {0}")]
    CacheFile(String),

    #[error("model error: {0}")]
    Model(#[from] model::ModelError),
}

// The conversion has to be done manually because it's in a `Box<T>`
impl From<HttpError> for ClientError {
    fn from(err: HttpError) -> Self {
        Self::Http(Box::new(err))
    }
}

pub type ClientResult<T> = Result<T, ClientError>;

pub const DEFAULT_API_BASE_URL: &str = "https://api.linkedin.com/v2/";
pub const DEFAULT_AUTH_BASE_URL: &str = "https://www.linkedin.com/oauth/v2";
pub const DEFAULT_CACHE_PATH: &str = ".linkedin_token_cache.json";

/// Struct to configure the LinkedIn client.
#[derive(Debug, Clone)]
pub struct Config {
    /// The LinkedIn API prefix, [`DEFAULT_API_BASE_URL`] by default.
    pub api_base_url: String,

    /// The LinkedIn Authentication prefix, [`DEFAULT_AUTH_BASE_URL`] by default.
    pub auth_base_url: String,

    /// The cache file path, in case it's used. By default it's
    /// [`DEFAULT_CACHE_PATH`]
    pub cache_path: PathBuf,

    /// Whether or not to save the authentication token into a JSON file,
    /// then reread the token from JSON file when launching the program without
    /// following the full auth process again
    pub token_cached: bool,

    /// Whether or not to check if the token has expired when sending a
    /// request with credentials, and in that case, automatically refresh it.
    pub token_refreshing: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_base_url: String::from(DEFAULT_API_BASE_URL),
            auth_base_url: String::from(DEFAULT_AUTH_BASE_URL),
            cache_path: PathBuf::from(DEFAULT_CACHE_PATH),
            token_cached: false,
            token_refreshing: false,
        }
    }
}

/// Generate `length` random chars from the Operating System.
///
/// It is assumed that system always provides high-quality cryptographically
/// secure random data, ideally backed by hardware entropy sources.
pub(crate) fn generate_random_string(length: usize, alphabet: &[u8]) -> String {
    let mut buf = vec![0u8; length];
    getrandom(&mut buf).unwrap();
    let range = alphabet.len();

    buf.iter()
        .map(|byte| alphabet[*byte as usize % range] as char)
        .collect()
}

#[inline]
pub(crate) fn join_ids<'a, T: Id + 'a>(ids: impl IntoIterator<Item = T>) -> String {
    let ids = ids.into_iter().collect::<Vec<_>>();
    ids.iter().map(Id::id).collect::<Vec<_>>().join(",")
}

#[inline]
pub(crate) fn join_scopes(scopes: &HashSet<String>) -> String {
    scopes
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Simple client credentials object for LinkedIn.
#[derive(Debug, Clone, Default)]
pub struct Credentials {
    pub id: String,
    /// PKCE doesn't require a client secret
    pub secret: Option<String>,
}

impl Credentials {
    /// Initialization with both the client ID and the client secret
    #[must_use]
    pub fn new(id: &str, secret: &str) -> Self {
        Self {
            id: id.to_owned(),
            secret: Some(secret.to_owned()),
        }
    }

    /// Initialization with just the client ID
    #[must_use]
    pub fn new_pkce(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            secret: None,
        }
    }

    /// Parses the credentials from the environment variables
    /// `LINKEDIN_CLIENT_ID` and `LINKED_CLIENT_SECRET`. You can optionally
    /// activate the `env-file` feature in order to read these variables from
    /// a `.env` file.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        #[cfg(feature = "env-file")]
        {
            dotenv::dotenv().ok();
        }

        Some(Self {
            id: env::var("LINKEDIN_CLIENT_ID").ok()?,
            secret: env::var("LINKEDIN_CLIENT_SECRET").ok(),
        })
    }

    /// Generates an HTTP basic authorization header with proper formatting
    ///
    /// This will only work when the client secret is set to `Option::Some`.
    #[must_use]
    pub fn auth_headers(&self) -> Option<HashMap<String, String>> {
        let auth = "authorization".to_owned();
        let value = format!("{}:{}", self.id, self.secret.as_ref()?);
        let value = format!("Basic {}", base64::encode(value));

        let mut headers = HashMap::new();
        headers.insert(auth, value);
        Some(headers)
    }
}

/// Structure that holds the required information for requests with OAuth.
#[derive(Debug, Clone)]
pub struct OAuth {
    pub redirect_uri: String,
    /// The state is generated by default, as suggested by the OAuth2 spec:
    /// [Cross-Site Request Forgery](https://tools.ietf.org/html/rfc6749#section-10.12)
    pub state: String,
    /// You could use macro [scopes!](crate::scopes) to build it at compile time easily
    pub scopes: HashSet<String>,
    pub proxies: Option<String>,
}

impl Default for OAuth {
    fn default() -> Self {
        Self {
            redirect_uri: String::new(),
            state: generate_random_string(16, alphabets::ALPHANUM),
            scopes: HashSet::new(),
            proxies: None,
        }
    }
}

impl OAuth {
    /// Parses the credentials from the environment variable
    /// `LINKEDIN_REDIRECT_URI`. You can optionally activate the `env-file`
    /// feature in order to read these variables from a `.env` file.
    #[must_use]
    pub fn from_env(scopes: HashSet<String>) -> Option<Self> {
        #[cfg(feature = "env-file")]
        {
            dotenv::dotenv().ok();
        }

        Some(Self {
            scopes,
            redirect_uri: env::var("LINKEDIN_REDIRECT_URI").ok()?,
            ..Default::default()
        })
    }
}
