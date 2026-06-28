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
    let settings = config::from_env()?;
    match cli.command {
        Command::Run => daemon::run(settings),
        Command::Stop => control::stop(&settings.pid_path),
        Command::Tgid { action } => match action {
            TgidAction::Add => cli::tgid_add(&settings),
            TgidAction::Remove => cli::tgid_remove(&settings),
            TgidAction::List => cli::tgid_list(&settings),
        },
        Command::Mailbox { action } => match action {
            MailboxAction::Add => cli::mailbox_add(&settings),
            MailboxAction::Remove => cli::mailbox_remove(&settings),
            MailboxAction::List => cli::mailbox_list(&settings),
        },
    }
}
