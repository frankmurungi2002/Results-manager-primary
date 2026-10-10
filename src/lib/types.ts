/**
 * TypeScript mirrors of the shapes the Rust backend sends.
 *
 * These are hand-kept rather than generated: the surface is small, and an
 * explicit file is easier to read against the SRS than a generated one. When a
 * Rust struct changes, change it here too — `npm run typecheck` will then point
 * at every screen that needs updating.
 */

export type Role = "school_admin" | "teacher";
export type TeacherMode = "subject_teacher" | "class_teacher";

/** An office some staff hold on top of their account. */
export type Office = "head_teacher" | "director_of_studies" | "secretary" | "bursar";

export const OFFICE_TITLES: Record<Office, string> = {
  head_teacher: "Headteacher",
  director_of_studies: "Director of Studies",
  secretary: "Secretary",
  bursar: "Bursar",
};

/**
 * What a person may do across the whole school. Mirrors `Permission` in
 * `security/session.rs`, which decides; the interface only uses these to
 * choose what to show. Class and Subject Teachers need none of them.
 */
export type Permission =
  | "manage_school"
  | "manage_backups"
  | "manage_staff"
  | "view_audit"
  | "manage_calendar"
  | "manage_academics"
  | "view_all_learners"
  | "view_all_classes"
  | "manage_all_classes"
  | "edit_all_marks"
  | "manage_learners"
  | "issue_pass_outs"
  | "manage_fees"
  | "view_finance"
  | "void_receipts"
  | "send_messages"
  | "head_teacher_comment"
  | "view_insights";

export interface SessionView {
  userId: string;
  username: string;
  fullName: string;
  role: Role;
  office: Office | null;
  permissions: Permission[];
  mode: TeacherMode | null;
  mustChangePassword: boolean;
  /** The account that runs the system (FR-B4). Never use it to decide access. */
  isAdmin: boolean;
  canSwitchToClassTeacher: boolean;
  classIds: string[];
  classTeacherOf: string[];
  signedInAt: string;
}

export interface StartupState {
  provisioned: boolean;
  institutionName: string | null;
  session: SessionView | null;
  appVersion: string;
}

/** The error shape every failed command produces. */
export interface AppError {
  code:
    | "validation"
    | "not_found"
    | "conflict"
    | "unauthenticated"
    | "forbidden"
    | "bad_credentials"
    | "account_locked"
    | "not_provisioned"
    | "database"
    | "io"
    | "spreadsheet"
    | "internal";
  message: string;
}

// --- Setup -----------------------------------------------------------------

export interface SetupDefaults {
  classes: DefaultClass[];
  subjects: DefaultSubject[];
  terms: string[];
  exams: DefaultExam[];
  suggestedYearLabel: string;
  regNumberPattern: string;
}

export interface DefaultClass {
  code: string;
  name: string;
  levelKind: "nursery" | "primary";
  ladderPosition: number;
  gradingSystemId: string;
}

export interface DefaultSubject {
  code: string;
  name: string;
  isCore: boolean;
  levels: string[];
}

export interface DefaultExam {
  code: string;
  name: string;
  weight: number;
  isFinal: boolean;
}

export interface SetupRequest {
  institutionName: string;
  motto?: string | null;
  address?: string | null;
  phone?: string | null;
  email?: string | null;
  accentColor: string;
  regNumberPattern: string;
  adminFullName: string;
  adminUsername: string;
  adminPassword: string;
  adminEmail?: string | null;
  academicYearLabel: string;
  classCodes: string[];
  terms: { name: string; startDate?: string | null; endDate?: string | null }[];
  exams: { code: string; name: string; weight: number; isFinal: boolean }[];
}

export interface SetupResult {
  institutionName: string;
  adminUsername: string;
  classesCreated: number;
  subjectsCreated: number;
  termsCreated: number;
}

// --- Institution -----------------------------------------------------------

export interface Institution {
  name: string;
  motto: string | null;
  hasLogo: boolean;
  accentColor: string;
  address: string | null;
  phone: string | null;
  email: string | null;
  regNumberPattern: string;
  setupCompleted: boolean;
}

// --- Academic structure ----------------------------------------------------

export interface ClassRow {
  id: string;
  code: string;
  name: string;
  levelKind: string;
  ladderPosition: number;
  defaultGradingSystemId: string | null;
  defaultGradingSystemName: string | null;
  status: string;
  learnerCount: number;
  subjectCount: number;
  classTeacherName: string | null;
}

export interface SubjectRow {
  id: string;
  code: string;
  name: string;
  maxScore: number;
  passMark: number;
  status: string;
  usedByClasses: number;
}

export interface ClassSubjectRow {
  id: string;
  classId: string;
  subjectId: string;
  catalogueName: string;
  displayName: string;
  code: string;
  maxScore: number;
  passMark: number;
  isCore: boolean;
  position: number;
  status: string;
  gradingSystemId: string;
  gradingSystemName: string;
  gradingOverridden: boolean;
  teacherName: string | null;
}

export interface GradingBand {
  id: string;
  label: string;
  lower_bound: number;
  upper_bound: number;
  points: number | null;
  remark: string | null;
  position: number;
}

export interface GradingSystem {
  id: string;
  code: string;
  name: string;
  description: string | null;
  kind: "numeric" | "descriptive" | "letter" | "percentage";
  is_builtin: boolean;
  is_editable: boolean;
  bands: GradingBand[];
}

export interface ExamRow {
  id: string;
  termId: string;
  seq: number;
  code: string;
  name: string;
  weight: number;
  isFinal: boolean;
  scheduledDate: string | null;
  status: string;
}

export interface TermRow {
  id: string;
  academicYearId: string;
  seq: number;
  name: string;
  startDate: string | null;
  endDate: string | null;
  status: string;
  exams: ExamRow[];
}

export interface AcademicYearRow {
  id: string;
  label: string;
  startDate: string | null;
  endDate: string | null;
  status: string;
  terms: TermRow[];
}

export interface FeatureFlags {
  studentPhotos: boolean;
  examPermits: boolean;
  streams: boolean;
  weeklyAssignments: boolean;
}

// --- Staff -----------------------------------------------------------------

export interface UserSummary {
  id: string;
  username: string;
  fullName: string;
  email: string | null;
  phone: string | null;
  role: Role;
  office: Office | null;
  status: string;
  mustChangePassword: boolean;
  lastLoginAt: string | null;
}

