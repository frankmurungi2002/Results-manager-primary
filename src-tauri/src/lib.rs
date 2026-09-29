//! Phantom School Manager — offline-first school management for Ugandan nursery and
//! primary schools.
//!
//! The backend owns everything that matters: the database, who is signed in,
//! what they may touch, and how a document gets branded. The webview is a
//! rendering surface with no credentials and no direct database access — every
//! request crosses the IPC boundary as a named, permission-checked command.

pub mod audit;
pub mod backup;
pub mod commands;
pub mod db;
pub mod domain;
pub mod error;
pub mod security;
pub mod sms;
pub mod state;

use tauri::Manager;

use crate::state::{AppPaths, AppState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("could not find a place to store data: {e}"))?;

            let paths = AppPaths::resolve(data_dir)
                .map_err(|e| format!("could not prepare the data folder: {e}"))?;

            log::info!("data directory: {}", paths.data_dir.display());

            let state = AppState::new(paths)
                .map_err(|e| format!("could not open the school database: {e}"))?;

            app.manage(state);
            backup::spawn_scheduler(app.handle().clone());
            sms::spawn_scheduler(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // --- Session (FR-B1, FR-C1, FR-C2) -----------------------------
            commands::auth::startup_state,
            commands::auth::sign_in,
            commands::auth::sign_out,
            commands::auth::current_session,
            commands::auth::set_teacher_mode,
            commands::auth::change_password,
            commands::auth::reset_user_password,
            // --- First run --------------------------------------------------
            commands::setup::setup_defaults,
            commands::setup::complete_setup,
            // --- Institution and system (FR-B2, FR-B9, FR-B10, FR-E2) -------
            commands::system::get_institution,
            commands::system::update_institution,
            commands::system::set_institution_logo,
            commands::system::get_institution_logo,
            commands::system::get_login_images,
            commands::system::set_login_image,
            commands::system::get_theme,
            commands::system::set_theme,
            commands::system::search_audit_log,
            commands::system::backup_status,
            commands::system::set_mirror_path,
            commands::system::run_backup,
            commands::system::list_backups,
            commands::system::inspect_backup,
            commands::system::restore_backup,
            commands::system::dashboard_summary,
            // --- Academic structure (FR-B3, FR-B5, FR-B7, FR-C13) ----------
            commands::academics::list_classes,
            commands::academics::save_class,
            commands::academics::retire_class,
            commands::academics::list_subjects,
            commands::academics::save_subject,
            commands::academics::list_class_subjects,
            commands::academics::add_class_subject,
            commands::academics::update_class_subject,
            commands::academics::retire_class_subject,
            commands::academics::list_grading_systems,
            commands::academics::save_grading_system,
            commands::academics::list_academic_years,
            commands::academics::open_term,
            commands::academics::close_term,
            commands::academics::get_features,
            commands::academics::set_feature,
            // --- Staff (FR-B4, FR-B6) ---------------------------------------
            commands::teachers::list_staff,
            commands::teachers::create_staff,
            commands::teachers::retire_staff,
            commands::teachers::list_assignments,
            commands::teachers::assign_teacher,
            commands::teachers::unassign_teacher,
            // --- Learners (FR-C7, FR-C8, FR-C10) ----------------------------
            commands::students::list_class_roster,
            commands::students::search_students,
            commands::students::save_student,
            commands::students::drop_student,
            commands::students::readmit_student,
            commands::students::transfer_student,
            commands::students::set_student_photo,
            commands::students::get_student_photo,
            // --- Marks (FR-C3, FR-C5, FR-C6) --------------------------------
            commands::marks::load_marks_sheet,
            commands::marks::save_marks,
            commands::marks::marks_progress,
            commands::marks::subject_analytics,
            commands::marks::set_marks_deadline,
            commands::marks::get_marks_deadline,
            // --- Insights: questions nobody could ask before -----------------
            commands::insights::ple_projection,
            commands::insights::subject_heatmap,
            // --- Onboarding import (FR-G13) ---------------------------------
            commands::importer::read_spreadsheet,
            commands::importer::import_learners,
            commands::importer::read_image_file,
            // --- Outputs (FR-D1, FR-D2, FR-D4, FR-B8, FR-G11) ---------------
            commands::reports::build_report_cards,
            commands::reports::build_class_list,
            commands::reports::list_comment_bank,
            commands::reports::add_comment_to_bank,
            commands::reports::save_report_comment,
            commands::reports::get_report_comment,
            commands::reports::build_id_cards,
            commands::reports::build_pass_out_slip,
            commands::reports::build_exam_permits,
            // --- Daily attendance register (FR-G9) -------------------------
            commands::attendance::load_register,
            commands::attendance::save_register,
            commands::attendance::attendance_overview,
            commands::attendance::attendance_month,
            commands::reports::build_attendance_register,
            // --- Class timetables (FR-G6) and the exam timetable (FR-G7) ---
            commands::timetable::get_timetable_setup,
            commands::timetable::save_timetable_setup,
            commands::timetable::load_class_timetable,
            commands::timetable::save_timetable_slot,
            commands::timetable::set_lessons_per_week,
            commands::timetable::clear_timetable,
            commands::timetable::auto_fill_timetable,
            commands::timetable::load_teacher_timetable,
            commands::timetable::build_timetable_document,
            commands::timetable::list_exam_papers,
            commands::timetable::save_exam_paper,
            commands::timetable::delete_exam_paper,
            commands::timetable::auto_generate_exam_timetable,
            commands::timetable::build_exam_timetable,
            // --- Optional features: streams (FR-C11), weekly work (FR-C12) --
            commands::features::list_streams,
            commands::features::save_stream,
            commands::features::retire_stream,
            commands::features::set_student_stream,
            commands::features::load_weekly_sheet,
            commands::features::save_weekly_scores,
            // --- Pass-outs (FR-G22) and SMS (FR-G8) ------------------------
            commands::passouts::list_pass_outs,
            commands::passouts::create_pass_out,
            commands::passouts::mark_pass_out_returned,
            commands::passouts::get_sms_settings,
            commands::passouts::save_sms_settings,
            commands::passouts::list_sms_outbox,
            commands::passouts::send_queued_sms,
            commands::passouts::send_test_sms,
            commands::teachers::set_staff_photo,
            commands::teachers::get_staff_photo,
            commands::reports::get_report_settings,
            commands::reports::save_report_settings,
            commands::reports::set_fees_block,
            commands::reports::set_fees_rule,
            commands::reports::get_fees_rule,
        ])
        .build(tauri::generate_context!())
        .expect("Phantom School Manager failed to start")
        .run(|app, event| {
            // A last snapshot on the way out, if anything changed since the
            // scheduled one (SRS 16.1: every fifteen minutes and on close).
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Err(err) = backup::take_if_changed(&state, backup::Trigger::OnExit) {
                        log::error!("backup on exit failed: {err}");
                    }
                }
            }
        });
}
