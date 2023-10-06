use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::*;
use dialoguer::console::Term;
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use either::Either;
use git_url_parse::GitUrl;
use http::header::ACCEPT;
use octocrab::auth::{Continue, OAuth};
use octocrab::models::checks::ListCheckRuns;
use octocrab::params::repos::Commitish;
use octocrab::OctocrabBuilder;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::process::Command;
use tracing::debug;
use which::which;

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct CLIArgs {
    #[arg(long, short)]
    cwd: Option<PathBuf>,
    #[command(subcommand)]
    command: CLICommand,
}

#[derive(Debug, Clone, Subcommand)]
enum CLICommand {
    Status,
    Open,
    Login,
    Logout
}

#[derive(Debug)]
struct GitHubRepository {
    owner: String,
    repo: String,
}

struct Git {
    bin: PathBuf,
    cwd: Option<PathBuf>,
}

const GITHUB_CLIENT_ID: &str = "Iv1.6759afe4a207433f";

impl Git {
    fn new(cwd: Option<PathBuf>) -> Result<Self> {
        let bin = which("git")?;
        Ok(Self { bin, cwd })
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(&self.bin);
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        cmd
    }

    async fn get_current_ref(&self) -> Result<String> {
        let output = self
            .cmd()
            .arg("rev-parse")
            .arg("--short")
            .arg("HEAD")
            .output()
            .await?;

        let git_ref = String::from_utf8(output.stdout)?;
        Ok(git_ref.trim().to_string())
    }

    // Gets the name version of ref, i.e. `main`
    async fn get_current_ref_as_name(&self) -> Result<String> {
        let output = self
            .cmd()
            .arg("rev-parse")
            .arg("--abbrev-ref")
            .arg("HEAD")
            .output()
            .await?;

        let git_ref = String::from_utf8(output.stdout)?;
        Ok(git_ref.trim().to_string())
    }

    // Uses `git config --get remote.origin.url` to get url and parses
    // it to get owner/repo
    async fn get_github_repo(&self) -> Result<GitHubRepository> {
        let output = self
            .cmd()
            .arg("config")
            .arg("--get")
            .arg("remote.origin.url")
            .output()
            .await?;

        let url = String::from_utf8(output.stdout)?;
        let url = url.trim();
        let git_url = GitUrl::parse(url).map_err(|_| anyhow!("unable to parse git remote. Please supply the owner and repository name manually with `--owner` and `--repo`"))?;
        Ok(GitHubRepository {
            owner: git_url.owner.ok_or(anyhow!("unable to parse git remote. Please supply the owner and repository name manually with `--owner` and `--repo`"))?,
            repo: git_url.name,
        })
    }
}

fn print_check_runs(git_ref: &str, runs: ListCheckRuns) {
    println!("Found {} runs for {}\n", runs.total_count, git_ref);
    let max_len = runs
        .check_runs
        .iter()
        .map(|run| run.name.len())
        .max()
        .unwrap_or_default();

    for run in runs.check_runs {
        let conclusion = match run.conclusion.as_deref() {
            Some("success") => "🟢",
            Some("failure") => "🔴",
            Some("neutral") => "⚪",
            Some("cancelled") => "❌",
            Some("timed_out") => "⌛",
            Some("action_required") => "🔧",
            Some(conclusion) => conclusion,
            None => "🟡",
        };

        println!(
            "{:width$}   {}",
            run.name.bold(),
            conclusion,
            width = max_len
        );
    }
}

async fn get_runs_for_current_branch(cwd: Option<PathBuf>) -> Result<(ListCheckRuns, String)> {
    let git = Git::new(cwd)?;
    let git_ref = git.get_current_ref().await?;
    debug!("found git ref for current branch: {}", git_ref);
    let octocrab = match AuthConfig::load() {
        Ok(auth) => Arc::new(
            OctocrabBuilder::new()
                .user_access_token(auth.access_token)
                .build()?,
        ),
        Err(err) => {
            debug!("failed to load authentication config: {}", err);
            debug!("falling back to default octocrab instance");

            octocrab::instance()
        }
    };

    let github_repo = git.get_github_repo().await?;

    let runs = octocrab
        .checks(github_repo.owner, github_repo.repo)
        .list_check_runs_for_git_ref(Commitish(git_ref.clone()))
        .send()
        .await
        .map_err(|err| {
            if matches!(err, octocrab::Error::GitHub { .. }) {
                println!("{}", "Failed to fetch check runs. Is your repository private? If so, you should log into your GitHub account with `fj login`".yellow());
            }

            err
        })?;

    let pretty_git_ref = git.get_current_ref_as_name().await?;

    Ok((runs, pretty_git_ref))
}