export interface CreateStaffResult {
  id: string;
  username: string;
  initialPassword: string;
}

export interface AssignmentRow {
  id: string;
  userId: string;
  teacherName: string;
  classId: string;
  className: string;
  streamId: string | null;
  streamName: string | null;
  subjectId: string | null;
  subjectName: string | null;
  role: "class_teacher" | "assistant_class_teacher" | "subject_teacher";
}

// --- Learners --------------------------------------------------------------

export interface StudentRow {
  id: string;
  regNumber: string;
  fullName: string;
  gender: string | null;
  dateOfBirth: string | null;
  lin: string | null;
  hasPhoto: boolean;
  guardianName: string | null;
  guardianPhone: string | null;
  guardianRelationship: string | null;
  status: string;
  classId: string | null;
  className: string | null;
  streamId: string | null;
  streamName: string | null;
  enrollmentStatus: string | null;
}

// --- Marks -----------------------------------------------------------------

export interface MarksRow {
  studentId: string;
  regNumber: string;
  fullName: string;
  score: number | null;
  isAbsent: boolean;
  gradeLabel: string | null;
  enteredBy: string | null;
  updatedAt: string | null;
}

export interface MarksSheet {
  classId: string;
  className: string;
  classSubjectId: string;
  subjectName: string;
  examId: string;
  examName: string;
  termName: string;
  maxScore: number;
  passMark: number;
  gradingSystemId: string;
  gradingSystemName: string;
  isOpen: boolean;
  rows: MarksRow[];
}

export interface MarkEntry {
  studentId: string;
  score: number | null;
  isAbsent: boolean;
}

export interface RejectedMark {
  studentId: string;
  studentName: string;
  reason: string;
}

export interface SaveMarksResult {
  saved: number;
  skipped: RejectedMark[];
  warnings: MarkWarning[];
}

export interface SubjectProgress {
  classSubjectId: string;
  subjectName: string;
  teacherName: string | null;
  expected: number;
  entered: number;
  percent: number;
}

export interface GradeCount {
  label: string;
  count: number;
}

export interface SubjectAnalytics {
  subjectName: string;
  examName: string;
  entered: number;
  absent: number;
  meanPercentage: number | null;
  highest: number | null;
  lowest: number | null;
  passRate: number | null;
  distribution: GradeCount[];
}

// --- Documents (FR-D4) -----------------------------------------------------

export interface Branding {
  institutionName: string;
  motto: string | null;
  logoDataUrl: string | null;
  accentColor: string;
  address: string | null;
  phone: string | null;
  email: string | null;
  /** Cleaned ink on transparent PNG, or null when absent or printing is off. */
  headSignatureDataUrl: string | null;
  stampDataUrl: string | null;
}

export interface Footer {
  generatedBy: string;
  printedOn: string;
  appVersion: string;
}

export interface DocumentEnvelope<T> {
  branding: Branding;
  footer: Footer;
  documentKind: string;
  body: T;
}

export interface ReportCardSubject {
  classSubjectId: string;
  name: string;
  isCore: boolean;
  maxScore: number;
  examScores: (number | null)[];
  examGrades: (string | null)[];
  examPoints: (number | null)[];
  position: number | null;
  termScore: number | null;
  gradeLabel: string;
  points: number | null;
  remark: string | null;
  teacherName: string | null;
}

export interface ReportCard {
  studentId: string;
  fullName: string;
  regNumber: string;
  gender: string | null;
  dateOfBirth: string | null;
  photoDataUrl: string | null;
  className: string;
  streamName: string | null;
  subjects: ReportCardSubject[];
  totalPoints: number | null;
  meanPercentage: number | null;
  division: string | null;
  position: number | null;
  classSize: number;
  daysPresent: number | null;
  daysPossible: number | null;
  classTeacherComment: string | null;
  headTeacherComment: string | null;
  /** Nursery: the "Behaviour and cleanliness" line. */
  conductComment: string | null;
  classTeacherName: string | null;
  /** Whose signature prints beside the Class Teacher's comment. */
  classTeacherId: string | null;
  totalScore: number | null;
  totalMax: number;
  examTotals: ExamTotal[];
  previous: PreviousTerm | null;
  weekly: WeeklySummary[];
  activities: ActivityRating[];
  /** What the learner owes, when the school prints balances on cards. */
  feesBalance: number | null;
  /** Proves the card is genuine: typed into Phantom or scanned. */
  verificationCode: string | null;
  verificationQr: string | null;
}

// --- Optional features: streams (FR-C11), weekly work (FR-C12), permits ------

export interface StreamRow {
  id: string;
  classId: string;
  name: string;
  learnerCount: number;
  classTeacherName: string | null;
}

export interface WeeklySummary {
  subject: string;
  scores: (number | null)[];
  weeksDone: number;
  weeksSet: number;
  meanPercentage: number | null;
}

export interface WeeklyRow {
  studentId: string;
  regNumber: string;
  fullName: string;
  streamName: string | null;
  score: number | null;
  remark: string | null;
}

export interface WeeklySheet {
  classSubjectId: string;
  className: string;
  subjectName: string;
  termName: string;
  week: number;
  outOf: number;
  weeksRecorded: number[];
  rows: WeeklyRow[];
}

export interface ExamPermit {
  studentId: string;
  fullName: string;
  regNumber: string;
  streamName: string | null;
  photoDataUrl: string | null;
  serial: string;
}

export interface ExamPermitBatch {
  examName: string;
  termName: string;
  academicYear: string;
  className: string;
  examDate: string | null;
  subjects: string[];
  paperTimes: (string | null)[];
  permits: ExamPermit[];
  blocked: BlockedLearner[];
}

// --- Daily attendance (FR-G9) --------------------------------------------------

export type AttendanceState = "present" | "absent" | "late" | "excused";

export interface RegisterRow {
  studentId: string;
  regNumber: string;
  fullName: string;
  gender: string | null;
  streamName: string | null;
  guardianPhone: string | null;
  state: AttendanceState | null;
  note: string | null;
  absencesThisTerm: number;
}

export interface Register {
  classId: string;
  className: string;
  streamId: string | null;
  onDate: string;
  weekday: string;
  isWeekend: boolean;
  termId: string | null;
  termName: string | null;
  alreadyMarked: boolean;
  markedBy: string | null;
  rows: RegisterRow[];
}

