use super::*;
use url::Url;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkedInAction {
    Status {
        account: String,
    },
    Connect {
        account: String,
        token_env: String,
        confirm: Option<String>,
        version: String,
        organization: Option<String>,
    },
    AuthStart {
        account: String,
        client_id: String,
        redirect: Url,
        version: String,
        organization: Option<String>,
    },
    AuthFinish {
        account: String,
        callback: PathBuf,
        secret_env: String,
    },
    Reconcile {
        review: String,
        post: String,
        reported: bool,
        token_env: Option<String>,
    },
    CallbackPage {
        url: Url,
        output: PathBuf,
    },
}
fn account(command: Command) -> Command {
    command.arg(Arg::new("account").long("account").required(true))
}
fn version(command: Command) -> Command {
    command.arg(
        Arg::new("api-version")
            .long("api-version")
            .default_value(crate::tasks::announcement::linkedin::API_VERSION),
    )
}
fn organization(command: Command) -> Command {
    command.arg(
        Arg::new("organization")
            .long("organization")
            .value_name("ORGANIZATION_URN")
            .help("Post as this company Page; requires its organization API permissions"),
    )
}
pub(super) fn command() -> Command {
    Command::new("linkedin")
        .about("Authorize a LinkedIn member or company Page and reconcile explicit API submissions")
        .subcommand_required(true)
        .subcommand(account(
            Command::new("status")
                .about("Show the saved member and author binding; no network or token output"),
        ))
        .subcommand(organization(version(account(
            Command::new("connect")
                .about("Inspect an existing access token; save only with --confirm-member")
                .arg(
                    Arg::new("token-env")
                        .long("token-env")
                        .default_value("LINKEDIN_ACCESS_TOKEN"),
                )
                .arg(Arg::new("confirm-member").long("confirm-member")),
        ))))
        .subcommand(organization(version(account(
            Command::new("auth-start")
                .about("Create an OAuth authorization URL; no network or browser automation")
                .arg(Arg::new("client-id").long("client-id").required(true))
                .arg(
                    Arg::new("redirect-uri")
                        .long("redirect-uri")
                        .required(true)
                        .value_parser(clap::value_parser!(Url)),
                ),
        ))))
        .subcommand(account(
            Command::new("auth-finish")
                .about("Exchange the saved OAuth response and bind the authenticated member and author")
                .arg(
                    Arg::new("callback-file")
                        .long("callback-file")
                        .required(true)
                        .value_parser(clap::value_parser!(PathBuf)),
                )
                .arg(
                    Arg::new("secret-env")
                        .long("secret-env")
                        .default_value("LINKEDIN_CLIENT_SECRET"),
                ),
        ))
        .subcommand(
            Command::new("reconcile")
                .about("Resolve an uncertain submission by API read-back or explicit user report")
                .arg(Arg::new("review").required(true))
                .arg(Arg::new("post-urn").long("post-urn").required(true))
                .arg(
                    Arg::new("reported")
                        .long("reported")
                        .action(ArgAction::SetTrue)
                        .help("Record your own verification; does not claim API verification"),
                )
                .arg(Arg::new("token-env").long("token-env")),
        )
        .subcommand(
            Command::new("callback-page")
                .about("Write a standalone OAuth callback/download page for your own HTTPS site")
                .arg(
                    Arg::new("url")
                        .long("url")
                        .required(true)
                        .value_parser(clap::value_parser!(Url)),
                )
                .arg(
                    Arg::new("output")
                        .long("output")
                        .required(true)
                        .value_parser(clap::value_parser!(PathBuf)),
                ),
        )
}
pub(super) fn parse(args: &ArgMatches) -> LinkedInAction {
    let (name, args) = args.subcommand().unwrap();
    let s = |name| args.get_one::<String>(name).unwrap().clone();
    match name {
        "status" => LinkedInAction::Status {
            account: s("account"),
        },
        "connect" => LinkedInAction::Connect {
            account: s("account"),
            token_env: s("token-env"),
            confirm: args.get_one::<String>("confirm-member").cloned(),
            version: s("api-version"),
            organization: args.get_one::<String>("organization").cloned(),
        },
        "auth-start" => LinkedInAction::AuthStart {
            account: s("account"),
            client_id: s("client-id"),
            redirect: args.get_one::<Url>("redirect-uri").unwrap().clone(),
            version: s("api-version"),
            organization: args.get_one::<String>("organization").cloned(),
        },
        "auth-finish" => LinkedInAction::AuthFinish {
            account: s("account"),
            callback: args.get_one::<PathBuf>("callback-file").unwrap().clone(),
            secret_env: s("secret-env"),
        },
        "reconcile" => LinkedInAction::Reconcile {
            review: s("review"),
            post: s("post-urn"),
            reported: args.get_flag("reported"),
            token_env: args.get_one::<String>("token-env").cloned(),
        },
        "callback-page" => LinkedInAction::CallbackPage {
            url: args.get_one::<Url>("url").unwrap().clone(),
            output: args.get_one::<PathBuf>("output").unwrap().clone(),
        },
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn company_author_is_preserved_in_oauth_and_existing_token_arguments() {
        let args = command()
            .try_get_matches_from([
                "linkedin",
                "auth-start",
                "--account",
                "company",
                "--client-id",
                "client",
                "--redirect-uri",
                "https://example.test/callback",
                "--organization",
                "urn:li:organization:12345",
            ])
            .unwrap();
        assert!(
            matches!(parse(&args), LinkedInAction::AuthStart { organization: Some(value), .. } if value == "urn:li:organization:12345")
        );
        let args = command()
            .try_get_matches_from([
                "linkedin",
                "connect",
                "--account",
                "company",
                "--organization",
                "urn:li:organization:12345",
                "--confirm-member",
                "urn:li:person:member",
            ])
            .unwrap();
        assert!(
            matches!(parse(&args), LinkedInAction::Connect { organization: Some(value), confirm: Some(member), .. } if value == "urn:li:organization:12345" && member == "urn:li:person:member")
        );
    }
}
