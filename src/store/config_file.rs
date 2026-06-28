use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

fn default_port() -> u16 {
    993
}
fn default_folder() -> String {
    "INBOX".to_string()
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Mailbox {
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub user: String,
    #[serde(default = "default_folder")]
    pub folder: String,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub whitelist: Vec<i64>,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Default, Debug)]
pub struct ConfigFile {
    #[serde(default)]
    pub mailboxes: Vec<Mailbox>,
}

impl ConfigFile {
    #[allow(dead_code)]
    pub fn load(path: &Path) -> Result<ConfigFile> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("parsing {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConfigFile::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    #[allow(dead_code)]
    pub fn save(&self, path: &Path) -> Result<()> {
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(".mail2tg.json.tmp");
        let body = serde_json::to_string_pretty(self)?;
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(body.as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Mailbox {
        Mailbox {
            name: "gmail-duck".into(),
            host: "imap.gmail.com".into(),
            port: 993,
            user: "me@gmail.com".into(),
            folder: "INBOX".into(),
            targets: vec!["scpccomz@duck.com".into()],
            whitelist: vec![123, 456],
        }
    }

    #[test]
    fn json_roundtrip() {
        let cf = ConfigFile {
            mailboxes: vec![sample()],
        };
        let dir = std::env::temp_dir().join(format!("m2t-cf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("mail2tg.json");
        cf.save(&p).unwrap();
        let back = ConfigFile::load(&p).unwrap();
        assert_eq!(back.mailboxes, cf.mailboxes);
    }

    #[test]
    fn defaults_for_optional_fields() {
        let json = r#"{"mailboxes":[{"name":"x","host":"h","user":"u","targets":["t@duck.com"]}]}"#;
        let cf: ConfigFile = serde_json::from_str(json).unwrap();
        assert_eq!(cf.mailboxes[0].port, 993);
        assert_eq!(cf.mailboxes[0].folder, "INBOX");
        assert!(cf.mailboxes[0].whitelist.is_empty());
    }

    #[test]
    fn missing_file_is_empty() {
        let cf = ConfigFile::load(std::path::Path::new("/no/such/mail2tg.json")).unwrap();
        assert!(cf.mailboxes.is_empty());
    }
}
