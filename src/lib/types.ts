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

export interface SessionView {
  userId: string;
  username: string;
  fullName: string;
  role: Role;
  mode: TeacherMode | null;
  mustChangePassword: boolean;
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
  totalScore: number | null;
  totalMax: number;
  activities: ActivityRating[];
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
  streamName: string | null;
  termName: string;
  academicYear: string;
  examNames: string[];
  isFinal: boolean;
  cards: ReportCard[];
  blocked: BlockedLearner[];
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
  recentActivity: AuditEntry[];
  passOutsOut: number;
  overduePassOuts: OverduePassOut[];
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
  provider: "off" | "africastalking" | "egosms";
  username: string;
  apiKey: string;
  senderId: string;
  signature: string;
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
  skippedReason: string | null;
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

/** The RM fields an import can fill, in the order the mapping screen shows them. */
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
