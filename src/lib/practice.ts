/**
 * Fills the practice school through the same commands a real school uses:
 * staff with offices, learners in every class, a fee structure with
 * payments, bursaries and a promise, marks for the current term,
 * requirements, spending and today's registers.
 *
 * Everything is made up. Parents' phone numbers are 0700 0xx xxx and the
 * school is offline only, so no SMS can leave. The same names come out every
 * time, so a trainer can say "find Akello Brian in P5" and it is there.
 */

import { api } from "./api";
import type { AcademicYearRow, AttendanceState, ClassRow, ExamRow, PaymentMethod } from "./types";

export const PRACTICE_SCHOOL = "Mwenge Practice Primary School";
export const PRACTICE_PASSWORD = "Practice#2026";
export const PRACTICE_PIN = "4826";

/** Who can sign in to the practice school, and what each one shows. */
export const PRACTICE_LOGINS: { username: string; who: string }[] = [
  { username: "admin", who: "School Admin: everything, including settings and backups" },
  { username: "head", who: "Headteacher: the whole school, fees and conduct, no marks editing" },
  { username: "dos", who: "Director of Studies: marks, timetables, insights" },
  { username: "secretary", who: "Secretary: learners, gate, visitors, messages" },
  { username: "bursar", who: "Bursar: fees, expenses, the Day Book's money" },
  { username: "nakato", who: "Class Teacher of P7" },
  { username: "okello", who: "Class Teacher of P6" },
  { username: "atim", who: "Class Teacher of P5" },
];

const BOYS = ["Brian", "Ivan", "Joel", "Mark", "Daniel", "Isaac", "Samuel", "Allan", "Edwin", "Ronald", "Moses", "Elijah", "Paul", "Henry"];
const GIRLS = ["Sarah", "Grace", "Esther", "Patricia", "Joan", "Ruth", "Mercy", "Faith", "Doreen", "Shamim", "Agnes", "Hadija", "Brenda", "Prossy"];
const SURNAMES = ["Nakato", "Okello", "Mugisha", "Atim", "Ssemwogerere", "Nambi", "Opio", "Kyomuhendo", "Tumusiime", "Akello", "Byaruhanga",
  "Namukasa", "Kato", "Achieng", "Musoke", "Nalubega", "Tumwine", "Ochieng", "Asiimwe", "Kisakye", "Waiswa", "Nabirye", "Odongo", "Kirabo"];

/** A small, repeatable random sequence, so the practice school is the same every time. */
function rng(seed: number) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 2 ** 32;
  };
}

const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

export type Progress = (step: string, done: number, total: number) => void;

