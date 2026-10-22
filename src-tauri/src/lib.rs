use std::{path::PathBuf, sync::Mutex};

use perfboard_core::{BoardSize, ComponentDefinition, ProjectId, parse_component_library};
use perfboard_project::{
    Board, OperationRequest, OperationResult, ProjectDocument, apply_operation, load_project,
    save_project_atomic,
};
use serde::Serialize;

struct SessionState(Mutex<ProjectDocument>);

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct BootstrapInfo {
    pub app_name: &'static str,
    pub schema_version: u32,
}

#[tauri::command]
fn get_bootstrap_info() -> BootstrapInfo {
    BootstrapInfo {
        app_name: "Perfboard Designer",
        schema_version: perfboard_project::PROJECT_SCHEMA_VERSION.0,
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn get_project(state: tauri::State<'_, SessionState>) -> Result<ProjectDocument, String> {
    state
        .0
        .lock()
        .map_err(|error| error.to_string())
        .map(|value| value.clone())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn create_project(
    state: tauri::State<'_, SessionState>,
    name: String,
    holes_x: u32,
    holes_y: u32,
    pitch_mm: f64,
) -> Result<ProjectDocument, String> {
    let size = BoardSize::new(holes_x, holes_y).map_err(|error| error.to_string())?;
    let document = ProjectDocument::new(
        ProjectId::new(),
        name,
        Board::new(format!("{holes_x}x{holes_y} perfboard"), size, pitch_mm),
    );
    document.validate().map_err(|error| error.to_string())?;
    *state.0.lock().map_err(|error| error.to_string())? = document.clone();
    Ok(document)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn apply_project_operation(
    state: tauri::State<'_, SessionState>,
    request: OperationRequest,
) -> Result<OperationResult, String> {
    let mut document = state.0.lock().map_err(|error| error.to_string())?;
    apply_operation(&mut document, request).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_component_library() -> Result<Vec<ComponentDefinition>, String> {
    parse_component_library([
        include_str!("../../components/arduino-nano-like/component.toml"),
        include_str!("../../components/rs485-breakout/component.toml"),
    ])
    .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn save_project(state: tauri::State<'_, SessionState>, path: PathBuf) -> Result<(), String> {
    let document = state.0.lock().map_err(|error| error.to_string())?;
    save_project_atomic(&document, path).map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn open_project(
    state: tauri::State<'_, SessionState>,
    path: PathBuf,
) -> Result<ProjectDocument, String> {
    let document = load_project(path).map_err(|error| error.to_string())?;
    *state.0.lock().map_err(|error| error.to_string())? = document.clone();
    Ok(document)
}

fn initial_document() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new(),
        "Untitled project",
        Board::new(
            "25x18 perfboard",
            BoardSize::new(25, 18).expect("valid size"),
            2.54,
        ),
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the desktop application and its command bridge.
///
/// # Panics
/// Panics when Tauri cannot initialize or run the desktop event loop.
pub fn run() {
    tauri::Builder::default()
        .manage(SessionState(Mutex::new(initial_document())))
        .invoke_handler(tauri::generate_handler![
            get_bootstrap_info,
            get_project,
            create_project,
            apply_project_operation,
            get_component_library,
            save_project,
            open_project,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Perfboard Designer");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_command_reports_shared_schema_version() {
        assert_eq!(
            get_bootstrap_info(),
            BootstrapInfo {
                app_name: "Perfboard Designer",
                schema_version: perfboard_project::PROJECT_SCHEMA_VERSION.0
            }
        );
    }

    #[test]
    fn bundled_component_library_is_valid() {
        assert_eq!(get_component_library().unwrap().len(), 2);
    }
}
