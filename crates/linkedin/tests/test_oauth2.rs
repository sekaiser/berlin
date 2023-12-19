use libs::chrono::prelude::*;
use libs::chrono::Duration;

use libs::url::Url;
use linkedin::{
    prelude::*, scopes, AuthCodeLinkedIn, ClientCredsLinkedIn, Config, Credentials, OAuth, Token,
};
use std::{collections::HashMap, fs, io::Read, path::PathBuf};

#[test]
fn test_get_authorize_url() {
    let oauth = OAuth {
        state: "fdsafdsfa".to_owned(),
        redirect_uri: "localhost".to_owned(),
        scopes: scopes!("profile"),
        ..Default::default()
    };
    let creds = Credentials::new("this-is-my-client-id", "this-is-my-client-secret");

    let linkedin = AuthCodeLinkedIn::new(creds, oauth);

    let authorize_url = linkedin.get_authorize_url(false).unwrap();
    let hash_query: HashMap<_, _> = Url::parse(&authorize_url)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();

    assert_eq!(hash_query.get("client_id").unwrap(), "this-is-my-client-id");
    assert_eq!(hash_query.get("response_type").unwrap(), "code");
    assert_eq!(hash_query.get("redirect_uri").unwrap(), "localhost");
    assert_eq!(hash_query.get("scope").unwrap(), "profile");
    assert_eq!(hash_query.get("state").unwrap(), "fdsafdsfa");
}
