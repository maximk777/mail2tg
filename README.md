# mail2tg

Telegram bot that forwards selected emails into Telegram. It polls one or more
IMAP mailboxes, keeps only mail addressed to configured target addresses (e.g.
DuckDuckGo `@duck.com` aliases) and sent from whitelisted sender domains, and
pushes a compact card to each mailbox's Telegram whitelist.

Single static binary (musl / ARM), configured via env + an interactive CLI
(`mail2tg run`, `mail2tg mailbox add`, `mail2tg tgid add`, `mail2tg stop`).
