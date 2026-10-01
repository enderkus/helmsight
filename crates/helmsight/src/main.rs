//! Command-line entry point.

mod bootstrap;
mod cli;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "helmsight",
    version,
    about = "Watch every server from one screen. Install nothing on them.",
    long_about = "Agentless monitoring dashboard for Linux servers. Collects metrics and inventory \
over SSH with read-only commands and serves a web UI."
)]
struct Cli {
    /// Configuration file.
    #[arg(short, long, global = true, env = "HELMSIGHT_CONFIG")]
    config: Option<PathBuf>,

    /// Data directory; overrides `server.data_dir`. Without a configuration
    /// file, commands use the local-mode default directory.
    #[arg(long, global = true, env = "HELMSIGHT_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Log format.
    #[arg(long, global = true, default_value = "text", value_parser = ["text", "json"])]
    log_format: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the web server and collectors.
    Serve {
        /// Monitor only this machine by reading /proc directly (no SSH, no config needed).
        #[arg(long)]
        local: bool,
        /// Override `server.listen`.
        #[arg(long)]
        listen: Option<String>,
    },
    /// Create a configuration file, key file and the first administrator.
    ///
    /// Values given as options are used as they are; the others are asked
    /// for, or take their defaults with --non-interactive.
    Init {
        /// Overwrite an existing configuration file.
        #[arg(long)]
        force: bool,
        /// Do not ask questions; use the options and defaults. The first
        /// administrator is only created with --password-stdin.
        #[arg(long)]
        non_interactive: bool,
        /// Address and port to listen on.
        #[arg(long, value_name = "ADDR")]
        listen: Option<String>,
        /// External https:// URL of the UI.
        #[arg(long, value_name = "URL")]
        public_url: Option<String>,
        /// Account on the monitored hosts.
        #[arg(long, value_name = "USER")]
        ssh_user: Option<String>,
        /// SSH private key used to log in to the monitored hosts.
        #[arg(long, value_name = "PATH")]
        ssh_key: Option<PathBuf>,
        /// Create a new ed25519 key at --ssh-key if it does not exist.
        #[arg(long)]
        generate_key: bool,
        /// Name of the first administrator.
        #[arg(long, value_name = "NAME")]
        admin: Option<String>,
        /// Read the administrator's password from standard input.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Manage local user accounts.
    #[command(subcommand)]
    User(UserCommand),
    /// Work with monitored hosts.
    #[command(subcommand)]
    Hosts(HostsCommand),
    /// Validate the configuration.
    #[command(subcommand)]
    Config(ConfigCommand),
    /// Manage secrets stored encrypted in the database.
    #[command(subcommand)]
    Secret(SecretCommand),
    /// Audit log tools.
    #[command(subcommand)]
    Audit(AuditCommand),
}

#[derive(Subcommand)]
enum UserCommand {
    /// Add a user. The password is prompted for (or read from stdin with --password-stdin).
    Add {
        username: String,
        #[arg(long, default_value = "viewer", value_parser = ["viewer", "operator", "admin"])]
        role: String,
        #[arg(long)]
        password_stdin: bool,
    },
    /// Remove a user.
    Remove { username: String },
    /// Set a new password and end the user's sessions.
    ResetPassword {
        username: String,
        #[arg(long)]
        password_stdin: bool,
        /// Also remove two-factor authentication.
        #[arg(long)]
        reset_totp: bool,
    },
    /// List users.
    List,
}

#[derive(Subcommand)]
enum HostsCommand {
    /// Check connectivity, authentication and host keys of all hosts.
    Test {
        /// Only test these hosts.
        names: Vec<String>,
        /// Interactively trust unknown host keys after showing their fingerprint.
        #[arg(long)]
        trust: bool,
    },
    /// Print a script that prepares the monitoring account on a host.
    ///
    /// Run the script as root on each monitored host, for example:
    /// `helmsight hosts bootstrap --from 10.0.0.5 | ssh root@web-1 sh`
    Bootstrap {
        /// Address(es) of this helmsight server as the hosts see it; the key
        /// is only accepted from there (OpenSSH `from=` patterns, e.g.
        /// 10.0.0.5 or 10.0.0.0/24).
        #[arg(long, value_name = "ADDR")]
        from: Option<String>,
        /// Account to create (default: `ssh.user`).
        #[arg(long, value_name = "USER")]
        user: Option<String>,
        /// Private key whose public key is installed (default: the first of
        /// `ssh.identity_files`).
        #[arg(long, value_name = "PATH")]
        key: Option<PathBuf>,
        /// Do not add the account to a group that can read the system
        /// journal (needed for failed login history).
        #[arg(long)]
        no_journal: bool,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Validate the configuration file and print a summary.
    Check,
}

#[derive(Subcommand)]
enum SecretCommand {
    /// Store a secret (value prompted for, or read from stdin with --stdin).
    Set {
        name: String,
        #[arg(long)]
        stdin: bool,
    },
    /// List stored secret names.
    List,
    /// Delete a stored secret.
    Delete { name: String },
}

#[derive(Subcommand)]
enum AuditCommand {
    /// Verify the audit log hash chain.
    Verify,
}

fn init_logging(format: &str) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("HELMSIGHT_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,russh=warn,hyper=warn"));
    let ansi = std::io::IsTerminal::is_terminal(&std::io::stderr());
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(ansi)
        .with_writer(std::io::stderr);
    if format == "json" {
        builder.json().init();
    } else {
        builder.compact().init();
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(&cli.log_format);
    if let Some(d) = &cli.data_dir {
        cli::set_data_dir(d.clone());
    }
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: cannot start runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = rt.block_on(async move {
        match cli.command {
            Command::Serve { local, listen } => {
                cli::serve(cli.config, local, listen, cli.data_dir).await
            }
            Command::Init {
                force,
                non_interactive,
                listen,
                public_url,
                ssh_user,
                ssh_key,
                generate_key,
                admin,
                password_stdin,
            } => {
                cli::init(
                    cli.config,
                    cli::InitOptions {
                        force,
                        non_interactive,
                        listen,
                        data_dir: cli.data_dir,
                        public_url,
                        ssh_user,
                        ssh_key,
                        generate_key,
                        admin,
                        password_stdin,
                    },
                )
                .await
            }
            Command::User(c) => match c {
                UserCommand::Add {
                    username,
                    role,
                    password_stdin,
                } => cli::user_add(cli.config, username, role, password_stdin).await,
                UserCommand::Remove { username } => cli::user_remove(cli.config, username).await,
                UserCommand::ResetPassword {
                    username,
                    password_stdin,
                    reset_totp,
                } => cli::user_reset(cli.config, username, password_stdin, reset_totp).await,
                UserCommand::List => cli::user_list(cli.config).await,
            },
            Command::Hosts(HostsCommand::Test { names, trust }) => {
                cli::hosts_test(cli.config, names, trust).await
            }
            Command::Hosts(HostsCommand::Bootstrap {
                from,
                user,
                key,
                no_journal,
            }) => cli::hosts_bootstrap(cli.config, from, user, key, !no_journal),
            Command::Config(ConfigCommand::Check) => cli::config_check(cli.config),
            Command::Secret(c) => match c {
                SecretCommand::Set { name, stdin } => {
                    cli::secret_set(cli.config, name, stdin).await
                }
                SecretCommand::List => cli::secret_list(cli.config).await,
                SecretCommand::Delete { name } => cli::secret_delete(cli.config, name).await,
            },
            Command::Audit(AuditCommand::Verify) => cli::audit_verify(cli.config).await,
        }
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
