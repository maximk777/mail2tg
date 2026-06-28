use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Default)]
pub struct Credentials {
    map: BTreeMap<String, String>,
}

impl Credentials {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(|s| s.as_str())
    }

    pub fn set(&mut self, name: &str, password: &str) {
        self.map.insert(name.to_string(), password.to_string());
    }

    pub fn remove(&mut self, name: &str) {
        self.map.remove(name);
    }

    pub fn load(path: &Path) -> Result<(Credentials, bool)> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let map: BTreeMap<String, String> = serde_json::from_slice(&bytes)
                    .with_context(|| format!("parsing {}", path.display()))?;
                let mode = std::fs::metadata(path)?.permissions().mode() & 0o077;
                Ok((Credentials { map }, mode == 0))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok((Credentials::default(), true))
            }
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(".mail2tg.credentials.tmp");
        let body = serde_json::to_string_pretty(&self.map)?;
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            f.write_all(body.as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn roundtrip_and_perms_0600() {
        let dir = std::env::temp_dir().join(format!("m2t-cr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("mail2tg.credentials");
        let mut c = Credentials::default();
        c.set("gmail-duck", "app-pass-1");
        c.save(&p).unwrap();
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let (back, perms_ok) = Credentials::load(&p).unwrap();
        assert_eq!(back.get("gmail-duck"), Some("app-pass-1"));
        assert!(perms_ok);
    }

    #[test]
    fn loose_perms_flagged() {
        let dir = std::env::temp_dir().join(format!("m2t-cr2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("mail2tg.credentials");
        Credentials::default().save(&p).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        let (_c, perms_ok) = Credentials::load(&p).unwrap();
        assert!(!perms_ok);
    }

    #[test]
    fn missing_is_empty_ok() {
        let (c, perms_ok) =
            Credentials::load(std::path::Path::new("/no/such.credentials")).unwrap();
        assert!(c.get("x").is_none());
        assert!(perms_ok);
    }
}