export interface SaveRegisterResult {
  saved: number;
  present: number;
  absent: number;
  late: number;
  excused: number;
  textsQueued: number;
}

export interface AttendanceOverviewRow {
  classId: string;
  className: string;
  learners: number;
  marked: number;
  present: number;
  absent: number;
  late: number;
  excused: number;
  markedBy: string | null;
  classTeacherName: string | null;
}

export interface RegisterDay {
  date: string;
  day: number;
  initial: string;
  present: number | null;
}

export interface MonthRow {
  studentId: string;
  fullName: string;
  regNumber: string;
  gender: string | null;
  marks: string[];
  present: number;
  absent: number;
  late: number;
  excused: number;
  rate: number | null;
}

export interface MonthTotals {
  daysMarked: number;
  present: number;
  absent: number;
  late: number;
  excused: number;
  rate: number | null;
}

export interface MonthRegister {
  className: string;
  streamName: string | null;
  monthLabel: string;
  days: RegisterDay[];
  rows: MonthRow[];
  totals: MonthTotals;
}

// --- Timetables (FR-G6) and the exam timetable (FR-G7) -------------------------

export type PeriodKind = "lesson" | "break" | "lunch" | "assembly" | "games" | "prep";

export interface PeriodRow {
  id: string;
  seq: number;
  label: string;
  startTime: string;
  endTime: string;
  kind: PeriodKind;
}

export interface TimetableSetup {
  periods: PeriodRow[];
  days: number;
}

export interface SlotRow {
  day: number;
  periodId: string;
  subjectId: string | null;
  subjectName: string | null;
  teacherId: string | null;
  teacherName: string | null;
  room: string | null;
  clash: string | null;
}

export interface SubjectLoad {
  classSubjectId: string;
  subjectId: string;
  name: string;
  lessonsPerWeek: number;
  placed: number;
  teacherId: string | null;
  teacherName: string | null;
}

export interface ClassTimetable {
  classId: string;
  className: string;
  streamId: string | null;
  streamName: string | null;
  days: number;
  periods: PeriodRow[];
  slots: SlotRow[];
  subjects: SubjectLoad[];
}

export interface AutoFillResult {
  placed: number;
  unplaced: string[];
}

export interface TeacherSlot {
  day: number;
  periodId: string;
  className: string;
  streamName: string | null;
  subjectName: string | null;
  room: string | null;
}

export interface TeacherTimetable {
  userId: string;
  teacherName: string;
  days: number;
  periods: PeriodRow[];
  slots: TeacherSlot[];
}

export interface TimetableCell {
  day: number;
  periodId: string;
  main: string;
  sub: string | null;
}

export interface TimetableGrid {
  heading: string;
  subheading: string | null;
  cells: TimetableCell[];
}

export interface TimetableDocument {
  title: string;
  days: number;
  periods: PeriodRow[];
  grids: TimetableGrid[];
}

export interface ExamPaperRow {
  id: string;
  examId: string;
  subjectId: string;
  subjectName: string;
  paperLabel: string | null;
  onDate: string;
  startTime: string;
  endTime: string;
  venue: string | null;
  invigilatorId: string | null;
  invigilatorName: string | null;
  classIds: string[];
  classNames: string[];
}

export interface AutoExamResult {
  created: number;
  firstDate: string | null;
  lastDate: string | null;
}

export interface ExamDay {
  date: string;
  label: string;
  papers: ExamPaperRow[];
}

export interface ExamTimetableDocument {
  examName: string;
  termName: string;
  academicYear: string;
  className: string | null;
  days: ExamDay[];
}

export interface ExamTotal {
  marks: number | null;
  aggregate: number | null;
  average: number | null;
}

export interface PreviousTerm {
  termName: string;
  totalPoints: number | null;
  division: string | null;
  meanPercentage: number | null;
  position: number | null;
  classSize: number;
}

export interface GradeKeyBand {
  label: string;
  lower: number;
  upper: number;
  points: number | null;
}

export type Rating = "excellent" | "very_good" | "good" | "fair" | "needs_help";

export interface ActivityRating {
  activity: string;
  rating: Rating;
}

export interface ReportCommentDetail {
  classTeacherComment: string | null;
  headTeacherComment: string | null;
  conductComment: string | null;
  activityRatings: ActivityRating[];
  activityNames: string[];
}

export interface ReportSettings {
  headTeacherName: string;
  requirements: string;
  nurseryActivities: string[];
}

export interface BlockedLearner {
  studentId: string;
  fullName: string;
  regNumber: string;
  reason: string;
}

export interface ReportCardBatch {
  className: string;
  levelKind: "nursery" | "primary";
  activityNames: string[];
  nextTermBegins: string | null;
  requirements: string | null;
  headTeacherName: string | null;
  classAverage: number | null;
  examClassAverages: (number | null)[];
  gradeKey: GradeKeyBand[];
  weeklyWeeks: number;
  streamName: string | null;
  termName: string;
  academicYear: string;
  examNames: string[];
  isFinal: boolean;
  cards: ReportCard[];
  blocked: BlockedLearner[];
  /** Class Teachers' signatures, once each. */
  signatures: SignatureImage[];
  /** Print the fees block (balance and next term's fees) on each card. */
  showFees: boolean;
  nextTermFees: number | null;
}

export interface SignatureImage {
  userId: string;
  dataUrl: string;
}

// --- Signatures and the school stamp -----------------------------------------

export type SignatureKind = "head_teacher" | "stamp" | "staff";

export interface StaffSignature {
  userId: string;
  fullName: string;
  dataUrl: string | null;
}

export interface SignatureOverview {
  printSignatures: boolean;
  headTeacher: string | null;
  stamp: string | null;
  canManageSchool: boolean;
  /** The office, or the headteacher for their own signature. */
  canSetHeadSignature: boolean;
  staff: StaffSignature[];
}

export interface IdCard {
  id: string;
  fullName: string;
  number: string;
  title: string;
  detail: string | null;
  gender: string | null;
  dateOfBirth: string | null;
  contactLabel: string;
  contact: string | null;
  photoDataUrl: string | null;
}

export interface IdCardBatch {
  kind: "student" | "staff";
  academicYear: string | null;
  validUntil: string | null;
  cards: IdCard[];
}

export interface ClassListRow {
  number: number;
  regNumber: string;
  fullName: string;
  gender: string | null;
  guardianName: string | null;
  guardianPhone: string | null;
}

