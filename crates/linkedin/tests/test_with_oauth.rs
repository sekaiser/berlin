use linkedin::{prelude::*, scopes, AuthCodeLinkedIn, ClientResult, Credentials, OAuth, Token};

use std::env;

use chrono::{prelude::*, Duration};
use maybe_async::maybe_async;

/// Generating a new OAuth client for the requests.
#[maybe_async]
pub async fn oauth_client() -> AuthCodeLinkedIn {
    if let Ok(access_token) = env::var("LINKEDIN_ACCESS_TOKEN") {
        let tok = Token {
            access_token,
            ..Default::default()
        };

        AuthCodeLinkedIn::from_token(tok)
    } else if let Ok(refresh_token) = env::var("LINKEDIN_REFRESH_TOKEN") {
        // The credentials must be available in the environment. Enable
        // `env-file` in order to read them from an `.env` file.
        let creds = Credentials::from_env().unwrap_or_else(|| {
            panic!(
                "No credentials configured. Make sure that either the \
                `env-file` feature is enabled, or that the required \
                environment variables are exported (`LINKEDIN_CLIENT_ID`, \
                `LINKEDIN_CLIENT_SECRET`)."
            )
        });

        let scopes = scopes!("openid", "profile", "email");
        // Using every possible scope
        let oauth = OAuth::from_env(scopes).unwrap();

        // Creating a token with only the refresh token in order to obtain the
        // access token later.
        let token = Token {
            refresh_token: Some(refresh_token),
            ..Default::default()
        };

        let spotify = AuthCodeLinkedIn::new(creds, oauth);
        *spotify.token.lock().await.unwrap() = Some(token);
        spotify.refresh_token().await.unwrap();
        spotify
    } else {
        panic!(
            "No access tokens configured. Please set `LINKEDIN_ACCESS_TOKEN` \
             or `LINKEDIN_REFRESH_TOKEN`, which can be obtained with the \
             `oauth_tokens` example."
        )
    }
}
