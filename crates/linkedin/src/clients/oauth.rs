use crate::{
    clients::BaseClient,
    http::Query,
    util::{build_map, JsonBuilder},
    ClientResult, OAuth, Token,
};

use std::collections::HashMap;

use maybe_async::maybe_async;
use serde_json::{json, Map};
use url::Url;

/// This trait implements the methods available strictly to clients with user
/// authorization, including some parts of the authentication flow that are
/// shared, and the endpoints.
#[maybe_async]
pub trait OAuthClient: BaseClient {
    fn get_oauth(&self) -> &OAuth;

    /// Obtains a user access token given a code, as part of the OAuth
    /// authentication. The access token will be saved internally.
    async fn request_token(&self, code: &str) -> ClientResult<()>;

    /// Tries to read the cache file's token.
    ///
    /// This will return an error if the token could not be read (e.g. it is not
    /// available or the JSON is malformed). It may return `Ok(None)` if:
    ///
    /// * The read token is expired and `allow_expired` is false
    /// * Its scopes do not match with the current client (you will need to
    ///   re-authenticate to gain access to more scopes)
    /// * The cached token is disabled in the config
    async fn read_token_cache(&self, allow_expired: bool) -> ClientResult<Option<Token>> {
        if !self.get_config().token_cached {
            log::info!("Auth token cache read ignored (not configured)");
            return Ok(None);
        }

        log::info!("Reading auth token cache");
        let token = Token::from_cache(&self.get_config().cache_path)?;
        if !self.get_oauth().scopes.is_subset(&token.scopes)
            || (!allow_expired && token.is_expired())
        {
            // Invalid token since it does not have at least the currently
            // required scopes or it is expired.
            Ok(None)
        } else {
            Ok(Some(token))
        }
    }

    /// Parse the response code in the given response url. If the URL cannot be
    /// parsed or the `code` parameter is not present, this will return `None`.
    ///
    /// As the [RFC 6749 indicates](https://datatracker.ietf.org/doc/html/rfc6749#section-4.1),
    /// the state should be the same between the request and the callback. This
    /// will also return `None` if this is not true.
    fn parse_response_code(&self, url: &str) -> Option<String> {
        let url = Url::parse(url).ok()?;
        let params = url.query_pairs().collect::<HashMap<_, _>>();
        let code = params.get("code")?;

        // Making sure the state is the same
        let expected_state = &self.get_oauth().state;
        let state = params.get("state").map(AsRef::as_ref);

        if state != Some(expected_state) {
            log::error!("Request state does not match the callback state");
            return None;
        }

        Some(code.to_string())
    }

    /// Tries to open the authorization URL in the user's browser, and return
    /// the obtained code.
    ///
    /// Note: this method requires the `cli` feature.
    #[cfg(feature = "cli")]
    fn get_code_from_user(&self, url: &str) -> ClientResult<String> {
        use crate::ClientError;

        log::info!("Opening browser with auth URL");
        match webbrowser::open(url) {
            Ok(_) => println!("Opened {} in your browser.", url),
            Err(why) => eprintln!(
                "Error when trying to open an URL in your browser: {:?}. \
                 Please navigate here manually: {}",
                why, url
            ),
        }

        log::info!("Prompting user for code");
        println!("Please enter the URL you were redirected to: ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let code = self
            .parse_response_code(&input)
            .ok_or_else(|| ClientError::Cli("unable to parse the response code".to_string()))?;

        Ok(code)
    }

    /// Opens up the authorization URL in the user's browser so that it can
    /// authenticate. It reads from the standard input the redirect URI
    /// in order to obtain the access token information. The resulting access
    /// token will be saved internally once the operation is successful.
    ///
    /// If the [`Config::token_cached`] setting is enabled for this client,
    /// and a token exists in the cache, the token will be loaded and the client
    /// will attempt to automatically refresh the token if it is expired. If
    /// the token was unable to be refreshed, the client will then prompt the
    /// user for the token as normal.
    ///
    /// Note: this method requires the `cli` feature.
    #[cfg(feature = "cli")]
    #[maybe_async]
    async fn prompt_for_token(&self, url: &str) -> ClientResult<()> {
        match self.read_token_cache(true).await {
            Ok(Some(new_token)) => {
                let expired = new_token.is_expired();

                // Load token into client regardless of whether it's expired o
                // not, since it will be refreshed later anyway.
                *self.get_token().lock().await.unwrap() = Some(new_token);

                if expired {
                    // Ensure that we actually got a token from the refetch
                    match self.refetch_token().await? {
                        Some(refreshed_token) => {
                            log::info!("Successfully refreshed expired token from token cache");
                            *self.get_token().lock().await.unwrap() = Some(refreshed_token)
                        }
                        // If not, prompt the user for it
                        None => {
                            log::info!("Unable to refresh expired token from token cache");
                            let code = self.get_code_from_user(url)?;
                            self.request_token(&code).await?;
                        }
                    }
                }
            }
            // Otherwise following the usual procedure to get the token.
            _ => {
                let code = self.get_code_from_user(url)?;
                self.request_token(&code).await?;
            }
        }

        self.write_token_cache().await
    }
}