export interface ClassListBody {
  className: string;
  academicYear: string;
  termName: string | null;
  rows: ClassListRow[];
  total: number;
  boys: number;
  girls: number;
}

export interface CommentBankEntry {
  id: string;
  scope: "class_teacher" | "head_teacher";
  category: string | null;
  text: string;
  isBuiltin: boolean;
}

// --- System ----------------------------------------------------------------

/** The activity log's hash chain, checked from the entries themselves. */
export interface ChainReport {
  entries: number;
  intact: boolean;
  problem: string | null;
  problemEntry: number | null;
  headEntry: number;
  headHash: string;
}

export interface AuditIntegrity {
  report: ChainReport;
  /** The code a printout made now would carry. */
  currentCode: string;
}

export interface AuditExport {
  rangeLabel: string;
  search: string | null;
  entries: AuditEntry[];
  truncated: boolean;
  report: ChainReport;
  verificationCode: string;
  printedBy: string;
}

export interface CodeCheck {
  matches: boolean;
  entry: number;
  message: string;
}

export interface AuditEntry {
  id: number;
  at: string;
  actor_name: string | null;
  actor_role: string | null;
  action: string;
  entity: string | null;
  entity_id: string | null;
  summary: string;
}

export interface BackupStatus {
  lastBackupAt: string | null;
  lastBackupPath: string | null;
  mirrorPath: string | null;
  localBackupDir: string;
  cloudEnabled: boolean;
}

export interface BackupResult {
  localPath: string;
  mirrorPath: string | null;
  bytes: number;
  at: string;
}

export interface BackupFile {
  path: string;
  fileName: string;
  location: "local" | "mirror";
  bytes: number;
  modifiedAt: string;
}

export interface BackupSummary {
  path: string;
  institutionName: string | null;
  schemaVersion: number;
  learners: number;
  marks: number;
  lastActivityAt: string | null;
  compatible: boolean;
}

export interface ClassMarks {
  classId: string;
  className: string;
  entered: number;
  expected: number;
}

export interface RegistersToday {
  taken: number;
  classes: number;
}

export interface Desk {
  feesBlocked: number | null;
  marksBehind: ClassMarks[];
  registersToday: RegistersToday | null;
  myRegistersPending: string[];
}

export interface DashboardSummary {
  institutionName: string;
  academicYear: string | null;
  currentTerm: string | null;
  learners: number;
  boys: number;
  girls: number;
  classes: number;
  teachers: number;
  subjects: number;
  marksEnteredThisTerm: number;
  marksExpectedThisTerm: number;
  lastBackupAt: string | null;
  passOutsOut: number;
  overduePassOuts: OverduePassOut[];
  /** Each office's own figures; a field is absent for whoever may not see it. */
  desk: Desk;
}

// --- Pass-outs (FR-G22) and SMS (FR-G8) --------------------------------------

export type PassOutReason = "sick" | "appointment" | "family" | "permission" | "other";
export type SmsStatus = "queued" | "sent" | "failed";

export interface PassOutRow {
  id: string;
  number: number;
  studentId: string;
  studentName: string;
  regNumber: string;
  className: string;
  reasonKind: PassOutReason;
  reason: string | null;
  destination: string | null;
  pickedUpBy: string | null;
  pickedUpRelationship: string | null;
  pickedUpPhone: string | null;
  timeOut: string;
  expectedBack: string | null;
  returnedAt: string | null;
  status: "out" | "returned";
  guardianPhone: string | null;
  smsStatus: SmsStatus | null;
  smsError: string | null;
  returnSmsStatus: SmsStatus | null;
  issuedByName: string | null;
  overdue: boolean;
}

export interface OverduePassOut {
  id: string;
  studentName: string;
  className: string;
  expectedBack: string;
}

export interface PassOutSlip {
  number: number;
  studentName: string;
  regNumber: string;
  className: string;
  photoDataUrl: string | null;
  reasonLabel: string;
  reason: string | null;
  destination: string | null;
  pickedUpBy: string | null;
  pickedUpRelationship: string | null;
  pickedUpPhone: string | null;
  timeOut: string;
  expectedBack: string | null;
  issuedByName: string | null;
  guardianTexted: boolean;
}

export interface SmsSettings {
  provider: "off" | "africastalking" | "egosms" | "egosms_classic";
  username: string;
  apiKey: string;
  senderId: string;
  signature: string;
  /** Whether Class Teachers and group leaders may text their own parents. */
  teachersMayBroadcast: boolean;
  /** What one SMS costs, shillings: says how many texts the credit buys. */
  pricePerSms: number;
  /** Warn when the credit falls below this. */
  warnBelow: number;
}

export interface SmsOutboxRow {
  id: string;
  toPhone: string;
  body: string;
  kind: string;
  status: SmsStatus;
  attempts: number;
  lastError: string | null;
  createdAt: string;
  sentAt: string | null;
}

export interface FlushResult {
  sent: number;
  failed: number;
  stillQueued: number;
  /** Closed unsent because they were out of date by the time they could go. */
  expired: number;
  skippedReason: string | null;
}

// --- Bulk SMS (FR-G23) and learner groups ------------------------------------

export type AudienceKind = "school" | "class" | "stream" | "group" | "staff";

/** One part of who a message goes to. `id` is empty for school and staff. */
export interface AudiencePart {
  kind: AudienceKind;
  id: string | null;
}

export interface AudienceStream {
  id: string;
  name: string;
}

export interface AudienceClass {
  id: string;
  name: string;
  learners: number;
  streams: AudienceStream[];
}

export interface AudienceGroup {
  id: string;
  name: string;
  members: number;
}

/** What the signed-in person may text. */
export interface AudienceOptions {
  canTextSchool: boolean;
  canTextStaff: boolean;
  classes: AudienceClass[];
  groups: AudienceGroup[];
  smsReady: boolean;
  blockedReason: string | null;
}

export type SmsEncoding = "gsm" | "unicode";

export interface BroadcastPreview {
  audienceLabel: string;
  /** Phone numbers that will be texted. */
  recipients: number;
  /** Learners covered, each child counted once. */
  learners: number;
  /** SMS the gateway will charge for, across every recipient. */
  units: number;
  partsPerMessage: number;
  encoding: SmsEncoding;
  sample: string | null;
  missingCount: number;
  missingNames: string[];
  notes: string[];
}

