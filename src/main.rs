mod cli;
mod config;
mod control;
mod daemon;
mod email;
mod imap_client;
mod state;
mod store;
mod telegram;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mail2tg", version, about = "Forward selected emails to Telegram")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the daemon (poll mailboxes + Telegram listener)
    Run,
    /// Gracefully stop a running daemon
    Stop,
    /// Manage per-mailbox Telegram recipient IDs
    Tgid {
        #[command(subcommand)]
        action: TgidAction,
    },
    /// Manage mailboxes
    Mailbox {
        #[command(subcommand)]
        action: MailboxAction,
    },
}

#[derive(Subcommand)]
enum TgidAction {
    Add,
    Remove,
    List,
}

#[derive(Subcommand)]
enum MailboxAction {
    Add,
    Remove,
    List,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    if let Err(e) = real_main() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn real_main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        // Only `run` needs TG_BOT_TOKEN / SENDER_DOMAINS.
        Command::Run => daemon::run(config::from_env()?),
        // `stop` only reads the pidfile path.
        Command::Stop => control::stop(&config::pid_path_from_env()),
        // CLI config commands only need the store file paths.
        Command::Tgid { action } => {
            let (c, cr) = config::store_paths_from_env();
            match action {
                TgidAction::Add => cli::tgid_add(&c, &cr),
                TgidAction::Remove => cli::tgid_remove(&c, &cr),
                TgidAction::List => cli::tgid_list(&c, &cr),
            }
        }
        Command::Mailbox { action } => {
            let (c, cr) = config::store_paths_from_env();
            match action {
                MailboxAction::Add => cli::mailbox_add(&c, &cr),
                MailboxAction::Remove => cli::mailbox_remove(&c, &cr),
                MailboxAction::List => cli::mailbox_list(&c, &cr),
            }
        }
    }
}
