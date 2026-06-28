use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};

pub fn write_pidfile(path: &Path) -> Result<()> {
    std::fs::write(path, std::process::id().to_string())
        .with_context(|| format!("writing pidfile {}", path.display()))?;
    Ok(())
}

pub fn read_pid(path: &Path) -> Result<i32> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading pidfile {}", path.display()))?;
    text.trim().parse::<i32>().map_err(|_| anyhow!("invalid pid in {}", path.display()))
}

pub fn remove_pidfile(path: &Path) {
    let _ = std::fs::remove_file(path);
}

pub fn stop(path: &Path) -> Result<()> {
    let pid = read_pid(path)?;
    let rc = unsafe { libc::kill(pid, libc::SIGTERM) };
    if rc != 0 {
        return Err(anyhow!("failed to signal pid {pid} (is it running?)"));
    }
    Ok(())
}

pub fn install_signal_handler(flag: Arc<AtomicBool>) -> Result<()> {
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&flag))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&flag))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pidfile_roundtrip() {
        let dir = std::env::temp_dir().join(format!("m2t-pid-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("mail2tg.pid");
        write_pidfile(&p).unwrap();
        let pid = read_pid(&p).unwrap();
        assert_eq!(pid, std::process::id() as i32);
        remove_pidfile(&p);
        assert!(!p.exists());
    }

    #[test]
    fn stop_missing_pidfile_errors() {
        assert!(stop(std::path::Path::new("/no/such/mail2tg.pid")).is_err());
    }
}