export interface BroadcastSent {
  id: string;
  recipients: number;
  units: number;
  audienceLabel: string;
}

export interface BroadcastRow {
  id: string;
  audienceLabel: string;
  template: string;
  recipients: number;
  units: number;
  sent: number;
  waiting: number;
  failed: number;
  sentBy: string;
  createdAt: string;
}

export interface GroupRow {
  id: string;
  name: string;
  note: string | null;
  leaderId: string | null;
  leaderName: string | null;
  members: number;
}

export interface GroupInput {
  name: string;
  note: string | null;
  leaderId: string | null;
}

export interface GroupMember {
  studentId: string;
  fullName: string;
  regNumber: string;
  className: string | null;
  hasPhone: boolean;
}

// --- Mark anomaly detection ------------------------------------------------

export interface MarkWarning {
  studentId: string;
  studentName: string;
  score: number;
  usualPercentage: number;
  comparedAgainst: number;
  suggested: number | null;
  reason: string;
}

// --- Insights --------------------------------------------------------------

export interface DivisionCount {
  label: string;
  count: number;
}

export interface ImprovementStep {
  subjectName: string;
  currentScore: number;
  maxScore: number;
  currentGrade: string;
  nextGrade: string;
  marksNeeded: number;
  pointsGained: number;
}

export interface NearMiss {
  studentId: string;
  fullName: string;
  regNumber: string;
  aggregate: number;
  currentDivision: string;
  targetDivision: string;
  pointsNeeded: number;
  totalMarksNeeded: number;
  steps: ImprovementStep[];
}

export interface IncompleteLearner {
  studentId: string;
  fullName: string;
  missing: string[];
}

export interface PleProjection {
  className: string;
  termName: string;
  academicYear: string;
  totalLearners: number;
  projectedLearners: number;
  coreSubjects: string[];
  divisions: DivisionCount[];
  nearMisses: NearMiss[];
  incomplete: IncompleteLearner[];
  divisionsComputable: boolean;
  bestAggregate: number | null;
  meanAggregate: number | null;
}

export interface HeatCell {
  className: string;
  subjectName: string;
  meanPercentage: number;
  delta: number;
  learners: number;
}

export interface SubjectHeatmap {
  termName: string;
  classes: string[];
  subjects: string[];
  cells: HeatCell[];
  schoolMean: number | null;
  weakest: HeatCell[];
}

// --- Onboarding import (FR-G13) --------------------------------------------

export interface SpreadsheetPreview {
  fileName: string;
  sheets: string[];
  sheet: string;
  headers: string[];
  rows: string[][];
  totalRows: number;
  truncated: boolean;
  suggestedMapping: Record<string, number>;
}

export interface ImportRow {
  rowNumber: number;
  fullName: string;
  regNumber: string | null;
  className: string | null;
  gender: string | null;
  guardianName: string | null;
  guardianPhone: string | null;
  status: "ready" | "blocked" | "skipped" | "imported";
  problems: string[];
  notes: string[];
}

export interface ImportResult {
  committed: boolean;
  total: number;
  ready: number;
  blocked: number;
  imported: number;
  rows: ImportRow[];
}

/** The Phantom School Manager fields an import can fill, in the order the mapping screen shows them. */
export const IMPORT_FIELDS = [
  { key: "fullName", label: "Learner's name", required: true },
  { key: "className", label: "Class", required: false },
  { key: "regNumber", label: "Registration number", required: false },
  { key: "gender", label: "Sex", required: false },
  { key: "dateOfBirth", label: "Date of birth", required: false },
  { key: "lin", label: "LIN", required: false },
  { key: "guardianName", label: "Guardian's name", required: false },
  { key: "guardianPhone", label: "Guardian's phone", required: false },
  { key: "guardianRelationship", label: "Relationship", required: false },
] as const;

// --- Subscription (FR-B11) ------------------------------------------------------

export type LicenceState = "pilot" | "trial" | "active" | "grace" | "expired";

export interface LicenceStatus {
  state: LicenceState;
  /** What the school sends the owner when it pays. */
  schoolCode: string;
  /** Past the grace days: everything can be read, printed and backed up; nothing changed. */
  readOnly: boolean;
  plan: string | null;
  nameOnKey: string | null;
  keyId: string | null;
  /** The last day of full use. */
  until: string | null;
  daysLeft: number | null;
  clockRolledBack: boolean;
}


// ---------------------------------------------------------------------------
// Offline only
// ---------------------------------------------------------------------------

export interface Connectivity {
  /** The school switched every internet feature off. */
  offlineOnly: boolean;
  /** Texts waiting in the outbox. */
  queuedMessages: number;
}

// ---------------------------------------------------------------------------
// Fees (FR-G5)
// ---------------------------------------------------------------------------

export type FeeItemKind = "termly" | "optional";
export type PaymentMethod = "cash" | "mobile_money" | "bank" | "schoolpay" | "cheque" | "other";
export type AdjustmentKind = "bursary" | "discount" | "waiver" | "charge" | "balance_forward" | "prepayment";

export interface FeeSettings {
  paymentInstructions: string;
  showOnReportCard: boolean;
  autoBlockAbove: number | null;
  textReceipts: boolean;
}

export interface FeeItemRow {
  id: string;
  name: string;
  kind: FeeItemKind;
  position: number;
}

export interface FeeRateRow {
  feeItemId: string;
  classId: string;
  amount: number;
}

export interface FeeTermOption {
  id: string;
  label: string;
  status: string;
  hasRates: boolean;
}

export interface FeeClassOption {
  id: string;
  name: string;
  learners: number;
}

export interface FeeSetup {
  termId: string;
  terms: FeeTermOption[];
  items: FeeItemRow[];
  classes: FeeClassOption[];
  rates: FeeRateRow[];
  /** Learners taking each optional item, per class (`amount` is the count). */
  optinCounts: FeeRateRow[];
}

export interface FeeItemInput {
  id: string | null;
  name: string;
  kind: FeeItemKind;
  position: number | null;
}

export interface FeeRateInput {
  feeItemId: string;
  classId: string;
  amount: number;
}

export interface OptinRow {
  studentId: string;
  fullName: string;
  className: string;
  taking: boolean;
}

export interface PromiseRow {
  id: string;
  studentId: string;
  termId: string;
  amount: number;
  dueOn: string;
  promisedBy: string | null;
  note: string | null;
  status: "open" | "kept" | "broken" | "cancelled";
  createdAt: string;
  overdue: boolean;
}

