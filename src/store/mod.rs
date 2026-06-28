pub mod config_file;
pub mod credentials;

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use config_file::{ConfigFile, Mailbox};
use credentials::Credentials;

#[allow(dead_code)]
pub struct Store {
    pub config: ConfigFile,
    pub creds: Credentials,
    pub perms_ok: bool,
    config_path: PathBuf,
    creds_path: PathBuf,
}

#[allow(dead_code)]
pub struct Usable {
    pub mailbox: Mailbox,
    pub password: String,
}

#[allow(dead_code)]
impl Store {
    pub fn load(config_path: &Path, creds_path: &Path) -> Result<Store> {
        let config = ConfigFile::load(config_path)?;
        let (creds, perms_ok) = Credentials::load(creds_path)?;
        Ok(Store {
            config,
            creds,
            perms_ok,
            config_path: config_path.to_path_buf(),
            creds_path: creds_path.to_path_buf(),
        })
    }

    pub fn save(&self) -> Result<()> {
        self.config.save(&self.config_path)?;
        self.creds.save(&self.creds_path)?;
        Ok(())
    }

    pub fn mailbox(&self, name: &str) -> Option<&Mailbox> {
        self.config.mailboxes.iter().find(|m| m.name == name)
    }

    pub fn names(&self) -> Vec<String> {
        self.config.mailboxes.iter().map(|m| m.name.clone()).collect()
    }

    fn mailbox_mut(&mut self, name: &str) -> Option<&mut Mailbox> {
        self.config.mailboxes.iter_mut().find(|m| m.name == name)
    }

    pub fn add_tgid(&mut self, name: &str, id: i64) -> Result<bool> {
        let mb = self.mailbox_mut(name).ok_or_else(|| anyhow!("no mailbox '{name}'"))?;
        if mb.whitelist.contains(&id) {
            return Ok(false);
        }
        mb.whitelist.push(id);
        Ok(true)
    }

    pub fn remove_tgid(&mut self, name: &str, id: i64) -> Result<bool> {
        let mb = self.mailbox_mut(name).ok_or_else(|| anyhow!("no mailbox '{name}'"))?;
        let before = mb.whitelist.len();
        mb.whitelist.retain(|x| *x != id);
        Ok(mb.whitelist.len() != before)
    }

    pub fn add_mailbox(&mut self, mailbox: Mailbox, password: &str) -> Result<()> {
        if self.mailbox(&mailbox.name).is_some() {
            return Err(anyhow!("mailbox '{}' already exists", mailbox.name));
        }
        self.creds.set(&mailbox.name, password);
        self.config.mailboxes.push(mailbox);
        Ok(())
    }

    pub fn remove_mailbox(&mut self, name: &str) -> Result<()> {
        if self.mailbox(name).is_none() {
            return Err(anyhow!("no mailbox '{name}'"));
        }
        self.config.mailboxes.retain(|m| m.name != name);
        self.creds.remove(name);
        Ok(())
    }

    pub fn usable(&self) -> Vec<Usable> {
        self.config
            .mailboxes
            .iter()
            .filter(|m| !m.whitelist.is_empty())
            .filter_map(|m| {
                self.creds.get(&m.name).map(|pw| Usable {
                    mailbox: m.clone(),
                    password: pw.to_string(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::config_file::Mailbox;

    fn mb(name: &str, whitelist: Vec<i64>) -> Mailbox {
        Mailbox {
            name: name.into(), host: "h".into(), port: 993, user: "u".into(),
            folder: "INBOX".into(), targets: vec!["t@duck.com".into()], whitelist,
        }
    }

    fn empty_store() -> Store {
        Store {
            config: ConfigFile::default(),
            creds: Credentials::default(),
            perms_ok: true,
            config_path: "x".into(),
            creds_path: "y".into(),
        }
    }

    #[test]
    fn add_mailbox_rejects_duplicate() {
        let mut s = empty_store();
        s.add_mailbox(mb("a", vec![]), "pw").unwrap();
        assert!(s.add_mailbox(mb("a", vec![]), "pw2").is_err());
        assert_eq!(s.creds.get("a"), Some("pw"));
    }

    #[test]
    fn add_and_remove_tgid() {
        let mut s = empty_store();
        s.add_mailbox(mb("a", vec![]), "pw").unwrap();
        assert!(s.add_tgid("a", 5).unwrap());
        assert!(!s.add_tgid("a", 5).unwrap()); // duplicate no-op
        assert_eq!(s.mailbox("a").unwrap().whitelist, vec![5]);
        assert!(s.remove_tgid("a", 5).unwrap());
        assert!(!s.remove_tgid("a", 5).unwrap());
        assert!(s.add_tgid("nope", 1).is_err());
    }

    #[test]
    fn remove_mailbox_drops_secret() {
        let mut s = empty_store();
        s.add_mailbox(mb("a", vec![1]), "pw").unwrap();
        s.remove_mailbox("a").unwrap();
        assert!(s.mailbox("a").is_none());
        assert!(s.creds.get("a").is_none());
    }

    #[test]
    fn usable_requires_whitelist_and_creds() {
        let mut s = empty_store();
        s.add_mailbox(mb("haswl", vec![1]), "pw").unwrap();   // usable
        s.add_mailbox(mb("nowl", vec![]), "pw").unwrap();     // empty whitelist
        // mailbox present but no creds entry:
        s.config.mailboxes.push(mb("nocreds", vec![2]));
        let names: Vec<String> = s.usable().into_iter().map(|u| u.mailbox.name).collect();
        assert_eq!(names, vec!["haswl"]);
    }
}