// Idk kinda arbitrary
const RETRY_LIMIT: usize = 10;

#[derive(Debug, Serialize, Deserialize)]
struct AuthConfig {
    access_token: String,
    token_type: String,
    scope: Vec<String>,
}

impl From<OAuth> for AuthConfig {
    fn from(oauth: OAuth) -> Self {
        Self {
            access_token: oauth.access_token.expose_secret().to_string(),
            token_type: oauth.token_type,
            scope: oauth.scope,
        }
    }
}

impl AuthConfig {
    pub fn load() -> Result<Self> {
        let base_dirs = xdg::BaseDirectories::with_prefix("fj")?;
        let config_file_path = base_dirs.get_config_file("github.toml");
        let config_file = fs::read_to_string(config_file_path)?;
        Ok(toml::from_str(&config_file)?)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = CLIArgs::parse();

    match args.command {
        CLICommand::Status => {
            let (runs, git_ref) = get_runs_for_current_branch(args.cwd).await?;
            print_check_runs(&git_ref, runs);
        }
        CLICommand::Open => {
            let (runs, git_ref) = get_runs_for_current_branch(args.cwd).await?;
            let items = runs
                .check_runs
                .iter()
                .map(|run| run.name.to_string())
                .collect::<Vec<_>>();

            println!("Found {} runs for {}", runs.total_count, git_ref);
            let selection = FuzzySelect::with_theme(&ColorfulTheme::default())
                .items(&items)
                .default(0)
                .interact_on_opt(&Term::stderr())?;

            if let Some(index) = selection {
                if let Some(url) = &runs.check_runs[index].html_url {
                    webbrowser::open(url)?;
                } else {
                    eprintln!("No url found for run `{}`", runs.check_runs[index].name);
                }
            } else {
                eprintln!("No run selected");
            }
        }
        CLICommand::Logout => {
            let base_dirs = xdg::BaseDirectories::with_prefix("fj")?;
            let config_file_path = base_dirs.place_config_file("github.toml")?;
            fs::remove_file(&config_file_path)?;
            println!("Successfully logged out");
        }
        CLICommand::Login => {
            let octocrab = octocrab::Octocrab::builder()
                .base_uri("https://github.com")?
                .add_header(ACCEPT, "application/json".to_string())
                .build()?;

            let client_id = SecretString::from_str(GITHUB_CLIENT_ID)?;
            let device_codes = octocrab
                .authenticate_as_device(&client_id, &["repo"])
                .await?;

            println!(
                "Please enter the code {} at {}",
                device_codes.user_code, device_codes.verification_uri
            );
            webbrowser::open(&device_codes.verification_uri)?;

            let mut poll_duration = tokio::time::Duration::from_secs(device_codes.interval);

            for _ in 0..RETRY_LIMIT {
                match device_codes.poll_once(&octocrab, &client_id).await {
                    Ok(Either::Left(auth)) => {
                        let base_dirs = xdg::BaseDirectories::with_prefix("fj")?;
                        let config_file_path = base_dirs.place_config_file("github.toml")?;

                        debug!("config path is {}", config_file_path.display());
                        let auth: AuthConfig = auth.into();
                        fs::write(config_file_path, toml::to_string(&auth)?)?;

                        println!("Successfully logged in!");
                        break;
                    }
                    Ok(Either::Right(Continue::AuthorizationPending)) => {
                        tokio::time::sleep(poll_duration).await;
                    }
                    Ok(Either::Right(Continue::SlowDown)) => {
                        // Back off because we're polling too fast
                        poll_duration *= 2;
                    }
                    Err(err) => {
                        println!("Error: {}", err);
                        println!("Retrying in {} seconds", poll_duration.as_secs());
                        tokio::time::sleep(poll_duration).await;
                        // Back off just in case
                        poll_duration *= 2;
                    }
                }
            }
        }
    }

    Ok(())
}
