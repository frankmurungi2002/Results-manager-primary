/**
 * The only route from the interface to the backend.
 *
 * Every call is a named Tauri command; there is no SQL, no file access and no
 * credential in this layer. `call` normalises the error shape so a screen can
 * branch on `error.code` instead of parsing a message.
 */

import { invoke } from "@tauri-apps/api/core";

import type {
  AcademicYearRow,
  AppError,
  AssignmentRow,
  AuditEntry,
  BackupFile,
  BackupResult,
  BackupStatus,
  BackupSummary,
  ClassListBody,
  ClassRow,
  ClassSubjectRow,
  CommentBankEntry,
  CreateStaffResult,
  DashboardSummary,
  DocumentEnvelope,
  FeatureFlags,
  GradingSystem,
  ImportResult,
  Institution,
  MarkEntry,
  MarksSheet,
  PleProjection,
  ReportCardBatch,
  SaveMarksResult,
  SessionView,
  SpreadsheetPreview,
  SubjectHeatmap,
  SetupDefaults,
  SetupRequest,
  SetupResult,
  StartupState,
  StudentRow,
  SubjectAnalytics,
  SubjectProgress,
  SubjectRow,
  TeacherMode,
  UserSummary,
} from "./types";

export class ApiError extends Error {
  readonly code: AppError["code"];

  constructor(error: AppError) {
    super(error.message);
    this.name = "ApiError";
    this.code = error.code;
  }
}

function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value
  );
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    if (isAppError(raw)) {
      throw new ApiError(raw);
    }
    // A command that is not registered, or a panic, arrives as a bare string.
    throw new ApiError({
      code: "internal",
      message:
        typeof raw === "string" ? raw : "Something went wrong. Please try again.",
    });
  }
}