export async function buildPracticeSchool(progress: Progress): Promise<void> {
  const rand = rng(2026);
  const pick = <T,>(xs: T[]) => xs[Math.floor(rand() * xs.length)]!;
  const today = new Date();
  const year = today.getFullYear();
  const todayIso = iso(today);
  const TOTAL = 9;

  // 1. The school itself.
  progress("Setting up the school", 1, TOTAL);
  const defaults = await api.setupDefaults();
  const terms = [
    { name: "Term 1", startDate: `${year}-02-02`, endDate: `${year}-04-24` },
    { name: "Term 2", startDate: `${year}-05-25`, endDate: `${year}-08-14` },
    { name: "Term 3", startDate: `${year}-09-14`, endDate: `${year}-12-04` },
  ];
  await api.completeSetup({
    institutionName: PRACTICE_SCHOOL,
    motto: "Learning for life (practice school)",
    address: "Plot 1, Practice Road, Kampala",
    phone: "0700 000 000",
    email: null,
    accentColor: "#2A6B3F",
    regNumberPattern: defaults.regNumberPattern,
    adminFullName: "Practice Admin",
    adminUsername: "admin",
    adminPassword: PRACTICE_PASSWORD,
    adminEmail: null,
    academicYearLabel: String(year),
    classCodes: defaults.classes.map((c) => c.code),
    terms,
    exams: [
      { code: "BOT", name: "Beginning of Term", weight: 0, isFinal: false },
      { code: "MID", name: "Mid Term", weight: 0.4, isFinal: false },
      { code: "EOT", name: "End of Term", weight: 0.6, isFinal: true },
    ],
  });
  const admin = await api.signIn("admin", PRACTICE_PASSWORD);
  if (admin.mustChangePassword) await api.changePassword(PRACTICE_PASSWORD, PRACTICE_PASSWORD + "!").catch(() => undefined);
  await api.beginPracticeSchool();

  const years: AcademicYearRow[] = await api.listAcademicYears();
  const termRows = years[0]!.terms.slice().sort((a, b) => a.seq - b.seq);
  // The term the calendar is in now (or the last one that has started).
  let current = 0;
  terms.forEach((t, i) => {
    if (todayIso >= t.startDate) current = i;
  });
  const classes: ClassRow[] = (await api.listClasses()).sort((a, b) => a.ladderPosition - b.ladderPosition);

  // 2. Staff, with offices and classes.
  progress("Adding staff", 2, TOTAL);
  const staff: { username: string; id: string; initial: string }[] = [];
  const people: [string, string, "head_teacher" | "director_of_studies" | "secretary" | "bursar" | null][] = [
    ["head", "Sarah Kyomuhendo", "head_teacher"],
    ["dos", "Peter Tumwine", "director_of_studies"],
    ["secretary", "Amina Nansubuga", "secretary"],
    ["bursar", "John Mugisha", "bursar"],
    ["nakato", "Grace Nakato", null],
    ["okello", "Daniel Okello", null],
    ["atim", "Joan Atim", null],
  ];
  for (const [i, [username, name, office]] of people.entries()) {
    const r = await api.createStaff({ fullName: name, username, role: "teacher", phone: `0700 0001 ${String(i).padStart(2, "0")}` });
    if (office) await api.setStaffOffice(r.id, office);
    staff.push({ username, id: r.id, initial: r.initialPassword });
  }
  const byCode = (code: string) => classes.find((c) => c.code === code);
  for (const [username, code] of [["nakato", "P7"], ["okello", "P6"], ["atim", "P5"]] as const) {
    const cls = byCode(code);
    const who = staff.find((s) => s.username === username);
    if (cls && who) await api.assignTeacher({ userId: who.id, classId: cls.id, role: "class_teacher" });
  }

  // 3. Learners: eight in every class.
  progress("Admitting learners", 3, TOTAL);
  const learners: { id: string; name: string; cls: ClassRow }[] = [];
  let n = 0;
  for (const cls of classes) {
    for (let i = 0; i < 8; i++) {
      n += 1;
      const girl = n % 2 === 0;
      const surname = pick(SURNAMES);
      const first = girl ? pick(GIRLS) : pick(BOYS);
      const age = 3 + cls.ladderPosition;
      const r = await api.saveStudent({
        fullName: `${surname} ${first}`,
        gender: girl ? "F" : "M",
        dateOfBirth: `${year - age}-${String(1 + (n % 12)).padStart(2, "0")}-${String(1 + (n % 27)).padStart(2, "0")}`,
        // A few learners deliberately lack a LIN, so the PLE and EMIS checks have something to find.
        lin: cls.levelKind === "primary" && n % 6 !== 0 ? `PR${year}${String(100000 + n * 37).slice(-6)}` : null,
        guardianName: `${girl ? "Mrs" : "Mr"} ${surname}`,
        guardianPhone: `0700 0${String(10000 + n).slice(-5).replace(/(\d{2})(\d{3})/, "$1 $2")}`,
        guardianRelationship: "Parent",
        classId: cls.id,
      });
      learners.push({ id: r.id, name: `${surname} ${first}`, cls });
    }
  }

  // 4. Fees for every term up to now, with payments dated inside each term.
  progress("Setting fees and taking payments", 4, TOTAL);
  const tuition = await api.saveFeeItem({ id: null, name: "Tuition", kind: "termly", position: null });
  const development = await api.saveFeeItem({ id: null, name: "Development", kind: "termly", position: null });
  const lunch = await api.saveFeeItem({ id: null, name: "Lunch", kind: "optional", position: null });
  const transport = await api.saveFeeItem({ id: null, name: "Transport", kind: "optional", position: null });
  const methods: PaymentMethod[] = ["cash", "mobile_money", "cash", "bank", "schoolpay", "mobile_money"];
  for (let t = 0; t <= current; t++) {
    const term = termRows[t]!;
    await api.openTerm(term.id);
    await api.saveFeeRates(
      term.id,
      classes.flatMap((c) => [
        { feeItemId: tuition.id, classId: c.id, amount: 250000 + 15000 * c.ladderPosition },
        { feeItemId: development.id, classId: c.id, amount: 50000 },
        { feeItemId: lunch.id, classId: c.id, amount: 120000 },
        { feeItemId: transport.id, classId: c.id, amount: 150000 },
      ]),
    );
    await api.setFeeOptins(term.id, lunch.id, learners.filter((_, i) => i % 3 !== 0).map((l) => l.id), true);
    await api.setFeeOptins(term.id, transport.id, learners.filter((_, i) => i % 5 === 1).map((l) => l.id), true);
    const start = new Date(`${terms[t]!.startDate}T08:00:00`);
    const end = new Date(Math.min(new Date(`${terms[t]!.endDate}T08:00:00`).getTime(), today.getTime()));
    const span = Math.max(1, Math.floor((end.getTime() - start.getTime()) / 86400000));
    // Pay against each learner's real bill: past terms are mostly cleared,
    // this term is part-way, as in a real school mid-term.
    const accounts = await api.listFeeAccounts(term.id, null);
    for (const [i, row] of accounts.rows.entries()) {
      if (row.balance <= 0) continue;
      const roll = rand();
      const share = t < current ? (roll < 0.8 ? 1 : roll < 0.95 ? 0.5 : 0) : roll < 0.25 ? 1 : roll < 0.7 ? 0.5 : 0;
      const total = Math.floor((row.balance * share) / 5000) * 5000;
      if (total <= 0) continue;
      // A big bill is often paid in two instalments.
      const parts = total > 300000 && rand() < 0.5 ? [Math.floor(total / 2 / 5000) * 5000, total - Math.floor(total / 2 / 5000) * 5000] : [total];
      for (const [k, amount] of parts.entries()) {
        const method = methods[(i + k) % methods.length]!;
        const paid = new Date(start.getTime() + Math.floor(rand() * (span + 1)) * 86400000);
        let paidOn = t === current && i % 9 === 0 && k === parts.length - 1 ? todayIso : iso(paid);
        if (paidOn > todayIso) paidOn = todayIso;
        await api
          .recordFeePayment({
            studentId: row.studentId,
            amount,
            method,
            reference: method === "cash" ? null : `${method.slice(0, 2).toUpperCase()}${Math.floor(10_000_000 + rand() * 89_999_999)}`,
            paidBy: null,
            payerPhone: null,
            paidOn,
            note: null,
            textGuardian: false,
          })
          .catch(() => undefined);
      }
    }
  }
  const at = (i: number) => learners[i % learners.length]!;
  await api.addFeeAdjustment({ studentIds: [at(20).id, at(41).id], termId: null, feeItemId: null, kind: "bursary", amount: null, percent: 50, reason: "Staff child" });
  await api.addFeeAdjustment({ studentIds: [at(23).id], termId: null, feeItemId: null, kind: "discount", amount: 30000, percent: null, reason: "Third child in the school" });
  await api.addFeeAdjustment({ studentIds: [at(45).id], termId: null, feeItemId: null, kind: "charge", amount: 45000, percent: null, reason: "Educational trip to Entebbe Zoo" });
  const due = new Date(today.getTime() + 14 * 86400000);
  await api.addFeePromise({ studentId: at(30).id, amount: 200000, dueOn: iso(due), promisedBy: `Mrs ${at(30).name.split(" ")[0]}`, note: "After the coffee harvest" }).catch(() => undefined);

  // 5. Marks for this term's mid-term and end-of-term, in every primary class.
  progress("Entering marks", 5, TOTAL);
  const exams: ExamRow[] = termRows[current]!.exams.filter((e) => e.code === "MID" || e.code === "EOT");
  for (const cls of classes.filter((c) => c.levelKind === "primary")) {
    const subjects = await api.listClassSubjects(cls.id);
    const mine = learners.filter((l) => l.cls.id === cls.id);
    for (const subject of subjects) {
      for (const exam of exams) {
        const entries = mine.map((l, i) => {
          const talent = (l.name.length * 7 + i * 13) % 40;
          const score = Math.max(5, Math.min(subject.maxScore, Math.round(((40 + talent + Math.floor(rand() * 27) - 12) / 100) * subject.maxScore)));
          return { studentId: l.id, score, isAbsent: false };
        });
        await api.saveMarks(subject.id, exam.id, entries).catch(() => undefined);
      }
    }
  }

  // 6. What learners bring this term.
  progress("Setting requirements", 6, TOTAL);
  for (const item of [
    { name: "Ream of paper", quantity: 1, cashValue: 15000 },
    { name: "Toilet paper", quantity: 2, cashValue: null },
    { name: "Broom", quantity: 1, cashValue: 5000 },
  ]) {
    await api.saveRequirementItem({ id: null, termId: termRows[current]!.id, classId: null, name: item.name, quantity: item.quantity, cashValue: item.cashValue });
  }

  // 7. Some spending this month.
  progress("Recording spending", 7, TOTAL);
  for (const [category, description, party, amount] of [
    ["Food and kitchen", "10 bags of maize flour", "Kalerwe market", 450000],
    ["Power and water", "Electricity tokens", "UMEME", 120000],
    ["Stationery and printing", "Exam papers printing", "Kampala Printers", 180000],
  ] as const) {
    await api
      .recordCashEntry({ kind: "expense", onDate: todayIso, category, description, party, amount, method: "cash", reference: null })
      .catch(() => undefined);
  }

  // 8. Today's registers for the upper classes, on a school day.
  progress("Marking today's registers", 8, TOTAL);
  const day = today.getDay();
  if (day !== 0 && day !== 6) {
    for (const cls of classes.filter((c) => c.ladderPosition >= 8)) {
      const entries = learners
        .filter((l) => l.cls.id === cls.id)
        .map((l, i) => ({ studentId: l.id, state: (i === 3 ? "absent" : i === 5 ? "late" : "present") as AttendanceState, note: null }));
      await api.saveRegister({ classId: cls.id, streamId: null, onDate: todayIso, entries, notifyAbsent: false }).catch(() => undefined);
    }
  }
  await api.saveStaffRegisterSettings({ enabled: true, startTime: "07:30" });

  // 9. Everyone gets the same known password and PIN.
  progress("Giving everyone the practice password", 9, TOTAL);
  for (const s of staff) {
    await api.signIn(s.username, s.initial);
    await api.changePassword(s.initial, PRACTICE_PASSWORD);
    await api.setMySignInPin(PRACTICE_PIN).catch(() => undefined);
  }
  await api.signOut();
}