export interface FeeAccountRow {
  studentId: string;
  fullName: string;
  regNumber: string;
  className: string;
  guardianPhone: string | null;
  broughtForward: number;
  billed: number;
  paid: number;
  balance: number;
  promise: PromiseRow | null;
}

export interface FeeTotals {
  learners: number;
  expected: number;
  collected: number;
  outstanding: number;
  owing: number;
  credit: number;
}

export interface FeeAccountList {
  termId: string;
  termName: string;
  rows: FeeAccountRow[];
  totals: FeeTotals;
}

export interface PaymentRow {
  id: string;
  receiptNo: number;
  receiptLabel: string;
  studentId: string;
  studentName: string;
  className: string | null;
  termId: string;
  amount: number;
  method: PaymentMethod;
  methodLabel: string;
  reference: string | null;
  paidBy: string | null;
  paidOn: string;
  note: string | null;
  source: string;
  balanceAfter: number | null;
  status: "valid" | "voided";
  receivedByName: string | null;
  createdAt: string;
  voidReason: string | null;
  voidedByName: string | null;
  voidedAt: string | null;
}

export interface AdjustmentRow {
  id: string;
  termId: string;
  termName: string;
  feeItemName: string | null;
  kind: AdjustmentKind;
  label: string;
  amount: number | null;
  percent: number | null;
  reason: string;
  status: "active" | "cancelled";
  createdByName: string | null;
  createdAt: string;
  cancelReason: string | null;
}

export interface LearnerBrief {
  studentId: string;
  fullName: string;
  regNumber: string;
  className: string | null;
  guardianName: string | null;
  guardianPhone: string | null;
}

export interface BillLine {
  label: string;
  amount: number;
  kind: string;
}

export interface TermAccount {
  termId: string;
  termName: string;
  yearLabel: string;
  className: string | null;
  broughtForward: number;
  lines: BillLine[];
  billed: number;
  paid: number;
  balance: number;
}

export interface FeeAccountDetail {
  learner: LearnerBrief;
  terms: TermAccount[];
  balance: number;
  payments: PaymentRow[];
  adjustments: AdjustmentRow[];
  promises: PromiseRow[];
}

export interface RecordPaymentRequest {
  studentId: string;
  amount: number;
  method: PaymentMethod;
  reference: string | null;
  paidBy: string | null;
  payerPhone: string | null;
  paidOn: string | null;
  note: string | null;
  textGuardian: boolean;
}

export interface RecordedPayment {
  payment: PaymentRow;
  guardianTexted: boolean;
  promisesKept: number;
}

export interface AdjustmentInput {
  studentIds: string[];
  termId: string | null;
  feeItemId: string | null;
  kind: AdjustmentKind;
  amount: number | null;
  percent: number | null;
  reason: string;
}

export interface PromiseInput {
  studentId: string;
  amount: number;
  dueOn: string;
  promisedBy: string | null;
  note: string | null;
}

export interface CollectionDay {
  date: string;
  cash: number;
  mobileMoney: number;
  bank: number;
  schoolpay: number;
  other: number;
  total: number;
  receipts: number;
}

export interface Collections {
  from: string;
  to: string;
  days: CollectionDay[];
  totals: CollectionDay;
  payments: PaymentRow[];
  voided: PaymentRow[];
}

export interface FeeReceipt {
  payment: PaymentRow;
  regNumber: string;
  termName: string;
  amountWords: string;
  verificationCode: string;
  verificationQr: string;
}

export interface FeeInvoice {
  learner: LearnerBrief;
  broughtForward: number;
  lines: BillLine[];
  billed: number;
  paid: number;
  balance: number;
  promise: PromiseRow | null;
}

export interface FeeInvoiceBatch {
  termName: string;
  className: string | null;
  paymentInstructions: string;
  invoices: FeeInvoice[];
}

export interface InvoiceRequest {
  termId: string | null;
  classId: string | null;
  studentIds: string[] | null;
  onlyOwing: boolean;
}

// ---------------------------------------------------------------------------
// Checking a printed document
// ---------------------------------------------------------------------------

export interface DocumentCheck {
  genuine: boolean;
  kind: "receipt" | "report_card" | "";
  headline: string;
  /** What the school recorded, label then value, to compare with the paper. */
  facts: [string, string][];
  warning: string | null;
}

/** "UGX 1,250,000". */
export function ugx(amount: number): string {
  const sign = amount < 0 ? "-" : "";
  return `${sign}UGX ${Math.abs(Math.round(amount)).toLocaleString("en-US")}`;
}

export const PAYMENT_METHODS: { id: PaymentMethod; label: string; needsReference: boolean; referenceLabel: string }[] = [
  { id: "cash", label: "Cash", needsReference: false, referenceLabel: "Reference (optional)" },
  { id: "mobile_money", label: "Mobile Money", needsReference: true, referenceLabel: "Transaction ID" },
  { id: "bank", label: "Bank deposit", needsReference: true, referenceLabel: "Bank slip number" },
  { id: "schoolpay", label: "SchoolPay", needsReference: true, referenceLabel: "SchoolPay transaction ID" },
  { id: "cheque", label: "Cheque", needsReference: true, referenceLabel: "Cheque number" },
  { id: "other", label: "Other", needsReference: false, referenceLabel: "Reference (optional)" },
];

export const ADJUSTMENT_KINDS: { id: AdjustmentKind; label: string; reduces: boolean; percentAllowed: boolean; hint: string }[] = [
  { id: "bursary", label: "Bursary", reduces: true, percentAllowed: true, hint: "Sponsored or a school bursary" },
  { id: "discount", label: "Discount", reduces: true, percentAllowed: true, hint: "Sibling, staff child, early payment" },
  { id: "waiver", label: "Waiver", reduces: true, percentAllowed: true, hint: "Forgiven by the head or director" },
  { id: "charge", label: "Extra charge", reduces: false, percentAllowed: false, hint: "A trip, a broken window, a new uniform" },
  { id: "balance_forward", label: "Balance brought forward", reduces: false, percentAllowed: false, hint: "Owed from before Phantom" },
  { id: "prepayment", label: "Paid before Phantom", reduces: true, percentAllowed: false, hint: "Money already paid, from the old books" },
];

// ---------------------------------------------------------------------------
// Day Book
// ---------------------------------------------------------------------------

