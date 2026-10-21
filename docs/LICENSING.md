# Subscriptions and licence keys (FR-B11)

How a school pays, termly or yearly, and how Termbook knows,
with no internet.

## For the school

1. Settings → Subscription shows the **school code**, e.g. `9F3AC-2B1E0`.
2. The school pays for a **term** (about 120 days) or a **year** (365) and
   sends the school code with the payment reference.
3. It receives a **licence key**, one long line starting `TBK1.`, and pastes
   it into Settings → Subscription → Install key.

A new school has a **30-day trial** from the day setup was completed.

When a subscription ends, Termbook keeps working fully for **7 days of grace**,
with a warning on every screen. After that it becomes **read-only**:
everything can still be viewed, printed and backed up, and texts already
queued still go out, but nothing can be added or changed until a new key is
entered. A school is never locked out of its own records.

## For the owner (you)

Everything is in the **owner** folder of the project. Double-click:

| File | When |
|---|---|
| `1 Set up my keys (once).bat` | Once, ever. Makes your licence key and your update key in `%USERPROFILE%\TermbookKeys`, and puts their public halves into the program (`src-tauri/src/licence.rs`, `src-tauri/updater.pub`, `tauri.conf.json`). Safe to run again: it never replaces a key. |
| `2 Make a licence key for a school.bat` | Every payment. Asks for the school code, name, term or year, start date and payment reference; the key is copied, ready to paste into WhatsApp or email, and recorded in `TermbookKeys\issued-keys.csv`. |

After step 1: commit and push the changed files, and build the installer
again. Copies built before that do not check licences.

**The keys folder is the business.** Anyone with `termbook-issuer.key` can
make keys for every school; anyone with `updater.key` can push an update to
every school. Copy the folder to a USB disk kept somewhere safe. Never put it
on a school computer, in Google Drive, WhatsApp or email. It is outside the
project folder, so it is never committed or zipped with the code. Lose it
without a copy and every school needs a new installation with new keys.

Behind the scripts is `tools/termbook-licence` (`keygen`, `issue`,
`inspect`, `pubkey`), which can still be run by hand.

There was only ever one tool. Before the rename it was called
`phantom-licence` and its keys started `PSM1.`; no such key was ever issued.

## What it protects, honestly

- A key **cannot be forged** without your signing key (Ed25519 signatures).
- A key **cannot be edited**: changing its date breaks the signature.
- A key **works only for the school it was issued to**: its code is inside.
- Turning the computer's **clock back** does not help: Termbook remembers the
  latest date it has seen and uses that.
- Read-only is enforced **inside SQLite** (an authorizer on the connection),
  so no screen or command can forget to check it.

Keep the source code private. The licence check lives in the program, so
anyone holding the code could build a copy without it; the installers are
published from a separate public repository so the code never has to be.

What it is not: protection against someone who patches the program itself,
or who restores an old database *and* turns the clock back. It keeps honest
schools honest and makes a lapsed subscription obvious, which is what
manual activation needs (SRS 16.4). Online payment and automatic keys come
with the website (FR-W4, FR-W6); the tool's `issue` is what the website will
call.
