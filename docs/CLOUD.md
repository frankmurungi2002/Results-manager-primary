# Phantom Cloud: design

Four things the owner has asked for need a server Phantom does not have yet:
**its own bulk SMS**, **cloud backup**, **inviting teachers by email**, and
**schools paying online**. This document decides how that server works before
any of it is built, because the decisions are hard to undo once schools
depend on them.

Status: design only. Nothing here is built, except where marked.

---

## 1. One database per school, on the school's computer

*"Is it the same database for all the schools?"* No, and it should never be.

Each school keeps its own SQLite database on its own office computer, as it
does today. The cloud never holds a school's records in a form it can read.

| | One shared database in the cloud | One database per school (this design) |
|---|---|---|
| Works with no internet | No: every screen waits on the network | Yes: the network is only for SMS, backup, payment |
| A breach exposes | Every learner in every school | One school, and only if its computer is taken |
| One school's bug or bad import affects | Everyone | That school |
| Data protection law | You hold every child's record: the heaviest duties | Schools hold their own; you hold contact and billing details |
| 800 learners, power cut | Lost work since the last sync | Nothing: it is on the disk |

Uganda's Data Protection and Privacy Act (2019) applies to learners' records,
and children's data especially. Holding none of it centrally is the strongest
position a school software business can take. FR-O4 already says so; this
design keeps it true.

What the cloud **does** hold:

- **Schools**: code, name, contact person, plan, keys issued.
- **SMS**: each school's credit balance, and the numbers and texts in flight
  (the gateway needs them to send). Kept for delivery reports, then deleted.
- **Backups**: encrypted on the school's computer before upload; the cloud
  stores bytes it cannot read.
- **Invites**: only the email address a message goes to, until it is sent.

## 2. How a school's computer proves who it is

The licence key already proves which school a computer belongs to (it is
signed and names the school code). The cloud reuses it:

1. The first time Phantom needs the cloud, it makes a **device key pair**
   (Ed25519) and sends the public half with its licence key.
2. The cloud checks the licence signature and records "school `9F3AC2B1E0`,
   device key `…`".
3. Every later request is signed with the device key. No passwords, no
   tokens to steal from the webview (the private key lives with the Rust
   backend, like the session does).

A stolen licence key is not enough to impersonate a school: the device key is
also needed, and a new device registration shows up on the owner's list.

## 3. The service

A small HTTPS API, a database for the tables above, and object storage for
backups. Suggested: Rust (axum) to share code with the app, PostgreSQL, and
S3-compatible storage with no egress fees (Cloudflare R2) so restores do not
cost per download. Start on one small server; it handles thousands of schools.

## 4. Your own bulk SMS: reseller with credits

Chosen 2026-10-08. Phones only accept texts from licensed senders, so "our
own SMS platform" means: **Phantom holds one wholesale account with a
Ugandan aggregator, and schools buy SMS credits from Phantom.** Schools never
see the aggregator. This is what most local bulk SMS platforms are.

```
School PC ──signed──▶ Phantom Cloud ──▶ Aggregator ──▶ MTN / Airtel ──▶ parent
   (outbox)            (balance, -1 per SMS)       (delivery report back ▲)
```

- In the app this is one more provider in `sms.rs`, "Phantom SMS", beside
  Africa's Talking and EgoSMS. The outbox, priorities, expiry, the
  one-sender gate and the cost preview all stay exactly as built.
- The cloud checks the balance **before** sending and refuses with "not
  enough credit" (a Rejected, so it is not retried). The broadcast preview
  shows the balance beside the cost.
- Credits are bought with Mobile Money (section 7). Price per SMS is yours to
  set; your margin is the difference from the wholesale rate you negotiate.
- **Sender IDs**: texts should arrive from "RAINBOW NP", not a number. Each
  sender ID is registered with the networks through the aggregator, which
  takes time per school; schools without one use Phantom's.

To decide before building: which aggregator (price per SMS, sender-ID
turnaround, delivery reports, uptime), and the price to schools. Check UCC's
current rules for bulk and school messaging, and registration with the
Personal Data Protection Office, before launch. Both change; confirm with
someone who knows the current requirements.