export interface DayRow {
  date: string;
  /** Null when the reader may not see money. */
  feesCollected: number | null;
  feesCash: number | null;
  receipts: number | null;
  receiptsCancelled: number | null;
  spent: number | null;
  registers: number;
  present: number;
  absent: number;
  late: number;
  attendancePercent: number | null;
  passOuts: number;
  visitors: number;
  textsSent: number;
  textsFailed: number;
  learnersAdmitted: number;
  learnersLeft: number;
  staffSignedIn: number;
  staffLate: number;
  activity: number;
  note: string | null;
  noteBy: string | null;
}

export interface DayBook {
  from: string;
  to: string;
  money: boolean;
  days: DayRow[];
  totals: DayRow;
}

export interface OwnerSmsSettings {
  enabled: boolean;
  phones: string[];
  sendAt: string;
  includeMoney: boolean;
}

// ---------------------------------------------------------------------------
// Staff sign-in register
// ---------------------------------------------------------------------------

export interface StaffRegisterSettings {
  enabled: boolean;
  startTime: string;
}

export interface StationPerson {
  userId: string;
  fullName: string;
  hasPin: boolean;
  signedInAt: string | null;
  signedOutAt: string | null;
}

export interface StationResult {
  fullName: string;
  action: "in" | "out";
  time: string;
  late: boolean;
}

export interface RegisterRow {
  userId: string;
  fullName: string;
  office: string | null;
  signedInAt: string | null;
  signedOutAt: string | null;
  late: boolean;
  method: "pin" | "office" | null;
  note: string | null;
}

export interface OfficeEntry {
  userId: string;
  onDate: string;
  signedInAt: string;
  signedOutAt: string | null;
  note: string;
}

export interface MonthRow {
  userId: string;
  fullName: string;
  office: string | null;
  daysIn: number;
  daysLate: number;
  averageIn: string | null;
  marks: string[];
}

export interface StaffMonth {
  month: string;
  label: string;
  days: number[];
  startTime: string;
  rows: MonthRow[];
}

// ---------------------------------------------------------------------------
// Automatic texts
// ---------------------------------------------------------------------------

export interface AutoSmsSettings {
  absence: boolean;
  results: boolean;
  feeReminders: boolean;
  /** 1 = Monday … 7 = Sunday. */
  feeReminderDay: number;
  feeReminderTime: string;
  feeReminderAbove: number;
  promiseReminders: boolean;
  notReported: boolean;
}

export interface TextPlan {
  recipients: number;
  learners: number;
  withoutPhone: number;
  smsUnits: number;
  sample: string | null;
  skippedRecent: number;
}

export interface SmsBalance {
  amount: number | null;
  currency: string;
  checkedAt: string | null;
  low: boolean;
  warnBelow: number;
  textsLeft: number | null;
}

// ---------------------------------------------------------------------------
// A new school year
// ---------------------------------------------------------------------------

export interface PromotionLearner {
  studentId: string;
  fullName: string;
  regNumber: string;
  average: number | null;
  action: "promote" | "repeat" | "leave";
  considerRepeating: boolean;
}

export interface PromotionClass {
  classId: string;
  className: string;
  nextClassId: string | null;
  nextClassName: string | null;
  learners: PromotionLearner[];
}

export interface PromotionPlan {
  fromYearId: string;
  fromYear: string;
  suggestedLabel: string;
  resultsTerm: string | null;
  repeatBelow: number;
  classes: PromotionClass[];
}

export interface NewTerm {
  name: string;
  startDate: string | null;
  endDate: string | null;
}

export interface Decision {
  studentId: string;
  action: "promote" | "repeat" | "leave";
}

export interface NewYearRequest {
  label: string;
  terms: NewTerm[];
  decisions: Decision[];
}

export interface NewYearResult {
  yearLabel: string;
  promoted: number;
  repeating: number;
  left: number;
  firstTerm: string;
}

// ---------------------------------------------------------------------------
// Recovery code
// ---------------------------------------------------------------------------

export interface RecoverySheet {
  code: string;
  username: string;
  fullName: string;
  createdAt: string;
}

export interface RecoveryStatus {
  mayHaveCode: boolean;
  createdAt: string | null;
}

export interface UpdateInfo {
  currentVersion: string;
  configured: boolean;
  offlineOnly: boolean;
  lastChecked: string | null;
  availableVersion: string | null;
  availableNotes: string | null;
}

// ---------------------------------------------------------------------------
// SchoolPay statements
// ---------------------------------------------------------------------------

export interface StatementColumns {
  reference: number | null;
  amount: number | null;
  date: number | null;
  paymentCode: number | null;
  regNumber: number | null;
  name: number | null;
  payer: number | null;
}

export interface StatementLine {
  row: number;
  reference: string;
  amount: number | null;
  paidOn: string | null;
  nameOnStatement: string;
  paymentCode: string;
  status: "ready" | "suggested" | "duplicate" | "unmatched" | "invalid";
  studentId: string | null;
  studentName: string | null;
  className: string | null;
  matchedBy: "code" | "reg" | "name" | null;
  problem: string | null;
}

export interface StatementPlan {
  fileName: string;
  headers: string[];
  columns: StatementColumns;
  lines: StatementLine[];
  ready: number;
  readyAmount: number;
}

export interface ImportChoice {
  row: number;
  studentId: string;
}

export interface StatementImported {
  receipts: number;
  amount: number;
  skipped: number;
  codesLearned: number;
}

// ---------------------------------------------------------------------------
// Term requirements
// ---------------------------------------------------------------------------

export interface RequirementItem {
  id: string;
  termId: string;
  classId: string | null;
  className: string | null;
  name: string;
  quantity: number;
  cashValue: number | null;
}

export interface RequirementInput {
  id: string | null;
  termId: string | null;
  classId: string | null;
  name: string;
  quantity: number;
  cashValue: number | null;
}

export interface RequirementEntry {
  itemId: string;
  quantity: number;
  cashPaid: number;
  complete: boolean;
}

export interface RequirementRow {
  studentId: string;
  fullName: string;
  entries: RequirementEntry[];
  outstanding: string[];
}

export interface RequirementSheet {
  termId: string;
  termName: string;
  classId: string;
  className: string;
  items: RequirementItem[];
  rows: RequirementRow[];
  complete: number;
}

// --- Registrations: PLE candidates, EMIS, sports; exports -------------------

