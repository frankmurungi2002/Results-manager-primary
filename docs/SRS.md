RESULTS MANAGER
Consolidated Software Requirements Specification
Nursery + Primary Edition — Offline-First School Management System for Uganda
Consolidates the original founder's brief and Functional Requirements versions 1 through 6
into a single reference document, with build recommendations for a faster path to launch.
Prepared for: WAISWA
Sponsor: MB Primary School
Document date: September 2026

## [Heading 1] Table of Contents

## [Heading 1] 0. How This Document Was Built
This SRS consolidates everything that previously existed as six separate, evolving documents — the founder's original raw brief, and Functional Requirements Specification versions 1 through 6 — into one single reference. Nothing in the original brief has been dropped: it is reproduced in full in Appendix A. Every requirement that survived to V6 (the most recent, most consolidated version) is restated here in one consistent format. Section 16 adds an independent assessment of build complexity, flagging what is likely to slow the project down and how to reach a working, sellable product faster.
Requirement IDs (FR-W#, FR-B#, FR-C#, FR-D#, FR-E#, FR-O#) are preserved from the V4–V6 numbering for continuity with prior discussion and any existing design work. A Phase G (Section 11) was added covering modules requested after the initial consolidation — document generation, fees, attendance, HR, and general school-office administration. Section 12 maps every school staff role to the features that serve it. Section 14 (Non-Functional Requirements) states explicit UI/UX quality and cross-module consistency standards. Section 16.7 addresses the scope trade-off this growth creates.

## [Heading 1] 1. Introduction

## [Heading 2] 1.1 Purpose
This document specifies the functional and non-functional requirements of Results Manager (RM): an offline-first desktop application for Ugandan primary and nursery schools, paired with a companion website for distribution, subscription, and product administration. RM's job is to run the operational core of a school's academic year — registering learners and staff, capturing marks, producing every printed document a parent or a PLE administrator expects to see, and preserving that history for years — without depending on an internet connection that Ugandan schools cannot reliably guarantee.

## [Heading 2] 1.2 Scope
The first shipping edition covers Nursery (Baby, Middle, Top) and Primary (P1–P7) under a single installer, sponsored by MB Primary School. It includes: a worldwide grading-systems library, weekly-assignment tracking, a mandatory dual-external-SSD data mirror, a developer-facing admin panel on the website, and an encrypted cloud-backup endpoint that is built but switched off at launch. Secondary and University editions are out of scope for this release (see Section 15, Roadmap).

## [Heading 2] 1.3 Document Structure
The requirements are organised into seven build phases, each depending on the one before it:
Phase A — RM Official Website: distribution, payment, credentials, rescue tooling
Phase B — Institutional Configuration: everything the School Admin sets up and maintains
Phase C — Daily Operations: what teachers do day to day
Phase D — Outputs and Reporting: every printed and exported document
Phase E — Lifecycle, Audit, Recovery, and Multi-Year History
Phase F — Developer / Owner Admin Panel: business metrics, on the website only
Phase G — Extended School Operations: document generation, fees, HR, and general office administration

## [Heading 1] 2. Product Overview

## [Heading 2] 2.1 Core Promise
A Ugandan primary school pays once a term (or once a year), installs RM on one PC plus two external SSDs, and produces every printed output a parent or PLE administrator expects — registration records, mark sheets, exam permits, class lists, registers, and report cards — without ever needing a reliable internet connection.

## [Heading 2] 2.2 Working Tagline
"Marks in. Reports out. Offline."

## [Heading 2] 2.3 Target Users
Private Ugandan primary schools with 100–800 learners
Nursery sections attached to a primary school
Headteachers and director-owners — the buyers, not IT staff
Initial geography: Kampala, Wakiso, Mukono, Jinja, Mbarara, expanding outward

## [Heading 1] 3. Actors and Roles
RM has two operational actors inside the desktop application, one external service, and one actor on the product-owner side.

## [Heading 3] 3.1 RM Official Website
Hosts installer downloads, runs subscription payments, generates the first School Admin's credentials, distributes the rescue tool, and hosts the owner admin panel.

## [Heading 3] 3.2 School Admin
Configures and runs the institution day to day. Up to three per school. Owns dual-SSD setup, subscription monitoring, and feature toggles.

## [Heading 3] 3.3 Teacher
Operates as a Subject Teacher, a Class Teacher, or both (same credentials, different mode).

## [Heading 3] 3.4 RM Owner / Developer
The business owner of RM. Sees only aggregate subscription and geography metrics on the website's owner panel — never institutional or student data.

## [Heading 1] 4. System Architecture Overview

## [Heading 2] 4.1 The Modular Level Model
RM is built as a shared Core Layer plus exactly one Level Pack per installer. The Core Layer covers everything that doesn't vary by school level — login, branding, marks entry, audit, backup, subscription, multi-year history, the SSD mirror. The Level Pack covers what does vary — class list, default grading, exam labels, report-card template, promotion rules. This release ships one Level Pack: Nursery + Primary. Secondary (S1–S6, UCE/UACE grading) and University (Y1–Y4+, GPA) are future Level Packs on the same Core Layer, built only once Nursery + Primary is stable in real schools and funding or demand supports it.

## [Heading 2] 4.2 Worldwide Grading Systems Library
Each class gets a sensible default grading system; the School Admin (institution-wide) or Class Teacher (per subject, per class) can pick any of the following, or build a custom one:

[TABLE]
System | Description
Nursery Descriptive | Default for Baby/Middle/Top. Five descriptive, non-numeric bands (Excellent → Needs Improvement) suited to early-childhood education.
UNEB Primary | Default for P1–P7. Standard Ugandan primary numeric grading, 1 (Distinction) through U.
Standard A–F | Internationally common letter grading.
UK GCSE 9–1 | England/Wales/Northern Ireland secondary examinations; 9 highest, 1 lowest passing grade.
US GPA 4.0 | Letter grades map to grade-points; GPA is the credit-weighted mean.
International Baccalaureate 1–7 | IB World Schools grading; 7 is highest.
Cambridge International A*–G | Cambridge IGCSE and equivalents; A* highest, U ungraded.
Percentage Only | Raw percentage, no band conversion.
Custom Band Builder | School Admin defines bands manually — label, lower/upper bound, optional remark. Fallback for any policy that doesn't match a preset, and the tool for fine-tuning a preset.
[/TABLE]

For high-stakes use, thresholds shown by any preset should be verified against the relevant examination board — boards revise boundaries from time to time, and RM's custom band builder lets a School Admin correct any threshold before going live.

## [Heading 2] 4.3 Hardware Requirements
A Windows PC with at least 8 GB RAM and 256 GB internal storage
Two external SSDs, 256 GB minimum (512 GB recommended), USB 3.0/USB-C — SSD is mandatory, not HDD, because school environments are dusty and HDDs are impact-sensitive
Two free USB ports
A reliable power source, ideally with a UPS, to let in-flight writes complete safely during outages
Internet — not required day to day, but required for the initial download, subscription payments, software updates, feedback submission, and the owner's website panel
A printer, for every document RM produces

## [Heading 1] 5. Phase A — RM Official Website
The public-facing website: product information, subscription sign-up and payment, credential issuance, installer distribution, the software-rescue tool, the School Admin's online account, onboarding materials, and the feedback channel.
FR-W1  Public Landing and Product Information

[TABLE]
Description | A public landing page describes RM, its features, pricing, and which institution levels are available. Nursery + Primary is marked Available; Secondary and University are marked as future releases, gated on Nursery + Primary running stably and on funder interest, with an interest-list signup for schools who want to be notified.
Inputs | Visitor traffic to the site.
Processing | Static marketing content; anonymous funnel metrics (visits → sign-ups → payments) feed the owner dashboards in Phase F.
Outputs | Public marketing pages; entry point into the sign-up flow.
Priority | High
Depends on | Website hosting being live.
[/TABLE]

FR-W2  Subscription Plans and Pricing — Termly and Annually

[TABLE]
Description | Two plans only: Termly (~120 days, one Ugandan school term) and Annually (365 days). Daily/weekly/monthly tiers are deliberately excluded because schools budget termly, not by the day. Each plan carries an exact day-count so expiry, grace period, and revenue reporting compute precisely. Prices are set in UGX, with USD shown optionally.
Inputs | Plan configuration set by the product owner.
Processing | Renders the two plan cards; on selection, the plan ID and duration flow into checkout and into revenue reporting.
Outputs | A selected plan with a defined duration.
Priority | High
Depends on | FR-W1
[/TABLE]

FR-W3  Institution Sign-Up Form with Level Selection and Full Geography

[TABLE]
Description | Before payment, the school fills in: institution name, level (only Nursery + Primary selectable at launch), continent, country (defaults to Uganda), region/state, district, and a contact person's name, email, and phone. Full geography is captured up front purely so the owner's geographic dashboard (FR-O2) has real data from day one.
Inputs | Institution and contact details; level; geographic breakdown.
Processing | Validates fields, checks for duplicate institutions, stores a pending record.
Outputs | A pending institution record and a checkout link.
Priority | High
Depends on | FR-W2
[/TABLE]

FR-W4  Secure Payment via a Ugandan Payment Gateway

[TABLE]
Description | Payment runs through a Ugandan-capable gateway (e.g. Flutterwave, Pesapal, or DPO Pay) supporting Visa/Mastercard, MTN Mobile Money, Airtel Money, and PayPal where available.
Inputs | Payment method chosen on the gateway page.
Processing | Redirects to the gateway, waits for a signed callback, verifies the transaction, and records the subscription's issue and expiry dates.
Outputs | A confirmed transaction; a persisted subscription; an emailed receipt; a revenue event for FR-O1.
Priority | High
Depends on | FR-W3
[/TABLE]

FR-W5  Automatic Credential Issuance for the First School Admin

[TABLE]
Description | Once payment clears, the website generates login credentials for the school's first School Admin and emails them. Credentials are never issued before payment is confirmed.
Inputs | A verified subscription from FR-W4.
Processing | Derives a username and a strong initial password, sends them over an encrypted connection.
Outputs | A credentials email; hashed credentials embedded into the installer.
Priority | High
Depends on | FR-W4
[/TABLE]

FR-W6  Level-Specific Installer with a One-Time Install Token

[TABLE]
Description | After credentials are issued, the site generates a one-time install token tied to the paying institution and to the Nursery + Primary level, and serves the matching installer bundle.
Inputs | Click on the download link.
Processing | Verifies and consumes the install token, selects the correct level bundle.
Outputs | A downloaded installer carrying the institution ID, install token, and level tag.
Priority | High
Depends on | FR-W5
[/TABLE]

FR-W7  Connector-RM — Software-Rescue Tool

[TABLE]
Description | A small rescue tool for one specific situation: the RM application on a school's PC has become corrupted or won't start, but the two mandatory external SSDs (FR-B12) are healthy. Connector-RM reads the data straight off the SSDs and reconstitutes a working RM installation around them — no reinstall, no data loss. It is not the tool for a dead PC (move the SSDs to a new machine and install fresh instead) and not the tool for lost/failed SSDs (only a cloud restore can help there, once cloud is enabled).
Inputs | An authenticated request from the institution's online account.
Processing | Verifies the subscription and level, issues Connector-RM, which on the school's PC locates the SSDs, checks their pairing record, and rebuilds the application around the existing data.
Outputs | A restored, working RM installation reading the school's existing data.
Priority | Medium
Depends on | FR-W6, FR-B12
[/TABLE]

FR-W8  Encrypted Cloud Backup Endpoint — Built, Disabled at Launch

[TABLE]
Description | An endpoint that receives client-side encrypted snapshots from desktop RM on a School-Admin-chosen schedule. Built and tested for V4 but shipped switched off, to be enabled in a later release once the core product is proven — at which point every institution gets cloud backup automatically, with no reinstall.
Inputs | (When enabled) an encrypted snapshot uploaded by desktop RM.
Processing | (When enabled) authenticates the institution, verifies the snapshot, stores the blob, and retains the most recent snapshots.
Outputs | (When enabled) stored encrypted snapshots and a latest-snapshot pointer used by disaster recovery (FR-E4).
Priority | High to build; disabled at launch
Depends on | FR-W6
[/TABLE]

FR-W9  Institution Online Account

[TABLE]
Description | Each paying institution gets a simple self-service account on the website: subscription status, invoices and receipts, renewal, reissuing a lost install token, and downloading Connector-RM.
Inputs | Login by contact email.
Processing | Renders the account page from the institution's records.
Outputs | A self-service account portal.
Priority | Medium
Depends on | FR-W4
[/TABLE]

FR-W10  Description Manual and Optional Troubleshooting Quiz

[TABLE]
Description | A plain-English manual for a new School Admin, covering: preparing the PC and two SSDs; first run and hardware binding; setting up the academic calendar, classes, subjects, and teachers; day-to-day operation; what the subscription panel means and what happens at expiry; the three disaster-recovery scenarios; and common errors and fixes. Available as PDF and web pages, downloadable for offline reading. An optional multiple-choice quiz follows it and issues a printable completion certificate if passed — but passing is never required to download or use RM.
Inputs | School Admin's reading; quiz attempts.
Processing | Serves manual content; grades the quiz locally and issues a certificate on pass.
Outputs | Manual content; an optional certificate.
Priority | Medium
Depends on | FR-W9
[/TABLE]

FR-W11  Feedback Form on the Website

[TABLE]
Description | A public form for bug reports, suggestions, or questions — name (optional for the public, prefilled for registered schools), email, category, subject, message, and an optional attachment. Submissions land in the owner admin panel for triage; submissions from a known institution are linked to that institution's account so the owner sees subscription and location context alongside the message. Rate-limited to deter spam.
Inputs | Form submissions.
Processing | Validates, rate-limits, stores, and forwards to the owner panel.
Outputs | Feedback entries in the owner panel; an acknowledgement email.
Priority | Medium
Depends on | FR-W1
[/TABLE]


## [Heading 1] 6. Phase B — Institutional Configuration (School Admin)
Everything the School Admin sets up once and maintains over time: hardware binding, the dual-SSD data-safety mirror, institution identity, the academic calendar, classes and subjects, teacher accounts, optional feature toggles, fees rules, theme, backup status, the subscription tracker, software updates, and in-app feedback.
FR-B0  First-Run Hardware Binding (PC Lock)

[TABLE]
Description | On first launch, RM binds itself to the host PC's hardware identity and refuses to run on any other machine.
Inputs | Motherboard serial, primary disk serial, primary network adapter ID, OS install ID.
Processing | Hashes the hardware fingerprint together with the install token into a machine-bound activation key; checked at every startup.
Outputs | A hardware-bound activation.
Priority | High
Depends on | FR-W6
[/TABLE]

FR-B12  Mandatory Dual External SSD Mirror

[TABLE]
Description | RM only operates with two external SSDs connected and recognised as its paired mirror. Every write goes to both drives; if one fails, RM keeps running on the survivor and warns the School Admin loudly; plugging in a replacement triggers an automatic, checksum-verified rebuild. Full operational detail — initialisation, the write sequence, why RM does not delegate to Windows RAID, power-loss handling, failure detection, rebuild, safe eject for off-site rotation, and hardware requirements — is in section 7.1.
Inputs | Two external SSDs (256 GB minimum, 512 GB recommended, USB 3.0/USB-C); School Admin action at first run and on drive replacement.
Processing | See section 7.1.
Outputs | Two mirrored, verified copies of the institution's data; live mirror-health status on the dashboard.
Priority | High
Depends on | FR-B0
[/TABLE]

FR-B1  Unified Login Frame with Role-Based Routing

[TABLE]
Description | One login frame for every role. RM identifies whether the credentials belong to a School Admin or a Teacher and routes to the matching dashboard.
Inputs | Username and password.
Processing | Matches credentials against stored hashes, identifies the role, loads the dashboard.
Outputs | The correct dashboard, or an access-denied message.
Priority | High
Depends on | FR-B0, FR-B12, FR-W5
[/TABLE]

FR-B2  Institution Name (Compulsory) and Logo (Optional)

[TABLE]
Description | The institution's name is compulsory; a logo is optional and never blocks setup. Both appear throughout the application and on every output.
Inputs | Institution name; optional logo (max 2 MB).
Processing | Stores the identity and binds it to all output templates.
Outputs | A branded application.
Priority | High
Depends on | FR-B1
[/TABLE]

FR-B3  Define Academic Calendar

[TABLE]
Description | The School Admin sets the academic calendar. Sensible Ugandan defaults pre-fill: 3 terms a year, with BOT / MID / EOT examinations each term — adjustable if a school's calendar differs.
Inputs | Number of terms; exams per term; weighting.
Processing | Builds the academic calendar.
Outputs | A configured year with examination slots.
Priority | High
Depends on | FR-B2
[/TABLE]

FR-B4  Add School Admins (Up to Three)

[TABLE]
Description | Up to three School Admin accounts per school; the first admin (created at sign-up) can add the other two.
Inputs | Name, email, initial password.
Processing | Enforces the cap of three; stores hashed credentials.
Outputs | Up to three active School Admins.
Priority | Medium
Depends on | FR-B1
[/TABLE]

FR-B5  Define Classes and the Institutional Subject Catalogue

[TABLE]
Description | The default class list pre-fills as Baby, Middle, Top, P1–P7. Each class gets a sensible default grading system (Nursery Descriptive for Baby/Middle/Top; UNEB Primary for P1–P7), changeable to any system from the grading library. The School Admin also owns the master subject catalogue — every subject the institution ever teaches, each with a stable ID that survives renames — from which Class Teachers later pick and customise per class (FR-C13). The registration-number format is set here too, with uniqueness enforced institution-wide.
Inputs | Class list; subject catalogue (code, max score, pass mark); registration-number pattern; per-class default grading.
Processing | Stores the academic structure with defaults applied. The promotion ladder (Baby → Middle → Top → P1 → … → P7 → PLE candidate) is wired in here regardless of any class-name customisation.
Outputs | A configured class hierarchy, subject catalogue, and registration-number generator.
Priority | High
Depends on | FR-B3
[/TABLE]

FR-B6  Assign Teachers to Classes and Subjects

[TABLE]
Description | Every class gets exactly one Class Teacher and at most one Assistant; everyone else assigned to that class is a Subject Teacher. The same rule applies per stream if streams are switched on.
Inputs | Teacher name, email, role, scope.
Processing | Enforces the caps, generates credentials, stores the mapping.
Outputs | Provisioned teacher accounts.
Priority | High
Depends on | FR-B5
[/TABLE]

FR-B7  Optional Features Panel

[TABLE]
Description | Four toggles, all off by default: Student Photos, Exam Permits, Streams (up to 20 per class, each with its own Stream Class Teacher/Assistant, roster, and add/drop tracking), and Weekly Assignments (per-subject weekly scoring that later feeds a summary onto the back of the report card). Turning a toggle off when data already exists asks for confirmation; the data is kept, just hidden.
Inputs | Toggle values.
Processing | Stores each as a feature flag; the UI and outputs adjust live.
Outputs | A feature set tailored to the school.
Priority | High
Depends on | FR-B2
[/TABLE]

FR-B8  Fees-Block Rule for Outputs

[TABLE]
Description | The School Admin flags students with incomplete fees; RM then refuses to print exam permits or report cards for those students until the flag is cleared.
Inputs | Per-student fees-status flag; a master on/off switch for the rule.
Processing | Checks the flag at print time and skips blocked students, with a clear notice.
Outputs | Filtered output plus a notice of who was blocked.
Priority | Medium
Depends on | FR-B6
[/TABLE]

FR-B9  Theme Selection

[TABLE]
Description | A selectable visual theme (Dark, Light, etc.) applied system-wide.
Inputs | Theme choice.
Processing | Persists and applies the theme.
Outputs | A themed interface.
Priority | Low
Depends on | FR-B1
[/TABLE]

FR-B10  Backup Status and Future Cloud Controls

[TABLE]
Description | The dashboard shows live dual-SSD mirror health. Cloud-backup controls (schedule, on-demand backup, restore) are visible but greyed out and labelled 'Coming soon' until FR-W8 is switched on — at which point they activate with no reinstall required.
Inputs | Mirror status; future backup settings.
Processing | Displays mirror health continuously.
Outputs | Visible, current backup status.
Priority | High for mirror status; disabled for cloud
Depends on | FR-B12, FR-W8
[/TABLE]

FR-B11  Subscription Tracker, Grace Period, and Read-Only Mode

[TABLE]
Description | A precise expiry model: a yellow banner at 30% of the term remaining, red at 10%. At expiry, a 7-day grace period runs with full functionality but a persistent expiry banner. After the grace period, RM drops into Read-Only Mode — login, viewing, re-printing existing report cards, and exporting all still work, but no new marks, no new learners, no promotion, and no setting changes are allowed. Renewing on the website restores full operation the moment the PC next has internet. Data is never deleted at any point.
Inputs | Subscription expiry date; renewal events.
Processing | Tracks days remaining, applies the banner/grace/read-only state machine.
Outputs | Accurate subscription status and enforcement.
Priority | High
Depends on | FR-W4
[/TABLE]

FR-B13  Software Update Channel

[TABLE]
Description | A path for installed copies of RM to update to a newer build over the same website channel used for installation, without a full reinstall. Takes a snapshot before applying an update so a failed update can roll back to the prior working state.
Inputs | Available update on the website; School Admin's choice to apply it.
Processing | Downloads the update, snapshots the current state, applies the update, verifies, and offers rollback on failure.
Outputs | An updated installation with a rollback point; a changelog view.
Priority | High
Depends on | FR-W1, FR-B0, FR-B12, FR-E2
[/TABLE]

FR-B14  In-Application Feedback Channel

[TABLE]
Description | A Feedback menu reachable from any dashboard: category, subject, message, and an optional one-click screenshot (institutional data is auto-blurred unless the user chooses to include it). Sent immediately if online, or queued locally and sent on next internet contact. Tagged with the institution and RM version so the developer has full context. No student data is ever attached automatically.
Inputs | Feedback form fields; optional screenshot.
Processing | Stores locally if offline, transmits when online, tags with context.
Outputs | Feedback entries in the owner panel; an in-app confirmation.
Priority | Medium
Depends on | FR-B1, FR-W11
[/TABLE]


## [Heading 1] 7. Phase C — Daily Operations (Teachers)
Everything teachers do day to day: scoped access, marks entry, analytics, deadlines, student search, bio-data upkeep, correction messages, roster changes, streams, weekly assignments, per-class subject and grading management, and the controlled path for entering a late mark.
FR-C1  Scoped Teacher Access by Class and Subject

[TABLE]
Description | A teacher can only see and act on the classes and subjects they've been assigned.
Inputs | Selected class/subject; credentials.
Processing | Verifies credentials against the FR-B6 assignment.
Outputs | Scoped access, or a denial.
Priority | High
Depends on | FR-B6
[/TABLE]

FR-C2  Dual-Mode Login: Subject Teacher or Class Teacher

[TABLE]
Description | The same credentials can open either the Subject Teacher view or, if authorised, the Class Teacher view.
Inputs | Mode choice.
Processing | Checks authorisation and loads the right dashboard.
Outputs | The Subject Teacher or Class Teacher dashboard.
Priority | High
Depends on | FR-C1
[/TABLE]

FR-C3  Marks Entry — Three Methods

[TABLE]
Description | Marks can be entered by uploading an Excel sheet, typing directly into an in-app spreadsheet, or using a guided form — whichever a teacher prefers. Every method validates against the subject's maximum score and shows a live grade preview using the class's active grading system.
Inputs | A file, in-app edits, or form entries; the term and exam.
Processing | Validates and stores marks against (student, subject, term, exam).
Outputs | Saved marks, a live grade preview, and validation messages.
Priority | High
Depends on | FR-C2
[/TABLE]

FR-C5  Subject Teacher Dashboard — Subject-Level Analytics

[TABLE]
Description | Graphs and summary reports of how the class performed in a subject, using that class's active grading system.
Inputs | Marks already entered.
Processing | Aggregates distributions, averages, and pass rates.
Outputs | An analytics view.
Priority | Medium
Depends on | FR-C3
[/TABLE]

FR-C6  Class Teacher Dashboard — Deadline and Progress

[TABLE]
Description | The Class Teacher sets a marks-entry deadline; the dashboard shows, in real time, what percentage of each Subject Teacher's marks are in and a countdown to the deadline.
Inputs | Deadline; live entry data.
Processing | Computes per-teacher progress continuously.
Outputs | Per-teacher progress figures.
Priority | Medium
Depends on | FR-C2
[/TABLE]

FR-C7  Class Teacher Dashboard — Search by Name or Registration Number

[TABLE]
Description | A search box that finds a student by name or by their institution-wide-unique registration number.
Inputs | Search term.
Processing | Matches on name or reg. number.
Outputs | A list of matches.
Priority | Medium
Depends on | FR-B5, FR-C2
[/TABLE]

FR-C8  Bio-Data Management by Class Teacher

[TABLE]
Description | The Class Teacher edits a student's bio-data — name, photo (when the Photos toggle is on), gender, LIN — with every change audited.
Inputs | Bio-data fields; photo.
Processing | Validates and writes an audit entry.
Outputs | An updated, audited record.
Priority | Medium
Depends on | FR-C2, FR-B7
[/TABLE]

FR-C9  Correction Messages from Subject Teacher to Class Teacher

[TABLE]
Description | A Subject Teacher who spots wrong bio-data can flag it directly to the Class Teacher and Assistant.
Inputs | Message text; the affected student.
Processing | Routes to the relevant inboxes.
Outputs | An inbox entry, tracked to resolution.
Priority | Medium
Depends on | FR-C8
[/TABLE]

FR-C10  Add and Drop Learners (Never Delete)

[TABLE]
Description | Learners are added to a class, or dropped from it — never deleted. Each class keeps an Added folder and a Dropped folder, which feed the ranking view in FR-E1.
Inputs | Bio-data (on add); a reason (on drop).
Processing | Moves learners between folders without erasing history.
Outputs | An updated roster and preserved history.
Priority | High
Depends on | FR-C7
[/TABLE]

FR-C11  Stream Management (Active Only When the Streams Toggle Is On)

[TABLE]
Description | Up to 20 streams per class, each behaving like its own mini-class with a Stream Class Teacher and Assistant and the same capabilities described in FR-C6–FR-C10.
Inputs | Same as a class, scoped to a stream.
Processing | Identical logic to a class, gated by the Streams toggle.
Outputs | Per-stream rosters, dashboards, and folders.
Priority | High when on; inert otherwise
Depends on | FR-B5, FR-B6, FR-B7, FR-C6–FR-C10
[/TABLE]

FR-C12  Weekly Assignments — Recording and Aggregation

[TABLE]
Description | Active only when the Weekly Assignments toggle is on. Each week, the Subject Teacher records a per-learner, per-subject assignment score and an optional remark, using the same three entry methods as FR-C3. At term end, RM aggregates these into a per-week list, a term mean, a term grade, and an attendance count — feeding both the report card back page (FR-D1) and multi-year tracking (FR-E6).
Inputs | Weekly score per learner per subject; optional remark.
Processing | Stores by (student, subject, term, week); aggregates at term end.
Outputs | Stored weekly scores and an end-of-term summary.
Priority | High when on; inert otherwise
Depends on | FR-B7, FR-C3
[/TABLE]

FR-C13  Class Teacher Subject Management and Per-Subject Grading Editor

[TABLE]
Description | A table on the Class Teacher's dashboard listing every subject active in their class, with actions to add a subject from the institutional catalogue, remove one (hiding it going forward without touching historical marks), rename it for display in that class only, and set its grading system independently of the class default or of how the same subject is graded elsewhere. Every change is audited. The stable subject ID from the catalogue never changes, so renames don't break multi-year history.
Inputs | Add / Remove / Rename / Set-Grading actions; subject picked from the catalogue; grading choice or custom bands.
Processing | Updates the class's subject set and grading overrides while preserving the stable subject ID.
Outputs | An updated per-class subject set and grading overrides, fully audited.
Priority | High
Depends on | FR-B5, FR-C2, FR-E2
[/TABLE]

FR-C14  Late Marks Entry for a Single Student (Missed Exam, With Approval)

[TABLE]
Description | When a student missed an exam and the term is already closed, the Class Teacher can request a one-time, single-cell reopening for that student/subject/exam, with a stated reason. A School Admin reviews and approves or rejects. On approval, the Class Teacher enters the one mark; RM regenerates that student's report card, recomputes class ranking, updates any sealed yearly record, and writes a complete audit trail covering the request, the decision, and every downstream change. The School Admin can reverse an approval within 24 hours.
Inputs | The Class Teacher's request (student, term, exam, subject, reason); the School Admin's decision; the late mark.
Processing | Gates the edit on approval, opens exactly one cell, then recomputes everything downstream.
Outputs | An updated record, a regenerated report card, recomputed ranking, and a full audit trail.
Priority | High
Depends on | FR-C3, FR-C8, FR-E2, FR-E3, FR-D1
[/TABLE]


## [Heading 1] 8. Phase D — Outputs and Reporting
Every printed and exported artefact RM produces, and the single global rule that governs how every one of them is branded.
FR-D1  Partial and Final Report Cards, with Optional Weekly-Assignment Back Page

[TABLE]
Description | The Class Teacher prints partial (e.g. mid-term) and end-of-term report cards. The front carries class, term, per-subject grades (using each subject's class-active grading system), aggregate, class position, class teacher's and headteacher's comments, and the institution's own branding — never a sponsor or any other third-party name. If Weekly Assignments is on, the back carries that term's weekly-assignment summary. Respects the fees-block rule and the photos toggle.
Inputs | Selected exam set, term, class/stream; weekly-assignment data if applicable.
Processing | Aggregates, grades, ranks, and renders through the global print pipeline (FR-D4).
Outputs | A printable/exportable report card, front and optional back.
Priority | High
Depends on | FR-C3, FR-C12, FR-C13, FR-B3, FR-B5, FR-B7, FR-B8, FR-D4
[/TABLE]

FR-D2  Examination Permits, Class Lists, Mark Lists, Mark Sheets, Registers

[TABLE]
Description | The other class-wide documents a school needs, all routed through the same global print pipeline so they carry only the institution's branding. Exam Permits print only if that toggle is on; fees-block and other toggles are respected throughout.
Inputs | Class/stream, document type, term/exam context.
Processing | Populates the relevant template and applies branding via FR-D4.
Outputs | Printable/exportable documents.
Priority | High
Depends on | FR-D1, FR-B7, FR-D4
[/TABLE]

FR-D4  Global Print Pipeline

[TABLE]
Description | Every document RM produces — report cards, permits, lists, sheets, registers, multi-year exports, weekly-assignment exports, any future export — passes through one single pipeline. It stamps the institution's name and logo in the header, and 'Generated by Results Manager' plus the page number and print date in the footer (much like a PDF noting it was 'Created with Adobe Acrobat'). It never stamps a sponsor name or any third-party attribution, for any institution. No requirement is permitted to bypass this pipeline.
Inputs | Any document/export request from any other requirement.
Processing | Applies institution branding and the fixed footer; refuses any other attribution.
Outputs | A consistently branded, attributed document.
Priority | High
Depends on | FR-B2
[/TABLE]


## [Heading 1] 9. Phase E — Lifecycle, Audit, Recovery, and Multi-Year History
How RM tracks activity over time, promotes learners, recovers from failure, and builds a long-term academic record for the school.
FR-E1  Ranking of Class Add/Drop Activity

[TABLE]
Description | Aggregates every class's Added/Dropped folders and ranks classes by gain and loss percentage, with a per-stream breakdown when streams are on.
Inputs | Folder contents.
Processing | Computes and sorts gain/loss percentages.
Outputs | Two ranking views (gained most, lost most).
Priority | Medium
Depends on | FR-C10, FR-C11
[/TABLE]

FR-E2  Activity Tracking and Audit Log

[TABLE]
Description | Every action, by every actor, is logged with a timestamp — searchable from the School Admin dashboard, and precise enough to tell a Class Teacher's change from their Assistant's.
Inputs | All user actions.
Processing | Writes immutable, indexed audit entries.
Outputs | A searchable audit trail.
Priority | High
Depends on | FR-B4, FR-B6
[/TABLE]

FR-E3  Promotion of Learners

[TABLE]
Description | Learners move up the ladder — Baby → Middle → Top → P1 → P2 → … → P7 → PLE candidate — at term and year boundaries, without ever distorting historical records. Outgoing P7s are archived as graduates with their final aggregate after PLE results.
Inputs | End-of-term/year trigger; Class Teacher confirmation.
Processing | Advances each learner one rung; produces a sealed yearly record at year-end.
Outputs | Updated rosters; sealed yearly records; a PLE-candidate list.
Priority | High
Depends on | FR-B3, FR-B5, FR-C10
[/TABLE]

FR-E4  Disaster Recovery — Three Scenarios, Three Paths

[TABLE]
Description | Scenario 1 — PC dies, SSDs healthy: install fresh on a new PC, plug in both SSDs, RM recognises them and resumes; the cleanest and most common recovery. Scenario 2 — RM binary corrupted, SSDs healthy: run Connector-RM (FR-W7) to rebuild the application around the existing SSDs. Scenario 3 — SSDs lost or destroyed: recoverable only via cloud restore once that feature is enabled (FR-W8); until then this is the one irreducible risk, mitigated by rotating one SSD off-site.
Inputs | Identification of which scenario applies; the matching recovery tool.
Processing | Verifies institution identity and level, performs the matching recovery, runs integrity checks, logs the recovery.
Outputs | A restored, hardware-bound RM installation.
Priority | High
Depends on | FR-B0, FR-B12, FR-W6, FR-W7, FR-W8
[/TABLE]

FR-E5  Multi-Year Academic History — Subject Trends

[TABLE]
Description | Sealed yearly records are kept indefinitely. The School Admin can view year-over-year trends two ways: class-level ('how has P5 Mathematics performed over 5 years') and institution-level ('how has Mathematics performed school-wide over 5 years'), filterable by class, subject, year, and stream, and exportable.
Inputs | Sealed yearly records.
Processing | Aggregates per-year metrics at class and institution level.
Outputs | A multi-year trends dashboard.
Priority | High
Depends on | FR-E3
[/TABLE]

FR-E6  Multi-Year Weekly-Assignment Tracking

[TABLE]
Description | Active once Weekly Assignments has run for at least a full term. Tracks per-year mean weekly scores by class, subject, and institution over time, and whether weekly performance correlates with exam performance (FR-E5).
Inputs | End-of-term weekly aggregations.
Processing | Aggregates per year; computes trend and correlation.
Outputs | A multi-year weekly-assignment dashboard.
Priority | High when on; inert otherwise
Depends on | FR-C12, FR-E5
[/TABLE]


## [Heading 1] 10. Phase F — Developer / Owner Admin Panel (Website)
A website-only panel, visible solely to the RM owner, dealing exclusively in aggregate subscription and geography metrics — never institutional or student data.
FR-O0  Owner Panel Authentication — Single-Owner Account Only

[TABLE]
Description | Exactly one account exists: the owner's. No sign-up form, no delegation, no team accounts. The panel lives on an unlisted subdomain linked from nowhere public. Login requires a password plus a mandatory TOTP code; sessions are short-lived and IP-bound; failed logins are rate-limited.
Inputs | Password and TOTP code.
Processing | Validates both factors, checks IP rate limits, issues a bound session.
Outputs | An authenticated owner session — nothing else.
Priority | High
Depends on | FR-W1
[/TABLE]

FR-O1  Revenue Dashboard

[TABLE]
Description | Breaks revenue down by plan tier (Termly vs Annually) and separately by time window (today/week/month/year), with period-over-period comparison and drill-down into anonymised transactions (reference, plan, amount, date, country — never contact details).
Inputs | Subscription records; the current date.
Processing | Aggregates by tier and by window; computes period comparisons.
Outputs | A revenue dashboard with drill-down.
Priority | High
Depends on | FR-O0, FR-W2, FR-W4
[/TABLE]

FR-O2  Geographic Dashboard

[TABLE]
Description | Total subscribers with active-vs-expired split; a drill-down hierarchy World → Continent → Country → Region → District; a subscriber-density heat map; and ranked tables at every level.
Inputs | Institution geography from FR-W3.
Processing | Aggregates counts at each geographic level.
Outputs | A geographic dashboard with map and rankings.
Priority | High
Depends on | FR-O0, FR-W3
[/TABLE]

FR-O3  Growth and Churn Dashboard

[TABLE]
Description | For each period (day/week/month/year): new subscriptions, lost subscriptions (with a grace window separating 'churned' from merely 'lapsed'), net growth, and cumulative active subscribers — with drill-down into anonymised institution lists per bucket.
Inputs | Sign-up and subscription records.
Processing | Aggregates new/lost counts, computes net growth and running totals.
Outputs | A growth & churn dashboard.
Priority | High
Depends on | FR-O0, FR-W3, FR-W4
[/TABLE]

FR-O4  Data Privacy Boundary on the Owner Panel

[TABLE]
Description | The owner panel is architecturally restricted to subscription and geography tables. Marks, report cards, learner and teacher names, photos, and bio-data never leave the institution's own PC and SSD mirror. Cloud snapshots, once enabled, are encrypted with a key the owner does not hold.
Inputs | —
Processing | All panel queries are scoped to non-institutional tables.
Outputs | A panel that cannot, by construction, expose institutional data.
Priority | High
Depends on | FR-O0
[/TABLE]


## [Heading 1] 11. Phase G — Extended School Operations
Modules added after the initial six phases, extending RM from a marks-and-reports system into a fuller school-operations tool, while keeping the same offline-first, single-PC architecture and the same quality bar as the Core Layer. Each module below is a separate build item in the phased plan (Section 16) — they are grouped here by subject matter, not by build order.
FR-G1  Document Generator — Custom Report Builder and Template Upload

[TABLE]
Description | A single templating engine reused by report cards (FR-D1), admin letters, and certificates, so the product maintains one document-generation system rather than several. Two ways to produce a custom layout: (a) Custom Builder — the School Admin arranges available fields (grades table, comments, photo, logo placement, signature lines) on a canvas inside RM and saves it as a named template; (b) Template Upload — the School Admin uploads their own Word document containing placeholder tags (e.g. {{student_name}}, {{class}}, {{subject_table}}, {{guardian_name}}, {{date}}), and RM validates that every placeholder maps to a real field, clearly flagging any that don't, before the template can be used. Built-in starter templates are provided for: admission letter, transfer letter, dismissal letter, recommendation letter, general circular, completion certificate, good conduct certificate, and achievement certificate. Every generated document — whichever path produced it — passes through the same FR-D4 global print pipeline for institution branding.
Inputs | Field arrangement (Custom Builder) or an uploaded Word template with placeholders; the record(s) to merge in.
Processing | Custom Builder: RM renders the saved layout directly. Template Upload: RM parses the document, matches placeholders to real fields, reports any unmapped placeholder before allowing use, then merges data in at generation time, handling repeating sections (e.g. a variable-length subjects table) correctly.
Outputs | A ready-to-print or exportable document (report card, letter, or certificate) in the school's own chosen layout.
Priority | High for Custom Builder; Medium for Template Upload (higher engineering risk — see Section 14.7)
Depends on | FR-D4, FR-B5
[/TABLE]

FR-G2  Visitor Log (Visitation Card)

[TABLE]
Description | A front-desk register for anyone visiting a student. Records visitor name, relationship to the student, the student visited (searched by name or registration number, reusing FR-C7), purpose of visit, time in, time out, and a signature. RM prints a Visitation Slip the visitor carries during the visit. The School Admin gets a searchable daily/weekly log of all visits.
Inputs | Visitor details; student searched; purpose; time in/out.
Processing | Validates the student lookup against FR-C7's institution-wide records; timestamps entry and exit.
Outputs | A printed Visitation Slip; a searchable visitor log.
Priority | Medium
Depends on | FR-C7, FR-D4
[/TABLE]

FR-G3  School Overview Dashboard

[TABLE]
Description | A single landing screen for the School Admin combining, as at-a-glance cards: overall attendance rate (from FR-G9), academic performance summary (from existing marks data), fees status (from FR-G5), and staff presence (from FR-G4). This is a display layer only — it introduces no new data of its own, so it is naturally the last of the Phase G modules to build: it has nothing to show until attendance, fees, and staff records actually exist.
Inputs | Aggregated data from FR-G4, FR-G5, FR-G9, and existing marks records.
Processing | Pulls and summarises current-term figures from each source module.
Outputs | A single at-a-glance operational dashboard.
Priority | Medium
Depends on | FR-G4, FR-G5, FR-G9
[/TABLE]

FR-G4  Staff HR — Attendance, Leave, and Contracts

[TABLE]
Description | Daily staff attendance (clock-in/out or manual marking by the School Admin), leave requests and approval (type, dates, reason, approver), and a staff record per employee (contract type, start date, role, documents). Deliberately excludes payroll calculation (PAYE, NSSF, payslips) — see Section 14.7 for why. Where a school needs payroll, RM's staff records should be exportable to, or integrated with, a licensed Ugandan payroll provider rather than RM computing statutory deductions itself.
Inputs | Daily attendance marks; leave requests and decisions; staff contract details.
Processing | Tracks attendance history, routes leave requests to an approver, stores contract records with audit trail (FR-E2).
Outputs | Staff attendance history; leave records; a staff HR file per employee.
Priority | Medium
Depends on | FR-B6, FR-E2
[/TABLE]

FR-G5  Fees Ledger

[TABLE]
Description | Replaces the pass/fail fees-block flag in FR-B8 with an actual ledger: the School Admin sets a fee amount per class per term, payments are logged (date, amount, method, received-by), and RM computes a running balance per student. The fees-block rule (FR-B8) now reads from this ledger automatically instead of a manually-set flag — a student's exam permit or report card is blocked based on their real balance, not a separate switch someone has to remember to flip.
Inputs | Fee structure per class/term; individual payment records.
Processing | Maintains a running balance per student; feeds FR-B8's block rule and the Overview Dashboard (FR-G3).
Outputs | Per-student balance; payment history; an automatically-computed fees-block status.
Priority | High
Depends on | FR-B8, FR-B5
[/TABLE]

FR-G6  Timetable

[TABLE]
Description | A weekly grid per class/stream — subject, teacher, period, and day. RM checks for and warns on double-booking (the same teacher scheduled in two classes in the same period). Prints cleanly for posting in a classroom.
Inputs | Class/stream, subject, teacher, day, and period assignments.
Processing | Validates no teacher is double-booked across the whole institution's timetable.
Outputs | A printable weekly timetable per class/stream, and a conflict warning where relevant.
Priority | Medium
Depends on | FR-B5, FR-B6, FR-D4
[/TABLE]

FR-G7  Exams and Academic Calendar

[TABLE]
Description | One shared calendar visible to every teacher and the School Admin: term dates, holidays, school events, and scheduled exam dates/times per subject. The exam schedule set here is what drives Examination Permits (FR-D2) — a permit reflects the actual scheduled date rather than the School Admin tracking exam dates separately from the rest of the system.
Inputs | Term dates, holidays, events; per-subject exam date/time.
Processing | Stores the calendar centrally; feeds the exam date onto generated permits.
Outputs | A shared academic calendar; correctly-dated exam permits.
Priority | Medium
Depends on | FR-B3, FR-D2
[/TABLE]

FR-G8  SMS Notifications to Parents

[TABLE]
Description | Sends SMS alerts to a parent/guardian's phone number for: fees balance reminders, 'results are ready,' and attendance alerts (e.g. unexplained absence). Sent via a local SMS gateway (e.g. Africa's Talking or EgoSMS). Consistent with the offline-first model: RM only needs internet at the moment of sending a batch, not continuously — messages queue locally if offline and send on next internet contact, the same pattern already used for feedback (FR-B14).
Inputs | Guardian phone number (from bio-data, FR-C8); the triggering event (fees reminder, results ready, absence).
Processing | Queues messages locally, sends via the SMS gateway API when internet is available, logs delivery status.
Outputs | Delivered SMS alerts; a send/delivery log.
Priority | Medium
Depends on | FR-C8, FR-G5, FR-G9
[/TABLE]

FR-G9  Daily Student Attendance Register

[TABLE]
Description | A daily present/absent (and late, where relevant) register per class, marked by the Class Teacher. Distinct from the existing weekly-assignment 'attendance' metric in FR-C12, which measures assignment completion, not physical presence. Feeds a 'days present this term' figure onto the report card, the Overview Dashboard (FR-G3), and absence-alert SMS (FR-G8).
Inputs | Daily present/absent/late marks per student.
Processing | Stores daily records; aggregates a per-term attendance rate per student and per class.
Outputs | A daily register; a term attendance rate on report cards and dashboards.
Priority | High
Depends on | FR-C2, FR-B5
[/TABLE]

FR-G10  Guaranteed Full Data Export

[TABLE]
Description | Every table RM holds — learners, staff, marks, fees, attendance, audit log — can be exported to Excel/CSV at any time, in full, with no feature paywall on the export itself. This is a stated trust guarantee as much as a feature: a school's data is never held hostage inside RM.
Inputs | Export request; table/date-range selection.
Processing | Generates a complete Excel/CSV export through the FR-D4 pipeline.
Outputs | A complete, portable copy of the requested data.
Priority | Medium
Depends on | FR-D4
[/TABLE]

FR-G11  Report Card Comment Bank

[TABLE]
Description | A pool of common Class Teacher and Headteacher comments (e.g. 'Shows great improvement,' 'Needs to work harder on punctuality') that the Class Teacher picks from during report card generation instead of retyping similar wording for 40+ students every term. Institution-wide defaults are pre-loaded; the School Admin or Class Teacher can add their own phrases to the pool. A picked comment can still be edited per student before printing — the bank speeds up the common case, it never locks the teacher out of writing something specific.
Inputs | A comment pool (pre-loaded plus school-added entries); a per-student pick, with optional edits.
Processing | Stores the pool; surfaces relevant comments during report card generation (FR-D1); saves any per-student edit as free text without altering the pool.
Outputs | A comment attached to each student's report card, chosen quickly or typed fresh.
Priority | High
Depends on | FR-D1
[/TABLE]

FR-G12  Bulk Print Queue

[TABLE]
Description | Prints an entire class's, an entire stream's, or the whole school's report cards (or any other FR-D document) in a single action, with a print preview and a progress indicator, instead of one student at a time. Respects the fees-block rule (FR-B8/FR-G5) throughout — blocked students are automatically skipped and listed in a summary at the end, rather than silently included or requiring the operator to filter them out by hand.
Inputs | Selected class/stream/school scope; document type.
Processing | Queues each student's document, applies the fees-block filter, renders through the print pipeline (FR-D4), and reports progress without freezing the interface (Section 12.6).
Outputs | A batch of printed/exported documents, plus a summary of any students skipped and why.
Priority | High
Depends on | FR-D1, FR-D2, FR-D4, FR-B8
[/TABLE]

FR-G13  Onboarding Data Import

[TABLE]
Description | A guided first-setup step letting a new school upload their existing student and staff records from an Excel/CSV file, instead of manually re-entering 300+ learners one at a time. RM maps spreadsheet columns to its own fields (name, class, gender, registration number, guardian contact, etc.), flags rows that look wrong (missing required fields, a class name that doesn't match FR-B5's configured classes, a duplicate registration number) before import, and lets the School Admin fix or skip flagged rows rather than failing the whole import.
Inputs | An uploaded Excel/CSV file of existing student/staff records.
Processing | Parses the file, maps columns to RM fields, validates each row against FR-B5's class list and FR-B5's registration-number rules, and reports errors row-by-row before committing.
Outputs | A populated set of student/staff records; an import report listing anything skipped or flagged.
Priority | High
Depends on | FR-B5, FR-B6, FR-C10
[/TABLE]

FR-G14  Discipline and Conduct Log

[TABLE]
Description | A per-student record of merits, demerits, and incidents — date, type, description, and the staff member who logged it — separate from the dismissal-letter workflow already covered by the Document Generator (FR-G1). Feeds naturally into report-card comments (a pattern of merits can suggest a comment from the bank in FR-G11) and into parent communication (a serious incident can trigger the same SMS channel used for fees and attendance alerts, FR-G8).
Inputs | Incident/merit/demerit entries: student, date, type, description, logged-by.
Processing | Stores entries per student with an audit trail (FR-E2); optionally surfaces a pattern-based comment suggestion or an SMS trigger.
Outputs | A searchable per-student conduct history.
Priority | Medium
Depends on | FR-C8, FR-E2, FR-G8, FR-G11
[/TABLE]

FR-G15  Boarding Module — Dormitory Allocation and Roll Call (Feature-Gated)

[TABLE]
Description | A fifth optional toggle alongside FR-B7's existing four (Photos, Exam Permits, Streams, Weekly Assignments), off by default and only relevant to boarding schools. When enabled: dormitories can be defined, learners allocated to a dormitory and a bed/space, and a Dormitory Matron/Patron role can take a roll call (present/absent, with the same audit trail as the daily attendance register in FR-G9) at set times (e.g. evening prep, lights-out). Kept fully inert — no UI, no data model overhead — for the day-school majority of the customer base, consistent with how Streams already behaves when switched off.
Inputs | Dormitory list; learner-to-dormitory allocation; roll call marks.
Processing | Identical toggle-gating logic to FR-B7; roll call reuses the same present/absent pattern as FR-G9, scoped to a dormitory instead of a class.
Outputs | A dormitory roster; a roll call record; inert entirely when the toggle is off.
Priority | Low — build only once an actual boarding-school customer exists
Depends on | FR-B7, FR-G9, FR-E2
[/TABLE]

FR-G16  Student and Staff ID Cards

[TABLE]
Description | A founding requirement from the original project brief, restored here after being missed across V1–V6. Generates a printable ID card for every student and staff member: photo (when the Photos toggle, FR-B7, is on), name, class or role, registration/staff number, institution name and logo, and an expiry/academic-year marker so old cards are visibly outdated at a glance. Cards can be generated one at a time or in a batch for a whole class or staff list, sized for standard PVC card printers or as a print-and-laminate sheet for schools without a card printer.
Inputs | Student or staff record (bio-data, photo, role/class); print scope (single or batch).
Processing | Renders the card layout from the record, through the FR-D4 branding pipeline; batches follow the same queue-and-progress pattern as FR-G12.
Outputs | A printable/exportable ID card, single or batched.
Priority | High
Depends on | FR-C8, FR-B6, FR-B7, FR-D4, FR-G12
[/TABLE]

FR-G17  Admissions Pipeline

[TABLE]
Description | Tracks a prospective learner from first inquiry through to becoming an enrolled record, closing the gap between FR-G13 (bulk import of existing records) and FR-C10 (add/drop within an already-enrolled class). Stages: Inquiry (parent contact captured, no student record yet) → Application (admission form details, desired class, supporting documents) → Decision (School Admin marks accepted/waitlisted/declined, with a reason) → Enrolled (on acceptance, becomes a real student record in the target class via FR-C10, with the admissions history preserved and linked). Nothing is lost if an inquiry doesn't convert — declined and waitlisted applications stay searchable, so the school has a record of demand even when a seat isn't offered.
Inputs | Inquiry/contact details; application form data; the School Admin's decision.
Processing | Advances an application through the four stages; on 'Enrolled,' creates the student record via FR-C10 and links it back to its admissions history.
Outputs | A trackable admissions pipeline; an enrolled student record on acceptance; a retained history of every application regardless of outcome.
Priority | Medium
Depends on | FR-C10, FR-B5
[/TABLE]

FR-G18  Correspondence Log

[TABLE]
Description | A simple office register of incoming and outgoing letters and phone calls — date, from/to, subject, and a brief note — so the office can answer 'did we receive/send that' without relying on memory or a paper diary. Optionally links to a student, staff member, or guardian record when relevant, so a correspondence history can be pulled up alongside a student's file.
Inputs | Date, direction (in/out), correspondent, subject, note; optional link to a student/staff/guardian record.
Processing | Stores entries chronologically; indexes by correspondent and by linked record.
Outputs | A searchable correspondence log.
Priority | Low
Depends on | FR-C8, FR-B6
[/TABLE]

FR-G19  Meeting Minutes

[TABLE]
Description | Records for staff meetings, PTA meetings, and Board of Governors meetings: date, attendees (checked against staff records where applicable, FR-B6), agenda items, minutes text, and action points with an assignee and due date. Replaces the school's physical minute book with a searchable digital one. Action points can optionally be tracked to completion, giving the School Admin visibility into whether meeting decisions actually got followed up.
Inputs | Meeting date/type; attendee list; agenda; minutes text; action points (assignee, due date).
Processing | Stores the record; tracks each action point's status.
Outputs | A searchable minutes archive; an action-point tracker.
Priority | Low
Depends on | FR-B6
[/TABLE]

FR-G20  Petty Cash and Expenditure Log

[TABLE]
Description | Tracks money going out of the school — stationery, repairs, transport, small purchases — as a counterpart to the Fees Ledger (FR-G5), which only tracks money coming in. Each entry: date, amount, category, description, and who authorised it. Running balance shown per period (e.g. per term), giving the School Admin a basic income-vs-expenditure picture without building a full accounting system.
Inputs | Expense entries: date, amount, category, description, authoriser.
Processing | Maintains a running expenditure balance per period; can be viewed alongside FR-G5's income figures.
Outputs | An expenditure log; a basic income-vs-expenditure summary.
Priority | Medium
Depends on | FR-G5
[/TABLE]

FR-G21  School Store and Uniform Tracking

[TABLE]
Description | Tracks items the school sells or issues directly to students — uniforms, exercise books, stationery. For uniforms specifically: the School Admin defines available uniform items and sizes, and RM tracks which students have been assigned a uniform, which type/size, and which students have not yet been assigned one — surfaced as a clear 'assigned vs. not assigned' view per class, so the office can immediately see who still needs one. General store items (books, stationery) are tracked as simple stock-in/stock-out with a running quantity, optionally linked to a charge on the student's Fees Ledger record (FR-G5) when sold rather than issued free.
Inputs | Uniform/item catalogue (type, size, stock level); per-student assignment or sale record.
Processing | Maintains stock levels; tracks per-student uniform assignment status; optionally posts a charge to FR-G5 on sale.
Outputs | A stock register; an 'assigned vs. not assigned' uniform view per class; optional fees charges.
Priority | Medium
Depends on | FR-B5, FR-G5
[/TABLE]

FR-G22  Student Pass-Out / Exit Pass

[TABLE]
Description | Handles a student leaving campus before the normal end of day — picked up early, sick, or with permission. The Class Teacher or School Admin records: student, reason, who is picking the student up (name and relationship), time out, and an expected comeback time if the student is returning the same day. Generates a printed Pass-Out slip for the student/guardian to carry past the gate. An SMS is sent to the guardian's registered phone number (via FR-G8) confirming the student has left and stating the expected comeback time. If a comeback time was set and the student has not been marked as returned by then, RM flags the case on the School Admin dashboard rather than letting it go unnoticed.
Inputs | Student; reason; pickup person and relationship; time out; expected comeback time (if applicable).
Processing | Generates the slip through FR-D4; triggers an SMS via FR-G8; if a comeback time is set, monitors for a recorded return and raises a flag if it's missed.
Outputs | A printed Pass-Out slip; a guardian SMS notification; a dashboard flag on a missed comeback time.
Priority | High
Depends on | FR-C8, FR-D4, FR-G8, FR-G2
[/TABLE]

FR-G23  Bulk SMS Broadcast to Guardians

[TABLE]
Description | A free-form communication tool, distinct from FR-G8's automatic triggers (fees, results, attendance). The School Admin (institution-wide) or a Class Teacher (scoped to their own class) writes a message — a closure notice, a PTA meeting reminder, a change of reporting day, anything — and picks an audience: all guardians institution-wide, one class or stream, or a custom-picked list of individuals. Because bulk SMS carries a real per-message cost through the gateway, RM shows an estimated cost (message segments × recipient count) before sending, so a school is never surprised by its SMS bill. A message can be sent immediately or scheduled for a specific time, and follows the same offline-queue-then-send pattern as FR-G8 — it queues locally if there's no internet right now and sends on next connection. Delivery status is tracked per recipient, same as FR-G8.
Inputs | Message text; audience selection (all / class-or-stream / custom list); send-now or scheduled time.
Processing | Computes an estimated cost before sending; queues locally if offline; sends via the same SMS gateway integration as FR-G8 when online; tracks per-recipient delivery status.
Outputs | A sent or scheduled broadcast; an estimated-cost preview; a per-recipient delivery log.
Priority | Medium
Depends on | FR-G8, FR-C8, FR-B6
[/TABLE]


## [Heading 1] 12. Staff Roles and Duties — How RM Serves Each One
A Ugandan primary school runs on more roles than 'School Admin' and 'Teacher' suggests. This section maps the real staff roles in a typical school to the RM features that actually help each one, so it's clear who benefits from what — and honest about which real duties still fall outside RM today.

[TABLE]
Role | What RM Gives Them
Headteacher / Director | School Overview Dashboard (FR-G3); multi-year academic trends (FR-E5, FR-E6); subscription status (FR-B11); the institution's own branded identity across every output (FR-B2, FR-D4).
Deputy Headteacher / Director of Studies | Timetable (FR-G6) and the Exams & Academic Calendar (FR-G7); class/subject/teacher assignment oversight (FR-B5, FR-B6); marks-entry progress tracking (FR-C6); multi-year subject trends (FR-E5).
School Admin (up to 3 per school) | The full configuration surface: FR-B1–FR-B14, plus the Fees Ledger (FR-G5), Petty Cash (FR-G20), Admissions Pipeline (FR-G17), and every toggle and feature-gate in the system.
Class Teacher / Assistant Class Teacher | Bio-data (FR-C8), roster and add/drop (FR-C10), Class Teacher dashboard and deadlines (FR-C6), report card generation with the Comment Bank (FR-D1, FR-G11), Bulk Print Queue (FR-G12), Daily Attendance (FR-G9), Pass-Out slips (FR-G22).
Subject Teacher | Scoped marks entry in three methods (FR-C1–FR-C3), subject-level analytics (FR-C5), correction messages to the Class Teacher (FR-C9).
Stream Class Teacher / Assistant | The same capabilities as a Class Teacher, scoped to their stream, once Streams (FR-B7, FR-C11) is switched on.
Bursar / Accounts Officer | Fees Ledger with per-student running balance (FR-G5); Petty Cash and Expenditure Log (FR-G20); the fees-block rule feeding report cards and permits automatically (FR-B8).
Front Office / Secretary | Visitor Log (FR-G2); Admissions Pipeline (FR-G17); Correspondence Log (FR-G18); Student & Staff ID Cards (FR-G16); Student Pass-Out slips (FR-G22).
Discipline Master / Mistress | Discipline and Conduct Log (FR-G14), linked to the Comment Bank (FR-G11) and able to trigger a parent SMS (FR-G8) for serious incidents.
Matron / Patron (boarding schools) | The Boarding Module — dormitory allocation and roll call (FR-G15) — feature-gated, active only when a school switches it on.
Gate Security / Watchman | Verifies visitors against the Visitor Log (FR-G2) and departing students against a printed Pass-Out slip (FR-G22) — RM's contribution here is the paperwork the guard checks, not a separate guard-facing screen.
Store Keeper | School Store and Uniform Tracking (FR-G21) — stock levels, per-student uniform assignment status, and optional fees charges on sale.
Support staff (cooks, cleaners, drivers) | Covered by Staff HR for attendance, leave, and contract records (FR-G4). No role-specific module — see the gaps noted below.
[/TABLE]


## [Heading 2] 12.1 Duties Not Yet Covered
In the interest of an honest picture, a few real staff duties still fall outside RM as specified, and are intentionally left for a future release rather than rushed in:
Librarian — book issue/return tracking has no dedicated module yet; noted as a candidate for a future Phase G addition.
Sports/Games Master — inter-house competitions and co-curricular records aren't tracked; the Exams & Academic Calendar (FR-G7) can hold event dates, but not team rosters or results.
Health/Sick-Bay attendant — a dedicated health log (visits, known allergies, immunisation record) isn't specified; some schools will specifically ask for this.
Transport Officer / Driver — bus routes, fuel logs, and vehicle maintenance records aren't covered; relevant mainly to schools that run their own transport.
None of these block a launch — they're listed here so a future Phase G expansion is driven by a real gap analysis, not guesswork.

## [Heading 1] 13. Dual External SSD Mirror — Detailed Operational Model
Full detail behind FR-B12, referenced from Section 6.

## [Heading 2] Initialisation
At first run, RM detects external storage, and the School Admin selects two devices as the mirror pair. RM formats both with an RM-specific signature and a pairing manifest (pair ID, creation time, slot labels, institution ID), and refuses to leave setup until both are initialised — the mirror cannot be added later.

## [Heading 2] Normal operation
Every database change is written transactionally to both drives in sequence — begin marker, write, fsync, checksum-verify, repeat on the second drive, commit both. Any failed step rolls back on both drives.

## [Heading 2] Why the operating system is not trusted
RM does not delegate to Windows Storage Spaces or any OS-level RAID, since that would tie data safety to an admin-level Windows feature a typical school cannot manage. RM mirrors in user-space instead, so it behaves identically across Windows versions.

## [Heading 2] Power-loss handling
Given how common power outages are in Uganda: if power dies mid-write, RM scans both drives on next startup for incomplete transactions and rolls them back to the last fully-committed state. No half-written record is ever exposed.

## [Heading 2] Failure detection, replacement, and safe eject
A missing drive, a read/write error, a checksum mismatch, a SMART warning, or repeated timeouts all count as a failure event, triggering a persistent dashboard warning and an audit entry while RM keeps running on the surviving drive. Plugging in a replacement triggers an automatic, checksum-verified rebuild that can pause and resume across a school day. An explicit 'Eject Mirror Drive' action lets the School Admin safely remove a drive — for example, to rotate one SSD off-site at the end of each term as extra protection against fire or theft.

## [Heading 1] 14. Non-Functional Requirements

## [Heading 2] 14.1 Availability and Offline Operation
All daily operations (marks entry, bio-data, report generation) must function with zero internet connectivity. Internet is only required for installation, payment, software updates, feedback submission, and the owner panel.

## [Heading 2] 14.2 Data Safety
No single point of hardware failure should be able to destroy a school's academic history. This is the reasoning behind the mandatory dual-SSD mirror (see Section 13 above) and, longer-term, the encrypted cloud endpoint (FR-W8).

## [Heading 2] 14.3 Security
Role-based access strictly scoped to assigned classes and subjects (FR-C1)
Credentials stored hashed, never in plain text
Owner panel requires two-factor authentication and is on an unlisted URL (FR-O0)
Institutional data never leaves the school's own hardware unencrypted; cloud snapshots (when enabled) are end-to-end encrypted with a key the product owner does not hold (FR-O4)

## [Heading 2] 14.4 Usability
The primary user of the School Admin role is a headteacher or director-owner with no IT background. Setup should be completable from the onboarding manual (FR-W10) in a few hours without a hired technician.

## [Heading 2] 14.5 Auditability
Every write action by every actor is logged with actor, action, and timestamp (FR-E2), and no learner, teacher, or subject record is ever hard-deleted — only added, dropped, retired, or superseded, with full history retained.

## [Heading 2] 14.6 UI/UX Quality — Premium, Catching, Responsive
A headteacher's buying decision is shaped heavily by a single ~30-minute demo. First impression matters as much as feature completeness. The interface must be:
Premium and catching — a modern, clean visual design; one consistent colour palette (tunable per school via the accent-colour option in FR-B2), one font system, and consistent spacing and iconography throughout. No default-form-builder or dated-Windows look. The Dark/Light theme (FR-B9) must be genuinely designed, not just an inverted colour scheme.
Responsive to screen size — RM must reflow and scale correctly across the range of hardware a school might actually run it on, from a modest 1366×768 laptop to a large office monitor, not just the resolution it happened to be designed on.
Responsive under load — a class list of 800 learners across 10 classes must not freeze the interface; marks entry, search, and report generation should feel instant on the spec's minimum hardware (8 GB RAM). Long-running operations (mirror rebuild, batch report printing, promotion) must run in the background with visible progress, never lock the whole application.

## [Heading 2] 14.7 Uniform Quality Bar Across Modules
Every input method and every module is built to the same standard — there is no 'main' feature that is polished while everything else is an afterthought. Concretely: all three marks-entry methods in FR-C3 (Excel upload, in-app spreadsheet, structured form) must be equally reliable and equally fast, not one primary path with two neglected fallbacks; the same discipline applies across Phase G's modules (Visitor Log, Fees Ledger, Timetable, etc.) relative to the Core Layer. This is a standard a reviewer or a developer joining later can hold any part of the product to, not just a general aspiration.

## [Heading 1] 15. Out of Scope for This Release, and Future Roadmap
Secondary (S1–S6, UCE D1–F9 / UACE A–O/F) and University (Y1–Y4+, GPA) editions are reserved for future releases, built on the same Core Layer, conditional on two things both being true:
Nursery + Primary is running stably in real schools through complete academic years, without major bugs or data loss
Funders or a credible pipeline of paying institutions justify extending the work
Neither future level is promised on a calendar date — the roadmap is condition-bound, not time-bound. A per-teacher mobile mini-app was considered in earlier drafts (V2–V5) and deliberately dropped: the Wi-Fi/tunnel sync it required was judged too fragile for Ugandan school networks to depend on. The Excel-upload path in FR-C3 already covers 'enter marks away from school' without that risk. A phone app can be revisited later if real usage shows a clear, repeated need for it.

## [Heading 2] 15.1 Deliberately Excluded: Payroll Calculation
FR-G4 (Staff HR) covers attendance, leave, and contract records, but deliberately excludes computing payroll — PAYE, NSSF, and payslip generation. Uganda's tax bands and statutory deduction rules change over time, and getting them wrong is a legal and financial liability for the school, not a product bug. RM's position is to integrate with, or export cleanly to, an existing licensed Ugandan payroll provider rather than owning tax compliance in-house.

## [Heading 2] 15.2 Considered for a Later Release
Anonymised/blind marking — exam scripts marked by registration number instead of name, to guard against bias. Valuable for schools that want it, but it needs to be designed into the marks-entry flow from the start rather than retrofitted, so it's a future release rather than a quick add.
Parent portal — a limited, view-only way for a parent to check results and fees balance. To stay consistent with the offline-first model, this is more likely to be delivered via the SMS channel already specified in FR-G8 (e.g. a code a parent can text in) than as a second always-online web system for RM to maintain.

## [Heading 2] 15.3 Secondary Readiness — Known Constraints for a Future Level Pack
Secondary remains explicitly out of scope for this release (Section 4.1). This section exists so that deferral is an informed decision, not one that quietly paints a future Secondary Level Pack into a corner — it records what's structurally different about Secondary, without building any of it now.

## [Heading 3] The core architectural risk: per-student subjects, not per-class subjects
FR-B5 and FR-C13 assume one class has one shared subject list, taken by every student in it — true for Primary, where every P5 learner takes the same subjects. It stops being true partway through Secondary: O-Level (S1–S4) introduces electives, and A-Level (S5–S6) is built entirely around subject combinations (e.g. PCM, HEG, MEG) — two students in the same class, 'S5,' may share almost no subjects. Report cards, class lists, and mark sheets would need to be generated per-student-subject-set rather than per-class-subject-set. This is not a Level Pack swap on top of the existing Primary data model — it's closer to a redesign of the class/subject/enrollment relationship, and should be scoped as such whenever Secondary work actually begins, rather than assumed to be a smaller effort than it is.

## [Heading 3] Grading and assessment structure, not just different bands
UCE (O-Level) aggregate: best 8 subjects, each graded D1–F9, summed — different computation from Primary's straightforward mean, not just a different band label set.
UACE (A-Level): 3 principal plus subsidiary subjects, A–F/O grading, and a points system feeding university placement — a further different structure again.
Uganda's Competency-Based Curriculum (CBC) for lower secondary shifted assessment toward continuous formative assessment alongside end-of-term summative exams. A future Secondary Level Pack built against the current exam-only assumption would already be out of step with what UNEB actually requires — this needs deliberate design, not a preset swap.

## [Heading 3] Other Secondary-specific needs, noted for scoping, not committed
Joint/combined mock exams across several schools, sometimes requiring results import from an external source rather than entry by the school's own teachers.
A house system and prefect records — a grouping that cuts across classes for sports, discipline, and co-curricular activities, distinct from the class/stream structure.
Subject/career guidance tracking, particularly ahead of A-Level combination selection.
A light alumni record — secondary is an exit point to university or employment, and a 'graduated, here's where they ended up' record has real long-term value for a school's own marketing, though it's a nice-to-have rather than a launch requirement.
The Boarding Module (FR-G15) is already specified and feature-gated — Secondary is the segment most likely to actually switch it on, which is worth keeping in mind when boarding eventually gets prioritised.

## [Heading 1] 16. Assessment — What's Overbuilt, and How to Reach Launch Faster
The requirements above are complete and internally consistent — V6 is a genuinely well-thought-through specification. But 'complete' and 'buildable quickly' are different things. Several pieces here are the kind of engineering that a mature, well-funded product team takes on after proving the core loop works, not before. This section is an honest, independent read of where the biggest delivery risk sits, and what to cut, simplify, or defer so the first paying schools can start using RM sooner rather than later.

## [Heading 2] 16.1 The single biggest risk: the dual-SSD mirror
FR-B12 is, in effect, a request to hand-build software RAID-1 — transactional dual writes, checksummed read-back verification, power-loss recovery, failure detection, automatic rebuild, safe eject/reconnect. This is real, hard systems engineering. Storage vendors spend years hardening code like this, and subtle bugs in it don't show up as crashes — they show up as silently corrupted report cards discovered a term later, which is the worst possible failure mode for a product whose entire pitch is 'we protect your data.'
Recommendation: for the pilot and first paying cohort, replace the fully transactional mirror with a much simpler mechanism that still protects against the failure mode that matters most (one drive dying):
Write to one primary local database (on the internal drive or one SSD).
Run a scheduled copy-to-second-drive (e.g. every 15 minutes and on app close), not a real-time checksummed dual write.
Keep the 'take one SSD off-site periodically' habit — it's cheap and doesn't depend on the sophistication of the mirror underneath it.
This ships in days rather than weeks, is far easier to test and trust, and covers the realistic scenario (one drive fails or a PC dies) just as well. Build the fully verified, self-healing mirror once there's a paying customer base whose trust justifies the investment — and once there's been real-world evidence of how RM actually gets used and abused in the field.

## [Heading 2] 16.2 The worldwide grading library is oversized for a Uganda-primary launch
Only two grading systems will actually be used on day one: Nursery Descriptive and UNEB Primary. Shipping working, correctly-thresholded IB, Cambridge, US GPA, and UK GCSE presets is real effort for a market that isn't buying them yet.
Recommendation: ship only the two defaults plus the custom band builder. List the rest as 'coming soon' in the marketing material if useful for future positioning, but don't spend engineering time getting their thresholds right until Secondary/University editions are actually being scoped.

## [Heading 2] 16.3 The owner admin panel is built for a scale you don't have yet
Three full dashboards — revenue with drill-down, a geographic heat map, growth/churn analytics — are things you'll genuinely want once there are 50–100+ paying schools. For the first 20, a spreadsheet or a very plain internal table (institution, plan, amount, date, district) tells you everything these dashboards would, in a fraction of the build time.
Recommendation: defer FR-O1–O3 until subscriber count justifies the investment. Keep FR-O0 (secure single-owner login) and FR-O4 (privacy boundary) as design principles from day one, since they're cheap to bake in early and expensive to retrofit.

## [Heading 2] 16.4 Automated payment + hardware-lock adds risk before there's volume to justify it
Full gateway integration (FR-W4) with automatic credential issuance blocked on payment, plus hardware fingerprinting (FR-B0) that must correctly distinguish 'legitimate machine change' from 'piracy,' is exactly the kind of system that generates support tickets and locked-out headteachers if it's not thoroughly tested — and it's hard to thoroughly test before you have real transactions.
Recommendation: for the pilot phase (MB Primary School plus 2–3 friendly schools), handle payment manually — mobile money to a business number, manual activation-key issuance — and skip hardware fingerprinting entirely (a license key tied to an institution ID is enough for now). Automate both once the manual flow is proven and volume makes the manual work genuinely burdensome.

## [Heading 2] 16.5 Smaller cuts worth making for speed
FR-C14 (late marks entry) — the multi-step request/approve/reverse workflow is sound design, but for v1 a simpler 'School Admin can directly edit a closed-term mark, with a mandatory reason note that's logged' gets the same safety property with far less UI to build.
FR-W10 (onboarding manual + quiz + certificate) — write the manual; skip the quiz and certificate generation until there's time to spare. A manual alone still unblocks a new School Admin.
FR-C11 (streams) — correction from earlier drafts of this assessment: most Ugandan primary schools do run streams, so this should not be treated as a defer-until-asked item. Keep the FR-B7 toggle for the minority of schools without streams, but build the full per-stream logic properly in Phase 1 rather than Phase 2 (see Section 17).
FR-E5 / FR-E6 (multi-year trend dashboards) — genuinely can't produce an interesting chart until a school has used RM for 2+ years. Low priority; build once there's data to show.
FR-B13 (automated software update channel with rollback) — for the first cohort, a support-assisted manual reinstall is fine. Automate once you have enough installed copies that manual updates don't scale.

## [Heading 2] 16.6 What's already right — don't second-guess these
Dropping the mobile mini-app (done in V6) — correct call; Wi-Fi/tunnel sync in rural Uganda would have been an ongoing support nightmare for very little gain over the Excel-upload path.
Two subscription plans instead of four — matches how schools actually budget, and it's less code either way.
The expiry → grace period → read-only model (FR-B11) — protects data, is genuinely not hard to build, and directly prevents the worst outcome (a school losing access to its own records). Build this as specified from day one.
Never-delete for learners and subjects (drop, not delete) — cheap to implement if it's in the data model from the start, and it's the difference between a trustworthy audit trail and none.
The audit log (FR-E2) — should be close to free if it's designed into the data layer up front rather than bolted on later. Don't defer this one.
The fees-block rule (FR-B8) — a simple conditional check at print time; cheap, and it's the kind of feature headteachers will specifically ask about in a sales demo.

## [Heading 2] 16.7 A note on Phase G and scope discipline
Phase G (Section 11) now adds twenty-three modules on top of the original six phases — spanning report/document tooling, fees and expenditure, daily operations (attendance, timetable, calendar), guardian communication (SMS alerts and bulk broadcast), and office administration (ID cards, admissions, correspondence, minutes, the school store, and pass-outs). Individually, each is reasonable and several are genuinely high-value — ID Cards was a founding requirement, and the Fees Ledger, Bulk Print Queue, Onboarding Import, and Pass-Out slip are all things a school will ask about immediately. Collectively, they now represent several times the scope of the original V6 specification, and they push RM decisively toward being a full school-office system rather than a marks-and-reports tool with extras. That tension isn't a mistake to fix — it's a real trade-off to keep visible: RM's competitive edge is depth on the offline, Uganda-primary-specific problem, not breadth. The recommendation stands from Section 16.1–16.5: build the Pilot MVP narrow, prove it works in a real school for a real term, and only then work down the Phase G list in the order given in Section 17 — resist the temptation to build all twenty-three before the first paying school has used the core loop. The Boarding Module (FR-G15) remains the clearest example of a module included for completeness rather than as a suggestion to prioritise it.

## [Heading 1] 17. Recommended Phased Build Plan
A build order re-sequenced around the cuts above, aimed at getting MB Primary School and 2–3 friendly pilot schools onto a working version as fast as possible, then hardening from there.

## [Heading 2] Phase 0 — Pilot MVP
Goal: MB Primary School running one full term on RM.
Login and role-based routing (FR-B1, FR-C1, FR-C2)
Academic calendar, classes, subject catalogue, teacher assignment (FR-B3, FR-B5, FR-B6)
Marks entry, all three methods, built to equal quality (FR-C3)
Daily attendance register (FR-G9) — basic, high-value, and cheap relative to everything else in this phase
Report cards using only the two default grading systems, the Comment Bank (FR-G11), and the Bulk Print Queue (FR-G12) — end-of-term printing for a whole class is a day-one necessity, not a nice-to-have
Onboarding Data Import (FR-G13) — even the pilot school's existing records should be imported, not retyped by hand
The Custom Report Builder half of FR-G1 (skip Template Upload for now)
Student & Staff ID Cards (FR-G16) — a founding requirement, and highly visible in any sales demo
Simple scheduled backup-to-second-drive (simplified FR-B12, see Section 16.1)
Core UI/UX quality bar applied from day one (Section 14.6–14.7) — retrofitting visual polish later is far more expensive than building it in from the start
Manual activation, no payment gateway, no hardware lock

## [Heading 2] Phase 1 — First Paying Cohort (≈20 schools)
Fees Ledger, replacing the manual fees-block flag (FR-G5, FR-B8), and exam permits/class lists/mark sheets/registers (FR-D2)
Promotion ladder, audit log built in from the start (FR-E2, FR-E3)
Bio-data management, add/drop learners, correction messages (FR-C7–C10)
Visitor Log (FR-G2) — self-contained, low risk, good demo material
Admissions Pipeline (FR-G17) — closes the gap between one-time import (FR-G13) and add/drop within a class (FR-C10)
Streams, built in full (FR-C11) — most Ugandan primary schools actually run streams; this was wrongly treated as deferrable in an earlier pass (see Section 16.5). Toggle (FR-B7) stays off by default for the schools that don't need it.
Basic in-app feedback (email/WhatsApp link is enough initially; skip the screenshot tooling)
Simplified late-marks entry (direct edit with mandatory reason, not the full workflow)
Guaranteed full data export (FR-G10) — cheap once other exports exist, and a strong trust-building talking point

## [Heading 2] Phase 2 — Scale-Up (≈100 schools)
Full transactional, checksum-verified dual-SSD mirror (FR-B12 as originally specified)
Automated payment gateway and credential issuance (FR-W2–FR-W5)
Weekly assignments (FR-C12), full late-marks-entry workflow (FR-C14)
Software update channel with rollback (FR-B13)
Timetable (FR-G6) and the Exams & Academic Calendar (FR-G7)
Staff HR — attendance, leave, contracts, explicitly excluding payroll (FR-G4)
SMS notifications for fees reminders and results-ready alerts (FR-G8)
Bulk SMS Broadcast to guardians, with cost estimate and scheduling (FR-G23) — built right alongside FR-G8 since it reuses the same gateway integration
Student Pass-Out / Exit Pass with guardian SMS (FR-G22) — now that FR-G8 and the Visitor Log (FR-G2) both exist
Petty Cash and Expenditure Log (FR-G20) — completes the income/expenditure picture alongside the Fees Ledger
School Store and Uniform Tracking (FR-G21)
Discipline and Conduct Log (FR-G14) — pairs naturally with the Comment Bank and SMS channel, both already built by this phase
Template Upload, the harder half of the Document Generator, plus letters and certificates (rest of FR-G1)

## [Heading 2] Phase 3 — Mature Product
Owner admin panel — revenue, geography, growth/churn dashboards (FR-O1–O3)
Multi-year trend analytics (FR-E5, FR-E6)
Worldwide grading library expansion, hardware lock, Connector-RM, encrypted cloud backup (FR-W7, FR-W8, FR-B0)
Onboarding quiz and certificate (FR-W10 in full)
School Overview Dashboard (FR-G3) — deliberately last, since it only displays data that Phases 0–2 must already have produced
Boarding Module (FR-G15) — build only if and when an actual boarding-school customer signs up; costs nothing to leave un-built until then
Correspondence Log (FR-G18) and Meeting Minutes (FR-G19) — low-priority office utilities, self-contained and low risk whenever they're picked up
Roadmap items from Section 15: anonymised marking, SMS-based parent portal, payroll-provider integration

## [Heading 1] Appendix A — Original Raw Requirements Brief (Verbatim)
This is the founder's original, unedited description of the system, preserved in full as the source document from which every version of the Functional Requirements Specification — and this consolidated SRS — was developed.

## [Heading 2] Teachers
Input student marks by uploading an excel sheet or typing the marks directly in the Results Manager inbuilt excel sheet or using a form.
Then also it should allow entering results using a simple form using a smart phone, where the form shows the name of the learner and class, then a teacher enters the marks after which it is copied to desktop Results Manager and it automatically enters the marks — this form has to be created by the Results Manager itself then sent via WhatsApp or Email so that the teacher enters the marks. This isn't an ordinary form with over 100 learners on a page or several pages; this is a small application running on every teacher's smartphone, where you can press Next after entering each student's marks for all the subjects, or Previous if you think you missed something. This mini application can also provide the entered results, both partial and full marks, in an excel-sheet form but not editable in the sheet form to avoid mistakes — instead it takes you to the form if you want to make any changes.
Teachers log in using credentials assigned by the School Admin; teachers can only access the classes and subjects assigned to them, to avoid entering marks in the wrong class or subject. If a teacher selects a class like Form 4, Results Manager requires their assigned credentials, and grants access only to the specific class and subject assigned; wrong credentials mean no access, and the teacher is advised to contact the School Admin for help.
When a teacher selects a class, they can log in as a Subject Teacher or a Class Teacher. Logged in as Subject Teacher, they see graphs showing subject performance and pie-charts assessing student progress that term.
A teacher logged in as Class Teacher can set a deadline for all Subject Teachers to enter marks, shown on their dashboard as a percentage of results entered by each Subject Teacher.
The Class Teacher dashboard has a search box to find a student by name or registration number; the registration-number format is decided by the School Admin, but Results Manager ensures every learner across the institution has a unique registration number.
The Class Teacher can change a student's bio-data — name, photo, gender, LIN — and upload a new photo for a new or existing student. Subject Teachers can forward 'Correction Messages' to the Class Teacher if they spot wrong bio-data the Class Teacher missed.
The Class Teacher's credentials are the same ones used elsewhere, interpreted differently by context — the School Admin assigns these roles. A maximum of two teachers can be assigned as Class Teachers for a class (the Class Teacher and an Assistant Class Teacher), each with different login credentials, so Results Manager can show in the Admin dashboard which changes were made by whom — Results Manager tracks all activity and time, saved to the School Admin dashboard.
The Class Teacher's dashboard allows adding or dropping learners easily — DROP, not DELETE. Each class has a folder tracking both added and dropped learners, so Results Manager can show, from the School Admin dashboard, which classes gained or lost the most learners, ranked by percentage in descending order for both gains and losses.
RM lets the Class Teacher print report cards — both partial (e.g. a MID report card mid-term, if the School Admin has configured MID and EOT sets) and at end of term/semester — as well as examination permits, class lists, mark lists, examination registers, and mark sheets.
The Class Teacher can add up to a maximum of 20 Streams per class; each stream gets a Stream Class Teacher and Assistant, assigned by the School Admin, operating the same way a Class Teacher does for a class without streams. Each stream has its own add/drop folder reflected in the School Admin's dashboard, showing class and stream together.
Finally, the Class Teacher can promote learners to the next term during the academic year, and to the next class at year end — Results Manager tracks academic-year progress automatically and promotes classes without distorting any learner's information.

## [Heading 2] School Admin
The School Admin assigns classes and streams to Class Teachers, Stream Class Teachers, and their assistants, and assigns the remaining teachers to their classes and subjects.
The School Admin specifies how many terms/semesters make up an academic year, and how many examinations happen per term/semester (e.g. BOT, MID, EOT), so teachers know exactly where to enter marks.
RM shows a login frame and grants access to the correct dashboard — School Admin, Teacher, or Tech Admin — based on the credentials entered.
The School Admin enters the school/institution name so RM customises the entire application to show it is owned by that institution.
The School Admin can disable features such as photos or exam permits if the institution doesn't need them.
The School Admin can block students who haven't completed fees payment, if the school requires it, so their exam permits and report cards are not printed.
RM supports a maximum of 3 School Admins, added either by the first School Admin (created immediately after installation by the Tech Admin) or by the Tech Admin directly after installation.
The School Admin can select which theme (Dark, Light, etc.) to use from their dashboard.

## [Heading 2] Tech Admin
Immediately after installation, a Tech Sign-Up frame appears requiring the technician to enter specific credentials to start the application — not just any credentials will work. RM requires payment details (Visa card or PayPal) to subscribe first; the application will not start until payment is complete. [Original note: 'claude code must add the standard online services that are secure and then show me how I link them to my account.'] After payment, RM generates login credentials using a specific algorithm — credentials are generated only after payment is processed.
[Original note: 'I want RM to have a subscription system like the one Claude uses, not specifically using Stripe since I am in Uganda — you can clarify and give the best option and services that can do it in Uganda.']
The Tech Admin dashboard stores all the data RM works on for up to 6 months; uploads happen weekly or daily depending on what the Tech Admin selects, so that if there's a hack and RM goes down, the most recent upload can be restored and RM is back to normal.
The Tech Admin can set credentials for the 3 School Admins who operate RM, or set up just one and let that admin add the remaining two.

## [Heading 2] Note on how this brief evolved into the specification above
Two ideas in the original brief were deliberately changed during the V1–V6 process, for reasons worth keeping visible:
The per-teacher smartphone mini-app (with WhatsApp/email-distributed sync) was replaced by the existing Excel-upload path in FR-C3. The sync mechanism it depended on was judged too fragile for Ugandan school Wi-Fi to depend on.
The 'Tech Admin' role in the original brief — a single installer-level account gating the app behind payment, holding a rolling 6-month backup — evolved into two separate things in the specification: the website's payment/subscription flow (Phase A) and the single-owner developer admin panel (Phase F). This split keeps subscription mechanics on the website (where it belongs, since it needs internet) separate from anything running on a school's offline PC.