export const api = {
  // --- Session ------------------------------------------------------------
  startupState: () => call<StartupState>("startup_state"),
  signIn: (username: string, password: string) =>
    call<SessionView>("sign_in", { username, password }),
  signOut: () => call<void>("sign_out"),
  currentSession: () => call<SessionView | null>("current_session"),
  setTeacherMode: (mode: TeacherMode) =>
    call<SessionView>("set_teacher_mode", { mode }),
  changePassword: (currentPassword: string, newPassword: string) =>
    call<void>("change_password", { currentPassword, newPassword }),
  resetUserPassword: (userId: string) =>
    call<string>("reset_user_password", { userId }),

  // --- First run ----------------------------------------------------------
  setupDefaults: () => call<SetupDefaults>("setup_defaults"),
  completeSetup: (request: SetupRequest) =>
    call<SetupResult>("complete_setup", { request }),

  // --- Institution and system --------------------------------------------
  getInstitution: () => call<Institution>("get_institution"),
  updateInstitution: (request: {
    name: string;
    motto?: string | null;
    address?: string | null;
    phone?: string | null;
    email?: string | null;
    accentColor: string;
  }) => call<void>("update_institution", { request }),
  setInstitutionLogo: (png: number[] | null) =>
    call<void>("set_institution_logo", { png }),
  getInstitutionLogo: () => call<number[] | null>("get_institution_logo"),
  getTheme: () => call<string>("get_theme"),
  setTheme: (theme: string) => call<void>("set_theme", { theme }),
  searchAuditLog: (query?: string, limit = 100, offset = 0) =>
    call<AuditEntry[]>("search_audit_log", { query, limit, offset }),
  backupStatus: () => call<BackupStatus>("backup_status"),
  setMirrorPath: (path: string | null) => call<void>("set_mirror_path", { path }),
  runBackup: () => call<BackupResult>("run_backup"),
  listBackups: () => call<BackupFile[]>("list_backups"),
  inspectBackup: (path: string) => call<BackupSummary>("inspect_backup", { path }),
  restoreBackup: (path: string) => call<BackupSummary>("restore_backup", { path }),
  dashboardSummary: () => call<DashboardSummary>("dashboard_summary"),

  // --- Academic structure -------------------------------------------------
  listClasses: () => call<ClassRow[]>("list_classes"),
  saveClass: (request: {
    id?: string;
    code: string;
    name: string;
    levelKind: string;
    ladderPosition: number;
    defaultGradingSystemId?: string | null;
  }) => call<string>("save_class", { request }),
  retireClass: (classId: string) => call<void>("retire_class", { classId }),

  listSubjects: () => call<SubjectRow[]>("list_subjects"),
  saveSubject: (request: {
    id?: string;
    code: string;
    name: string;
    maxScore: number;
    passMark: number;
  }) => call<string>("save_subject", { request }),

  listClassSubjects: (classId: string) =>
    call<ClassSubjectRow[]>("list_class_subjects", { classId }),
  addClassSubject: (request: {
    classId: string;
    subjectId: string;
    isCore: boolean;
  }) => call<string>("add_class_subject", { request }),
  updateClassSubject: (request: {
    id: string;
    displayName?: string | null;
    gradingSystemId?: string | null;
    maxScore?: number | null;
    passMark?: number | null;
    isCore?: boolean | null;
  }) => call<void>("update_class_subject", { request }),
  retireClassSubject: (id: string) => call<void>("retire_class_subject", { id }),

  listGradingSystems: () => call<GradingSystem[]>("list_grading_systems"),
  saveGradingSystem: (request: {
    id?: string;
    name: string;
    description?: string | null;
    kind: string;
    bands: {
      label: string;
      lowerBound: number;
      upperBound: number;
      points?: number | null;
      remark?: string | null;
    }[];
  }) => call<string>("save_grading_system", { request }),

  listAcademicYears: () => call<AcademicYearRow[]>("list_academic_years"),
  openTerm: (termId: string) => call<void>("open_term", { termId }),
  closeTerm: (termId: string) => call<void>("close_term", { termId }),

  getFeatures: () => call<FeatureFlags>("get_features"),
  setFeature: (key: string, enabled: boolean) =>
    call<void>("set_feature", { key, enabled }),

  // --- Staff --------------------------------------------------------------
  listStaff: () => call<UserSummary[]>("list_staff"),
  createStaff: (request: {
    fullName: string;
    username: string;
    role: string;
    email?: string | null;
    phone?: string | null;
  }) => call<CreateStaffResult>("create_staff", { request }),
  retireStaff: (userId: string) => call<void>("retire_staff", { userId }),
  listAssignments: (classId?: string) =>
    call<AssignmentRow[]>("list_assignments", { classId: classId ?? null }),
  assignTeacher: (request: {
    userId: string;
    classId: string;
    streamId?: string | null;
    subjectId?: string | null;
    role: string;
  }) => call<string>("assign_teacher", { request }),
  unassignTeacher: (assignmentId: string) =>
    call<void>("unassign_teacher", { assignmentId }),

  // --- Learners -----------------------------------------------------------
  listClassRoster: (classId: string, includeDropped = false) =>
    call<StudentRow[]>("list_class_roster", { classId, includeDropped }),
  searchStudents: (query: string) =>
    call<StudentRow[]>("search_students", { query }),
  saveStudent: (request: {
    id?: string;
    fullName: string;
    regNumber?: string | null;
    gender?: string | null;
    dateOfBirth?: string | null;
    lin?: string | null;
    guardianName?: string | null;
    guardianPhone?: string | null;
    guardianRelationship?: string | null;
    address?: string | null;
    classId?: string | null;
    streamId?: string | null;
  }) => call<{ id: string; regNumber: string }>("save_student", { request }),
  dropStudent: (studentId: string, reason: string) =>
    call<void>("drop_student", { studentId, reason }),
  readmitStudent: (studentId: string, classId: string, streamId?: string | null) =>
    call<void>("readmit_student", { studentId, classId, streamId: streamId ?? null }),
  transferStudent: (studentId: string, classId: string, streamId?: string | null) =>
    call<void>("transfer_student", { studentId, classId, streamId: streamId ?? null }),
  setStudentPhoto: (studentId: string, png: number[]) =>
    call<void>("set_student_photo", { studentId, png }),
  getStudentPhoto: (studentId: string) =>
    call<number[] | null>("get_student_photo", { studentId }),

  // --- Marks --------------------------------------------------------------
  loadMarksSheet: (classSubjectId: string, examId: string) =>
    call<MarksSheet>("load_marks_sheet", { classSubjectId, examId }),
  saveMarks: (classSubjectId: string, examId: string, entries: MarkEntry[]) =>
    call<SaveMarksResult>("save_marks", { classSubjectId, examId, entries }),
  marksProgress: (classId: string, examId: string) =>
    call<SubjectProgress[]>("marks_progress", { classId, examId }),
  subjectAnalytics: (classSubjectId: string, examId: string) =>
    call<SubjectAnalytics>("subject_analytics", { classSubjectId, examId }),
  setMarksDeadline: (classId: string, examId: string, deadline: string | null) =>
    call<void>("set_marks_deadline", { request: { classId, examId, deadline } }),
  getMarksDeadline: (classId: string, examId: string) =>
    call<string | null>("get_marks_deadline", { classId, examId }),

  // --- Insights: questions nobody could ask before -------------------------
  pleProjection: (classId: string, termId: string) =>
    call<PleProjection>("ple_projection", { classId, termId }),
  subjectHeatmap: (termId: string) =>
    call<SubjectHeatmap>("subject_heatmap", { termId }),

  // --- Onboarding import (FR-G13) -----------------------------------------
  readSpreadsheet: (path: string, sheet?: string, headerRow?: number) =>
    call<SpreadsheetPreview>("read_spreadsheet", {
      path,
      sheet: sheet ?? null,
      headerRow: headerRow ?? null,
    }),
  importLearners: (request: {
    path: string;
    sheet?: string | null;
    headerRow?: number | null;
    mapping: Record<string, number>;
    defaultClassId?: string | null;
    commit: boolean;
    skipRows?: number[];
  }) =>
    call<ImportResult>("import_learners", {
      request: {
        path: request.path,
        sheet: request.sheet ?? null,
        headerRow: request.headerRow ?? null,
        mapping: request.mapping,
        defaultClassId: request.defaultClassId ?? null,
        commit: request.commit,
        skipRows: request.skipRows ?? [],
      },
    }),
  readImageFile: (path: string) => call<number[]>("read_image_file", { path }),

  // --- Documents ----------------------------------------------------------
  buildReportCards: (request: {
    classId: string;
    termId: string;
    studentIds?: string[];
    examIds?: string[];
  }) =>
    call<DocumentEnvelope<ReportCardBatch>>("build_report_cards", {
      request: {
        classId: request.classId,
        termId: request.termId,
        studentIds: request.studentIds ?? [],
        examIds: request.examIds ?? [],
      },
    }),
  buildClassList: (classId: string) =>
    call<DocumentEnvelope<ClassListBody>>("build_class_list", { classId }),
  listCommentBank: (scope: "class_teacher" | "head_teacher") =>
    call<CommentBankEntry[]>("list_comment_bank", { scope }),
  addCommentToBank: (
    scope: "class_teacher" | "head_teacher",
    text: string,
    category?: string | null,
  ) => call<string>("add_comment_to_bank", { scope, text, category: category ?? null }),
  saveReportComment: (request: {
    studentId: string;
    termId: string;
    classTeacherComment?: string | null;
    headTeacherComment?: string | null;
  }) => call<void>("save_report_comment", { request }),
  setFeesBlock: (studentId: string, blocked: boolean, note?: string | null) =>
    call<void>("set_fees_block", { studentId, blocked, note: note ?? null }),
  setFeesRule: (enabled: boolean) => call<void>("set_fees_rule", { enabled }),
  getFeesRule: () => call<boolean>("get_fees_rule"),
};