export interface Issue {
  /** name, gender, dob, lin, photo, age, team, nin */
  field: string;
  serious: boolean;
  message: string;
}

export interface LearnerCheck {
  studentId: string;
  regNumber: string;
  fullName: string;
  gender: string | null;
  dateOfBirth: string | null;
  lin: string | null;
  specialNeeds: string | null;
  guardianPhone: string | null;
  classId: string;
  className: string;
  hasPhoto: boolean;
  age: number | null;
  issues: Issue[];
  photoDataUrl: string | null;
  discipline: string | null;
}

export interface LearnerFix {
  fullName: string;
  gender: string | null;
  dateOfBirth: string | null;
  lin: string | null;
  specialNeeds: string | null;
}

export interface PleCheck {
  classId: string;
  className: string;
  examYear: number;
  candidates: LearnerCheck[];
  ready: number;
  returnBy: string | null;
}

export interface StaffCheck {
  userId: string;
  fullName: string;
  office: string | null;
  gender: string | null;
  phone: string | null;
  nin: string | null;
  tmisNumber: string | null;
  issues: Issue[];
}

export interface StaffIds {
  gender: string | null;
  nin: string | null;
  tmisNumber: string | null;
}

export interface AgeCount {
  age: number;
  boys: number;
  girls: number;
}

export interface CensusClass {
  className: string;
  boys: number;
  girls: number;
  unknownSex: number;
  newBoys: number;
  newGirls: number;
  repeatBoys: number;
  repeatGirls: number;
  specialNeeds: number;
  byAge: AgeCount[];
  unknownAge: number;
}

export interface EmisCheck {
  censusDate: string;
  yearLabel: string;
  classes: CensusClass[];
  ages: number[];
  learnersTotal: number;
  learnersReady: number;
  learners: LearnerCheck[];
  staff: StaffCheck[];
}

export interface SportsEvent {
  id: string;
  name: string;
  sport: string;
  termId: string | null;
  bornOnOrAfter: string | null;
  /** M, F or any */
  sex: string;
  maxAthletes: number | null;
  athletes: number;
}

export interface SportsEventInput {
  id: string | null;
  name: string;
  sport: string;
  bornOnOrAfter: string | null;
  sex: string;
  maxAthletes: number | null;
}

export interface SportsTeam {
  event: SportsEvent;
  athletes: LearnerCheck[];
  ready: number;
  warnings: string[];
}

export interface Correction {
  studentId: string;
  what: string;
  wrong: string;
  right: string;
}

export interface LetterInput {
  eventId: string;
  addressee: string;
  corrections: Correction[];
  note: string | null;
}

export interface CorrectionLine {
  fullName: string;
  lin: string | null;
  className: string;
  what: string;
  wrong: string;
  right: string;
}

export interface CorrectionLetter {
  date: string;
  addressee: string[];
  subject: string;
  eventName: string;
  sport: string;
  lines: CorrectionLine[];
  note: string | null;
  headTeacherName: string;
}

export interface StagedFile {
  file: string;
  suggestedName: string;
  /** xlsx, csv or folder */
  kind: string;
  detail: string;
}

export interface Invite {
  userId: string;
  fullName: string;
  username: string;
  code: string;
  expiresAt: string;
  message: string;
  phone: string | null;
  smsQueued: boolean;
}

// --- Spending, other income and banking -------------------------------------

export interface CashEntry {
  id: string;
  /** expense, income or banking */
  kind: string;
  voucher: string;
  onDate: string;
  category: string;
  description: string;
  party: string | null;
  amount: number;
  method: string;
  reference: string | null;
  status: string;
  recordedBy: string | null;
  voidReason: string | null;
  amountWords: string;
}

export interface CashEntryInput {
  kind: string;
  onDate: string | null;
  category: string;
  description: string;
  party: string | null;
  amount: number;
  method: string;
  reference: string | null;
}

export interface CategoryTotal {
  category: string;
  amount: number;
  entries: number;
}

export interface CashSummary {
  from: string;
  to: string;
  fees: number;
  otherIncome: number;
  income: number;
  spent: number;
  surplus: number;
  banked: number;
  cashAtStart: number;
  cashAtEnd: number;
  spending: CategoryTotal[];
  incomeBy: CategoryTotal[];
  entries: CashEntry[];
  expenseCategories: string[];
  incomeCategories: string[];
}

export interface ExportSection {
  id: string;
  label: string;
  allowed: boolean;
}

// --- Visitors and conduct --------------------------------------------------

export interface Visitor {
  id: string;
  slip: string;
  fullName: string;
  phone: string | null;
  idNumber: string | null;
  organisation: string | null;
  purpose: string;
  toSee: string | null;
  studentId: string | null;
  studentName: string | null;
  className: string | null;
  vehicle: string | null;
  note: string | null;
  timeIn: string;
  timeOut: string | null;
  signedInBy: string | null;
}

export interface VisitorInput {
  fullName: string;
  phone: string | null;
  idNumber: string | null;
  organisation: string | null;
  purpose: string;
  toSee: string | null;
  studentId: string | null;
  vehicle: string | null;
  note: string | null;
}

export interface VisitorBook {
  from: string;
  to: string;
  inside: Visitor[];
  visitors: Visitor[];
}

export const VISIT_PURPOSES: { id: string; label: string }[] = [
  { id: "parent", label: "Parent visiting a learner" },
  { id: "meeting", label: "Meeting someone at school" },
  { id: "official", label: "Official or inspector" },
  { id: "delivery", label: "Delivery or supplier" },
  { id: "job", label: "Looking for work" },
  { id: "other", label: "Other" },
];

export interface ConductEntry {
  id: string;
  studentId: string;
  studentName: string;
  classId: string;
  className: string;
  onDate: string;
  /** commendation or incident */
  kind: string;
  category: string;
  description: string;
  actionTaken: string | null;
  parentInformed: boolean;
  status: string;
  withdrawnReason: string | null;
  recordedBy: string | null;
}

export interface ConductInput {
  studentId: string;
  onDate: string | null;
  kind: string;
  category: string;
  description: string;
  actionTaken: string | null;
  parentInformed: boolean;
  textParent: boolean;
}

export interface ConductSaved {
  entry: ConductEntry;
  texted: boolean;
}

export interface ConductLog {
  entries: ConductEntry[];
  incidents: string[];
  commendations: string[];
  actions: string[];
}