The GSM-modem option (texts from a SIM at the school, no internet) stays
worth adding later, for urgent pass-out texts on days the internet is down.
It needs a serial-port permission, which is a security decision to take
deliberately, not something to slip in while building a feature.

## 5. Cloud backup, encrypted at the school

Backups to a second drive (built) protect against a dead disk. A fire, a
flood or a stolen computer takes the second drive too. Cloud backup is for
that.

- **Encrypted on the school's computer, before upload.** The School Admin
  sets a backup passphrase; a key is derived from it (Argon2id, as passwords
  are) and the file is encrypted with an authenticated cipher. Phantom Cloud
  stores ciphertext and cannot read a single learner's name.
- **The catch, stated plainly to the school:** lose the passphrase and the
  cloud copy is unreadable, by anyone, including Phantom. So setup prints a
  **recovery sheet** (the passphrase's recovery code) for the school safe.
  Paper again, because paper survives what kills computers.
- **Small and resumable.** Compressed before encryption; uploaded only when
  something changed; in resumable parts, because a connection that drops at
  80% must not start again from zero.
- **Kept**: one a day for 30 days, then one a month for 12.
- **Restore** on a new computer: install, sign in with the licence key, enter
  the passphrase, choose a date. This is FR-E4 scenario 2.

Photos are most of a school database's size. If upload cost is a problem on
mobile data, photos can go in a separate, less frequent backup.

## 6. Inviting teachers

*Worth saying first:* teachers sign in at the office computer, not at home,
so an invitation is not a link to a website. What an invitation is good for
is that **the School Admin never knows a teacher's password**, and the
teacher has a way to recover it later.

The design keeps the invitation on the school's computer and uses the cloud
only as a postman:

1. The office adds a teacher with their email and phone.
2. Phantom makes a one-time **invite code** (eight characters, valid 72
   hours) and stores only its hash, as it does passwords.
3. The code is delivered by **email** (through Phantom Cloud's mail relay),
   or **SMS** (through the outbox), or a **printed slip**: whichever the
   school has. No internet, no problem: the slip works.
4. The teacher, at the office computer, chooses "I have an invite code",
   types it, and sets their own password.

Because the code is checked on the school's computer, the cloud never sees a
password or the code's use. Steps 2, 3 (SMS and slip) and 4 need no cloud at
all and can be built now; email follows when the cloud exists. The same
mechanism gives "forgot my password" a safe answer later.

## 7. Paying online, and keys issued automatically

Today (SRS 16.4): the school pays by Mobile Money to the business number, you
run `phantom-licence issue`, you send the key on WhatsApp. That is right for
the first cohort.

Later: the school pays inside Phantom (MTN MoMo or Airtel Money, directly or
through a Ugandan payment aggregator), the payment confirmation reaches the
cloud, and the cloud issues the key and the app installs it, with no person in
between.

The risk to design out: issuing keys automatically means a signing key on an
internet-facing server, and a server can be broken into. So the server never
holds **your** key. It holds a separate **online key** that your key signs a
one-year certificate for; the app accepts keys from the online key only while
that certificate is valid. If the server is ever compromised, you stop
renewing the certificate and issue a new one, and the master key never left
your desk.

## 8. Order to build

1. **Invites without the cloud**: codes, SMS and printed slip. *No server
   needed; can be built next.*
2. **The cloud's skeleton**: device registration (section 2), school list for
   the owner.
3. **Phantom SMS with credits** (section 4), once an aggregator is chosen.
4. **Encrypted cloud backup** (section 5).
5. **Email** for invites (section 6), through the same cloud.
6. **Online payment and automatic keys** (section 7), once enough schools
   pay that manual issuing costs real time.

## Decisions needed from the owner

1. **SMS aggregator** and the **price per SMS** to schools.
2. **Hosting**: where the cloud runs. A provider with a data centre in Africa
   keeps it close; check the Data Protection Act's rules on storing personal
   data outside Uganda before choosing one abroad.
3. **Backup price**: included in the subscription, or an add-on.
4. **When to switch on subscriptions** (compile your public key in; see
   LICENSING.md).
