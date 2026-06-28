use std::io::Write;
use std::path::{Path, PathBuf};

pub struct MailboxState {
    pub last_uid: u32,
    pub baseline_ts: i64,
}

pub fn path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.txt"))
}

pub fn serialize(state: &MailboxState) -> String {
    format!("last_uid={}\nbaseline_ts={}\n", state.last_uid, state.baseline_ts)
}

pub fn parse(text: &str) -> MailboxState {
    let mut last_uid = 0u32;
    let mut baseline_ts = 0i64;
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "last_uid" => last_uid = v.trim().parse().unwrap_or(0),
                "baseline_ts" => baseline_ts = v.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    MailboxState { last_uid, baseline_ts }
}

pub fn save(dir: &Path, name: &str, state: &MailboxState) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let final_path = path(dir, name);
    let tmp = dir.join(format!(".{name}.txt.tmp"));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(serialize(state).as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &final_path)
}

pub fn load(dir: &Path, name: &str, now_ts: i64) -> std::io::Result<MailboxState> {
    let p = path(dir, name);
    match std::fs::read_to_string(&p) {
        Ok(text) => Ok(parse(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let st = MailboxState { last_uid: 0, baseline_ts: now_ts };
            save(dir, name, &st)?;
            Ok(st)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let s = MailboxState { last_uid: 42, baseline_ts: 1700000000 };
        let parsed = parse(&serialize(&s));
        assert_eq!(parsed.last_uid, 42);
        assert_eq!(parsed.baseline_ts, 1700000000);
    }

    #[test]
    fn parse_corrupt_defaults_to_zero() {
        let p = parse("garbage\nlast_uid=oops\n");
        assert_eq!(p.last_uid, 0);
        assert_eq!(p.baseline_ts, 0);
    }

    #[test]
    fn load_missing_seeds_baseline_and_persists() {
        let dir = std::env::temp_dir().join(format!("mail2tg-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(path(&dir, "box1"));
        let st = load(&dir, "box1", 1234).unwrap();
        assert_eq!(st.last_uid, 0);
        assert_eq!(st.baseline_ts, 1234);
        // second load reads the persisted baseline, ignoring the new now_ts
        let again = load(&dir, "box1", 9999).unwrap();
        assert_eq!(again.baseline_ts, 1234);
    }

    #[test]
    fn save_then_load_advances_uid() {
        let dir = std::env::temp_dir().join(format!("mail2tg-state2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        save(&dir, "b", &MailboxState { last_uid: 7, baseline_ts: 5 }).unwrap();
        let st = load(&dir, "b", 0).unwrap();
        assert_eq!(st.last_uid, 7);
        assert_eq!(st.baseline_ts, 5);
    }
}
