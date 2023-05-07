//! This example is specially useful for the OAuth tests. It simply obtains an
//! access token and a refresh token with all available scopes.
//!
//! Set LINKEDIN_CLIENT_ID, LINKEDIN_CLIENT_SECRET and LINKEDIN_REDIRECT_URI in
//! an .env file or export them manually as environmental variables for this to
//! work.

use linkedin::{prelude::*, scopes, AuthCodeLinkedIn, Credentials, OAuth};

#[tokio::main]
async fn main() {
    // You can use any logger for debugging.
    env_logger::init();

    // The credentials must be available in the environment. Enable the
    // `env-file` feature in order to read them from an `.env` file.
    let creds = Credentials::from_env().unwrap();

    // Using every possible scope
    let scopes = scopes!(
        "w_member_social",
        "r_emailaddress",
        "openid",
        "profile",
        "r_liteprofile",
        "email"
    );
    let oauth = OAuth::from_env(scopes).unwrap();

    let linkedin = AuthCodeLinkedIn::new(creds, oauth);

    let url = linkedin.get_authorize_url(false).unwrap();
    // This function requires the `cli` feature enabled.
    linkedin.prompt_for_token(&url).await.unwrap();

    let token = linkedin.token.lock().await.unwrap();
    println!("Access token: {}", &token.as_ref().unwrap().access_token);

    // Programmatic refresh tokens are available for a limited set of
    // partners. If this feature has been enabled for your application,
    // see Programmatic Refresh Tokens for instructions.
    // link: https://learn.microsoft.com/en-us/linkedin/shared/authentication/authorization-code-flow?tabs=HTTPS1
    // println!(
    //     "Refresh token: {}",
    //     token.as_ref().unwrap().refresh_token.as_ref().unwrap()
    // );
}
