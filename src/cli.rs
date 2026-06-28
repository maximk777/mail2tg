use anyhow::{anyhow, Result};
use dialoguer::{Confirm, Input, Password, Select};

use crate::config::Settings;
use crate::store::config_file::Mailbox;
use crate::store::Store;

fn load(settings: &Settings) -> Result<Store> {
    Store::load(&settings.config_path, &settings.credentials_path)
}

fn pick_mailbox(store: &Store) -> Result<Option<String>> {
    let names = store.names();
    if names.is_empty() {
        println!("Net nastroennykh yashchikov. Snachala: mail2tg mailbox add");
        return Ok(None);
    }
    let idx = Select::new()
        .with_prompt("Vyberte yashchik")
        .items(&names)
        .default(0)
        .interact()?;
    Ok(Some(names[idx].clone()))
}

pub fn tgid_list(settings: &Settings) -> Result<()> {
    let store = load(settings)?;
    if let Some(name) = pick_mailbox(&store)? {
        let mb = store.mailbox(&name).unwrap();
        println!("Access list for '{name}':");
        if mb.whitelist.is_empty() {
            println!("  (empty)");
        }
        for id in &mb.whitelist {
            println!("  - {id}");
        }
    }
    Ok(())
}

pub fn tgid_add(settings: &Settings) -> Result<()> {
    let mut store = load(settings)?;
    let name = match pick_mailbox(&store)? {
        Some(n) => n,
        None => return Ok(()),
    };
    {
        let mb = store.mailbox(&name).unwrap();
        println!("Current access list for '{name}':");
        if mb.whitelist.is_empty() {
            println!("  (empty)");
        }
        for id in &mb.whitelist {
            println!("  - {id}");
        }
    }
    let id: i64 = Input::<i64>::new()
        .with_prompt("Enter tg id")
        .interact_text()?;
    if store.add_tgid(&name, id)? {
        store.save()?;
        println!("Added {id} to '{name}'.");
    } else {
        println!("{id} is already in the list.");
    }
    Ok(())
}

pub fn tgid_remove(settings: &Settings) -> Result<()> {
    let mut store = load(settings)?;
    let name = match pick_mailbox(&store)? {
        Some(n) => n,
        None => return Ok(()),
    };
    let ids = store.mailbox(&name).unwrap().whitelist.clone();
    if ids.is_empty() {
        println!("Access list is empty.");
        return Ok(());
    }
    let labels: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
    let idx = Select::new()
        .with_prompt("Select tg id to remove")
        .items(&labels)
        .default(0)
        .interact()?;
    store.remove_tgid(&name, ids[idx])?;
    store.save()?;
    println!("Removed {} from '{name}'.", ids[idx]);
    Ok(())
}

pub fn mailbox_list(settings: &Settings) -> Result<()> {
    let store = load(settings)?;
    if store.config.mailboxes.is_empty() {
        println!("No mailboxes configured.");
        return Ok(());
    }
    for m in &store.config.mailboxes {
        println!(
            "- {}  host={} port={} user={} folder={} targets={:?} tgids={} password=****",
            m.name, m.host, m.port, m.user, m.folder, m.targets, m.whitelist.len()
        );
    }
    Ok(())
}

pub fn mailbox_add(settings: &Settings) -> Result<()> {
    let mut store = load(settings)?;
    let name: String = Input::<String>::new()
        .with_prompt("Name")
        .interact_text()?;
    if store.mailbox(&name).is_some() {
        return Err(anyhow!("mailbox '{name}' already exists"));
    }
    let host: String = Input::<String>::new()
        .with_prompt("IMAP host")
        .interact_text()?;
    let port: u16 = Input::<u16>::new()
        .with_prompt("IMAP port")
        .default(993)
        .interact_text()?;
    let user: String = Input::<String>::new()
        .with_prompt("IMAP user")
        .interact_text()?;
    let password = Password::new()
        .with_prompt("IMAP password")
        .interact()?;
    let folder: String = Input::<String>::new()
        .with_prompt("Folder")
        .default("INBOX".into())
        .interact_text()?;

    println!("Target addresses (one per line, empty line to finish):");
    let mut targets: Vec<String> = Vec::new();
    loop {
        let line: String = Input::<String>::new()
            .with_prompt(">")
            .allow_empty(true)
            .interact_text()?;
        if line.trim().is_empty() {
            break;
        }
        targets.push(line.trim().to_string());
    }

    let mailbox = Mailbox {
        name: name.clone(),
        host,
        port,
        user,
        folder,
        targets,
        whitelist: vec![],
    };
    store.add_mailbox(mailbox, &password)?;
    store.save()?;
    println!("Mailbox '{name}' saved. Add recipients: mail2tg tgid add");
    Ok(())
}

pub fn mailbox_remove(settings: &Settings) -> Result<()> {
    let mut store = load(settings)?;
    let name = match pick_mailbox(&store)? {
        Some(n) => n,
        None => return Ok(()),
    };
    if Confirm::new()
        .with_prompt(format!("Delete mailbox '{name}'?"))
        .default(false)
        .interact()?
    {
        store.remove_mailbox(&name)?;
        store.save()?;
        println!("Mailbox '{name}' deleted.");
    }
    Ok(())
}
