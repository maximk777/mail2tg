use std::net::TcpStream;
use std::time::Duration;

use anyhow::{anyhow, Result};
use rustls_connector::RustlsConnector;

use crate::store::config_file::Mailbox;

pub struct RawMessage {
    pub uid: u32,
    pub body: Vec<u8>,
}

pub trait MailSource {
    fn fetch_new(&mut self, last_uid: u32) -> Result<Vec<RawMessage>>;
}

pub struct ImapMailbox<'a> {
    mailbox: &'a Mailbox,
    password: &'a str,
}

impl<'a> ImapMailbox<'a> {
    pub fn new(mailbox: &'a Mailbox, password: &'a str) -> Self {
        ImapMailbox { mailbox, password }
    }
}

impl<'a> MailSource for ImapMailbox<'a> {
    fn fetch_new(&mut self, last_uid: u32) -> Result<Vec<RawMessage>> {
        let addr = (self.mailbox.host.as_str(), self.mailbox.port);
        let tcp = TcpStream::connect(addr)?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(30)))?;

        let connector = RustlsConnector::new_with_native_certs()?;
        let tls = connector.connect(&self.mailbox.host, tcp)?;

        let client = imap::Client::new(tls);
        let mut session = client
            .login(&self.mailbox.user, self.password)
            .map_err(|(e, _)| anyhow!("imap login failed: {e}"))?;

        // Run the work in a closure so logout is attempted on every path,
        // including any early `?` error after a successful login.
        let result = (|| -> Result<Vec<RawMessage>> {
            session.select(&self.mailbox.folder)?;

            // Guard against wrapping: if last_uid is u32::MAX there are no higher UIDs.
            let next_uid = match last_uid.checked_add(1) {
                Some(n) => n,
                None => return Ok(Vec::new()),
            };
            let query = format!("UID {next_uid}:*");
            let uids = session.uid_search(&query)?;
            let mut fresh: Vec<u32> = uids.into_iter().filter(|u| *u > last_uid).collect();
            fresh.sort_unstable();

            let mut out = Vec::with_capacity(fresh.len());
            for uid in fresh {
                let fetches = session.uid_fetch(uid.to_string(), "BODY[]")?;
                match fetches.iter().next().and_then(|f| f.body()) {
                    Some(body) => out.push(RawMessage {
                        uid,
                        body: body.to_vec(),
                    }),
                    None => {
                        // Don't silently drop: emit the UID with an empty body so the
                        // daemon advances past it (parse of an empty body skips it)
                        // rather than blocking the whole mailbox on one bad message.
                        log::warn!("no body for UID {uid} in '{}'; skipping", self.mailbox.name);
                        out.push(RawMessage {
                            uid,
                            body: Vec::new(),
                        });
                    }
                }
            }
            Ok(out)
        })();

        let _ = session.logout();
        result
    }
}
