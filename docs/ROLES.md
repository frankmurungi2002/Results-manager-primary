# Roles and permissions

Who may do what in Phantom School Manager. **The table below is generated from
the code** (`permissions_for` in `src-tauri/src/security/session.rs`), and a
test fails if this file and the code ever disagree. To change the policy,
change the code; the test prints the new table to paste here.

## How it works

Every person has one **account**: School Admin or Teacher. A Teacher may also
hold one **office**: Headteacher, Director of Studies, Secretary or Bursar.
An office adds to an account; it does not replace it. A headteacher who also
teaches P7 is a Teacher, Class Teacher of P7, *and* the Headteacher.

Teaching powers come from **assignments**, not from the table. A Class Teacher
manages their own class (register, roster, comments, every subject's marks).
A Subject Teacher enters marks for their own subject in their own class.
Nothing below is needed for that.

Every command in the backend names the permission it needs, and the interface
only offers what the signed-in person's permissions allow. The backend checks
again on every call, so a screen showing a button is never what grants it.

## The table

| Permission | School Admin | Headteacher | Director of Studies | Secretary | Bursar | Teacher |
|---|---|---|---|---|---|---|
| School identity, logo, optional features, grading systems, SMS and report settings, the stamp | ✓ | | | | | |
| Backups, and restoring one | ✓ | | | | | |
| Staff accounts, passwords and teaching assignments | ✓ | ✓ | | | | |
| The activity log | ✓ | ✓ | | | | |
| Opening and closing terms | ✓ | ✓ | ✓ | | | |
| Classes, subjects, streams, timetables and exam timetables | ✓ | ✓ | ✓ | | | |
| Every learner's record and class list (no marks) | ✓ | ✓ | ✓ | ✓ | ✓ | |
| Every class's marks, attendance and report cards, to read and print | ✓ | ✓ | ✓ | ✓ | | |
| A Class Teacher's powers in every class | ✓ | ✓ | ✓ | | | |
| Entering or correcting marks in any class | ✓ | | ✓ | | | |
| Admitting, editing, transferring and importing learners | ✓ | ✓ | | ✓ | | |
| Pass-outs for a learner in any class | ✓ | ✓ | | ✓ | | |
| Fee structure, bursaries, taking payments, receipts and invoices, the fees block | ✓ | ✓ | | | ✓ | |
| Fee balances, collections and the Day Book's money figures | ✓ | ✓ | | | ✓ | |
| Cancelling a receipt someone else issued | ✓ | ✓ | | | | |
| Bulk SMS to any part of the school, and learner groups | ✓ | ✓ | | ✓ | ✓ | |
| The headteacher's comment on report cards | ✓ | ✓ | | | | |
| The PLE projection and subject heat map | ✓ | ✓ | ✓ | | | |

## Rules that are not in the table

- **Nobody can take over an account more powerful than their own.**
  Resetting a password, retiring an account, creating one or changing its
  office is allowed only when the target's permissions are a subset of the
  actor's. A Headteacher cannot reset the School Admin's password (and then
  sign in as them). For the same reason a Headteacher cannot reset the
  Director of Studies' password: the DOS can correct marks and the
  Headteacher cannot. The School Admin can do both.
- **Managing every class is not editing its marks.** The Headteacher has a
  Class Teacher's powers everywhere but cannot change a mark. Marks for a
  whole class are written only by that class's own Class Teacher (by
  assignment) or by someone with "Entering or correcting marks in any class".
- **A Class Teacher cannot write the headteacher's comment.** The same save
  carries both comments; what changed decides which permission is needed.
- **One Headteacher at a time.** Two would each sign every report card.
- **Nobody changes their own office.** Another School Admin does it.
- **A school can still have at most three School Admins** (FR-B4).

## Each office's screen

Each person's dashboard opens with their own desk and the two actions they
use most:

| Who | Desk shows | First actions |
|---|---|---|
| Headteacher | marks entered this term, registers taken today, learners out on pass-outs, the fees block, the classes furthest behind with marks | Messages, Print reports |
| Director of Studies | marks entered this term, registers taken today, the classes furthest behind with marks | Timetables, Marks |
| Secretary | registers taken today, learners out on pass-outs | Add learner, Issue pass-out |
| Bursar | the fees block | Text parents, Fees block |
| Class Teacher | their own registers still to take today | Enter marks, Take the register |
