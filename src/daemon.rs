use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use regex::Regex;

use crate::config::Settings;
use crate::control;
use crate::email::card::build_card;
use crate::email::recipient::recipient_matches;
use crate::email::sender::{domain_matches, parse_sender};
use crate::email::parse_email;
use crate::imap_client::{ImapMailbox, MailSource, RawMessage};
use crate::state::{self, MailboxState};
use crate::store::config_file::Mailbox;
use crate::store::Store;
use crate::telegram::{start_reply_for_text, TelegramApi, TgClient};

pub enum Outcome {
    Skipped,
    Forwarded,
    DeliveryFailed,
}

pub fn process_message(
    msg: &RawMessage,
    mailbox: &Mailbox,
    settings: &Settings,
    ddg: &Regex,
    tg: &dyn TelegramApi,
    state: &mut MailboxState,
) -> Outcome {
    let parsed = match parse_email(&msg.body) {
        Some(p) => p,
        None => {
            state.last_uid = state.last_uid.max(msg.uid);
            return Outcome::Skipped;
        }
    };

    if parsed.timestamp != 0 && parsed.timestamp < state.baseline_ts {
        state.last_uid = state.last_uid.max(msg.uid);
        return Outcome::Skipped;
    }

    if !recipient_matches(&mailbox.targets, &parsed.recipients) {
        state.last_uid = state.last_uid.max(msg.uid);
        return Outcome::Skipped;
    }

    let sender = match parse_sender(&parsed.from_addr, ddg) {
        Some(s) if domain_matches(&s.domain, &settings.sender_domains) => s,
        _ => {
            state.last_uid = state.last_uid.max(msg.uid);
            return Outcome::Skipped;
        }
    };

    let card = build_card(
        &sender.address,
        &parsed.subject,
        &parsed.date_display,
        &parsed.body,
        settings.body_preview_chars,
    );

    let mut all_ok = true;
    for &id in &mailbox.whitelist {
        if let Err(e) = tg.send_message(id, &card) {
            log::error!("send to {id} failed for '{}': {e}", mailbox.name);
            all_ok = false;
        }
    }

    if all_ok {
        state.last_uid = state.last_uid.max(msg.uid);
        Outcome::Forwarded
    } else {
        Outcome::DeliveryFailed
    }
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn poll_mailbox(
    mailbox: &Mailbox,
    password: &str,
    settings: &Settings,
    ddg: &Regex,
    tg: &dyn TelegramApi,
) -> Result<()> {
    let mut st = state::load(&settings.state_dir, &mailbox.name, now_ts())?;
    let mut src = ImapMailbox::new(mailbox, password);
    let messages = src.fetch_new(st.last_uid)?;
    for msg in &messages {
        let before = st.last_uid;
        let outcome = process_message(msg, mailbox, settings, ddg, tg, &mut st);
        if st.last_uid != before {
            state::save(&settings.state_dir, &mailbox.name, &st)?;
        }
        if matches!(outcome, Outcome::DeliveryFailed) {
            break; // do not advance past an undelivered message; retry next cycle
        }
    }
    Ok(())
}

fn spawn_telegram_thread(
    settings: Arc<Settings>,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let tg = TgClient::new(&settings.tg_bot_token);
        let mut offset = 0i64;
        while !stop.load(Ordering::Relaxed) {
            match tg.get_updates(offset, 5) {
                Ok(updates) => {
                    for u in updates {
                        offset = offset.max(u.update_id + 1);
                        if let (Some(chat_id), Some(text)) = (u.chat_id, u.text.as_deref()) {
                            let in_wl = match Store::load(
                                &settings.config_path,
                                &settings.credentials_path,
                            ) {
                                Ok(s) => s
                                    .config
                                    .mailboxes
                                    .iter()
                                    .any(|m| m.whitelist.contains(&chat_id)),
                                Err(_) => false,
                            };
                            if let Some(reply) = start_reply_for_text(
                                text,
                                in_wl,
                                chat_id,
                                &settings.sender_domains,
                            ) {
                                if let Err(e) = tg.send_message(chat_id, &reply) {
                                    log::error!("reply to {chat_id} failed: {e}");
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("getUpdates failed: {e}");
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        }
    })
}

pub fn run(settings: Settings) -> Result<()> {
    let settings = Arc::new(settings);
    let stop = Arc::new(AtomicBool::new(false));
    control::install_signal_handler(Arc::clone(&stop))?;
    control::write_pidfile(&settings.pid_path)?;
    log::info!("mail2tg started");

    let ddg = Regex::new(&settings.ddg_regex)?;
    let tg = TgClient::new(&settings.tg_bot_token);

    let tg_handle = spawn_telegram_thread(Arc::clone(&settings), Arc::clone(&stop));

    let mut idle_logged = false;
    let mut perms_warned = false;
    while !stop.load(Ordering::Relaxed) {
        match Store::load(&settings.config_path, &settings.credentials_path) {
            Ok(store) => {
                if !store.perms_ok {
                    if !perms_warned {
                        log::warn!(
                            "{} is group/world-readable; fix to 0600",
                            settings.credentials_path.display()
                        );
                        perms_warned = true;
                    }
                } else {
                    perms_warned = false;
                }
                let usable = store.usable();
                if usable.is_empty() {
                    if !idle_logged {
                        log::warn!("no usable mailboxes; idling, waiting for config");
                        idle_logged = true;
                    }
                } else {
                    idle_logged = false;
                    for u in usable {
                        if let Err(e) =
                            poll_mailbox(&u.mailbox, &u.password, &settings, &ddg, &tg)
                        {
                            log::error!("mailbox '{}' poll failed: {e}", u.mailbox.name);
                        }
                    }
                }
            }
            Err(e) => log::error!("config reload failed: {e}"),
        }

        let mut slept = Duration::ZERO;
        while slept < settings.poll_interval && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(250));
            slept += Duration::from_millis(250);
        }
    }

    log::info!("mail2tg stopping");
    control::remove_pidfile(&settings.pid_path);
    if tg_handle.join().is_err() {
        log::error!("telegram thread panicked");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imap_client::RawMessage;
    use crate::store::config_file::Mailbox;
    use crate::telegram::{TelegramApi, Update};
    use std::sync::Mutex;

    struct MockTg {
        fail: bool,
        sent: Mutex<Vec<(i64, String)>>,
    }
    impl TelegramApi for MockTg {
        fn send_message(&self, chat_id: i64, html: &str) -> anyhow::Result<()> {
            if self.fail {
                return Err(anyhow::anyhow!("boom"));
            }
            self.sent.lock().unwrap().push((chat_id, html.to_string()));
            Ok(())
        }
        fn get_updates(&self, _o: i64, _t: u32) -> anyhow::Result<Vec<Update>> {
            Ok(vec![])
        }
    }

    fn settings() -> Settings {
        crate::config::from_lookup(|k| match k {
            "TG_BOT_TOKEN" => Some("t".into()),
            "SENDER_DOMAINS" => Some("openai.com".into()),
            _ => None,
        })
        .unwrap()
    }

    fn mailbox() -> Mailbox {
        Mailbox {
            name: "b".into(),
            host: "h".into(),
            port: 993,
            user: "u".into(),
            folder: "INBOX".into(),
            targets: vec!["scpccomz@duck.com".into()],
            whitelist: vec![111, 222],
        }
    }

    fn raw(uid: u32) -> RawMessage {
        RawMessage {
            uid,
            body: b"From: noreply_at_openai.com_x@duck.com\r\nTo: scpccomz@duck.com\r\nSubject: s\r\nDate: Sun, 28 Jun 2026 14:30:00 +0000\r\n\r\nbody\r\n".to_vec(),
        }
    }

    fn re() -> regex::Regex {
        regex::Regex::new(crate::config::DEFAULT_DDG).unwrap()
    }

    #[test]
    fn forwards_to_all_whitelist() {
        let tg = MockTg { fail: false, sent: Mutex::new(vec![]) };
        let mut st = crate::state::MailboxState { last_uid: 0, baseline_ts: 0 };
        let out = process_message(&raw(5), &mailbox(), &settings(), &re(), &tg, &mut st);
        assert!(matches!(out, Outcome::Forwarded));
        assert_eq!(tg.sent.lock().unwrap().len(), 2);
        assert_eq!(st.last_uid, 5);
    }

    #[test]
    fn delivery_failure_keeps_uid() {
        let tg = MockTg { fail: true, sent: Mutex::new(vec![]) };
        let mut st = crate::state::MailboxState { last_uid: 0, baseline_ts: 0 };
        let out = process_message(&raw(5), &mailbox(), &settings(), &re(), &tg, &mut st);
        assert!(matches!(out, Outcome::DeliveryFailed));
        assert_eq!(st.last_uid, 0);
    }

    #[test]
    fn wrong_recipient_skipped_but_advances() {
        let tg = MockTg { fail: false, sent: Mutex::new(vec![]) };
        let mut mb = mailbox();
        mb.targets = vec!["someone-else@duck.com".into()];
        let mut st = crate::state::MailboxState { last_uid: 0, baseline_ts: 0 };
        let out = process_message(&raw(9), &mb, &settings(), &re(), &tg, &mut st);
        assert!(matches!(out, Outcome::Skipped));
        assert_eq!(tg.sent.lock().unwrap().len(), 0);
        assert_eq!(st.last_uid, 9);
    }

    #[test]
    fn older_than_baseline_skipped() {
        let tg = MockTg { fail: false, sent: Mutex::new(vec![]) };
        let mut st = crate::state::MailboxState { last_uid: 0, baseline_ts: i64::MAX };
        let out = process_message(&raw(3), &mailbox(), &settings(), &re(), &tg, &mut st);
        assert!(matches!(out, Outcome::Skipped));
        assert_eq!(st.last_uid, 3);
    }

    #[test]
    fn empty_body_skipped_and_advances() {
        let tg = MockTg { fail: false, sent: Mutex::new(vec![]) };
        let mut st = crate::state::MailboxState { last_uid: 0, baseline_ts: 0 };
        let empty = RawMessage { uid: 8, body: Vec::new() };
        let out = process_message(&empty, &mailbox(), &settings(), &re(), &tg, &mut st);
        assert!(matches!(out, Outcome::Skipped));
        assert_eq!(tg.sent.lock().unwrap().len(), 0);
        assert_eq!(st.last_uid, 8); // advanced, no panic
    }

    /// Prove that a batch [uid5→DeliveryFailed, uid6→would succeed] leaves last_uid
    /// at its pre-batch value and never attempts uid6.
    #[test]
    fn batch_stops_at_first_delivery_failure() {
        // Fails on the first two send calls (uid5's two whitelist entries), then
        // would succeed — but we must never reach uid6.
        struct TrackingTg {
            call_count: Mutex<u32>,
            sent: Mutex<Vec<(i64, String)>>,
        }
        impl TelegramApi for TrackingTg {
            fn send_message(&self, chat_id: i64, html: &str) -> anyhow::Result<()> {
                let mut n = self.call_count.lock().unwrap();
                *n += 1;
                if *n <= 2 {
                    // uid5 has a 2-entry whitelist; fail both
                    return Err(anyhow::anyhow!("boom"));
                }
                drop(n);
                self.sent.lock().unwrap().push((chat_id, html.to_string()));
                Ok(())
            }
            fn get_updates(&self, _o: i64, _t: u32) -> anyhow::Result<Vec<Update>> {
                Ok(vec![])
            }
        }

        let tg = TrackingTg { call_count: Mutex::new(0), sent: Mutex::new(vec![]) };
        let mut st = crate::state::MailboxState { last_uid: 4, baseline_ts: 0 };
        let messages = vec![raw(5), raw(6)];

        // Replicate the poll_mailbox inner loop (state::save skipped — no real dir needed).
        for msg in &messages {
            let before = st.last_uid;
            let outcome = process_message(msg, &mailbox(), &settings(), &re(), &tg, &mut st);
            let _ = before;
            if matches!(outcome, Outcome::DeliveryFailed) {
                break;
            }
        }

        // uid5 triggered 2 send calls (both failed) → DeliveryFailed → break.
        // uid6 must never have been attempted.
        assert_eq!(st.last_uid, 4, "last_uid must not advance past a delivery failure");
        assert_eq!(
            *tg.call_count.lock().unwrap(),
            2,
            "uid 6 must not be attempted; only uid5's 2 whitelist sends should occur"
        );
        assert!(tg.sent.lock().unwrap().is_empty(), "no successful sends expected");
    }
}
