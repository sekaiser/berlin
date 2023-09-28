use crate::{
    auth_urls,
    clients::{BaseClient, OAuthClient},
    http::{Form, HttpClient},
    join_scopes, params,
    sync::Mutex,
    ClientResult, Config, Credentials, OAuth, Token,
};

use std::collections::HashMap;
use std::sync::Arc;

use libs::log;
use libs::url::Url;
use maybe_async::maybe_async;

#[derive(Clone, Debug, Default)]
pub struct AuthCodeLinkedIn {
    pub creds: Credentials,
    pub oauth: OAuth,
    pub config: Config,
    pub token: Arc<Mutex<Option<Token>>>,
    pub(crate) http: HttpClient,
}

/// This client has access to the base methods.
#[maybe_async]
impl BaseClient for AuthCodeLinkedIn {
    fn get_http(&self) -> &HttpClient {
        &self.http
    }

    fn get_token(&self) -> Arc<Mutex<Option<Token>>> {
        Arc::clone(&self.token)
    }

    fn get_creds(&self) -> &Credentials {
        &self.creds
    }

    fn get_config(&self) -> &Config {
        &self.config
    }

    /// Refetch the current access token given a refresh token. May return
    /// `None` if there's no access/refresh token.
    async fn refetch_token(&self) -> ClientResult<Option<Token>> {
        match self.token.lock().await.unwrap().as_ref() {
            Some(Token {
                refresh_token: Some(refresh_token),
                ..
            }) => {
                let mut data = Form::new();
                data.insert(params::REFRESH_TOKEN, refresh_token);
                data.insert(params::GRANT_TYPE, params::REFRESH_TOKEN);

                let headers = self
                    .creds
                    .auth_headers()
                    .expect("No client secret set in the credentials.");
                let mut token = self.fetch_access_token(&data, Some(&headers)).await?;
                token.refresh_token = Some(refresh_token.to_string());
                Ok(Some(token))
            }
            _ => Ok(None),
        }
    }
}

/// This client includes user authorization, so it has access to the user
/// private endpoints in [`OAuthClient`].
#[maybe_async]
impl OAuthClient for AuthCodeLinkedIn {
    fn get_oauth(&self) -> &OAuth {
        &self.oauth
    }

    /// Obtains a user access token given a code, as part of the OAuth
    /// authentication. The access token will be saved internally.
    async fn request_token(&self, code: &str) -> ClientResult<()> {
        log::info!("Requesting Auth Code token");

        let scopes = join_scopes(&self.oauth.scopes);
        let client_secret = self.creds.secret.as_ref().unwrap();
        let mut data = Form::new();
        data.insert(params::GRANT_TYPE, params::GRANT_TYPE_AUTH_CODE);
        data.insert(params::REDIRECT_URI, &self.oauth.redirect_uri);
        data.insert(params::CLIENT_ID, &self.creds.id);
        data.insert(params::CLIENT_SECRET, &client_secret);
        data.insert(params::CODE, code);
        data.insert(params::SCOPE, &scopes);
        data.insert(params::STATE, &self.oauth.state);

        let headers = self
            .creds
            .auth_headers()
            .expect("No client secret set in the credentials.");

        let token = self.fetch_access_token(&data, Some(&headers)).await?;
        *self.token.lock().await.unwrap() = Some(token);

        self.write_token_cache().await
    }
}

impl AuthCodeLinkedIn {
    /// Builds a new [`AuthCodeSpotify`] given a pair of client credentials and
    /// OAuth information.
    #[must_use]
    pub fn new(creds: Credentials, oauth: OAuth) -> Self {
        Self {
            creds,
            oauth,
            ..Default::default()
        }
    }

    /// Build a new [`AuthCodeSpotify`] from an already generated token. Note
    /// that once the token expires this will fail to make requests, as the
    /// client credentials aren't known.
    #[must_use]
    pub fn from_token(token: Token) -> Self {
        Self {
            token: Arc::new(Mutex::new(Some(token))),
            ..Default::default()
        }
    }

    /// Same as [`Self::new`] but with an extra parameter to configure the
    /// client.
    #[must_use]
    pub fn with_config(creds: Credentials, oauth: OAuth, config: Config) -> Self {
        Self {
            creds,
            oauth,
            config,
            ..Default::default()
        }
    }

    /// Returns the URL needed to authorize the current client as the first step
    /// in the authorization flow.
    pub fn get_authorize_url(&self, show_dialog: bool) -> ClientResult<String> {
        log::info!("Building auth URL");

        let scopes = join_scopes(&self.oauth.scopes);

        let mut payload: HashMap<&str, &str> = HashMap::new();
        payload.insert(params::CLIENT_ID, &self.creds.id);
        payload.insert(params::RESPONSE_TYPE, params::RESPONSE_TYPE_CODE);
        payload.insert(params::REDIRECT_URI, &self.oauth.redirect_uri);
        payload.insert(params::SCOPE, &scopes);
        payload.insert(params::STATE, &self.oauth.state);

        if show_dialog {
            payload.insert(params::SHOW_DIALOG, "true");
        }

        let request_url = self.auth_url(auth_urls::AUTHORIZE);
        let parsed = Url::parse_with_params(&request_url, payload)?;
        Ok(parsed.into())
    }
}
