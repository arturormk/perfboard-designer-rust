mod commands {
    use serde::Serialize;

    #[derive(Debug, Eq, PartialEq, Serialize)]
    pub struct BootstrapInfo {
        pub app_name: &'static str,
        pub schema_version: u32,
    }

    #[tauri::command]
    pub fn get_bootstrap_info() -> BootstrapInfo {
        BootstrapInfo {
            app_name: "Perfboard Designer",
            schema_version: perfboard_project::PROJECT_SCHEMA_VERSION.0,
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the desktop application and its command bridge.
///
/// # Panics
///
/// Panics when Tauri cannot initialize or run the desktop event loop.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::get_bootstrap_info])
        .run(tauri::generate_context!())
        .expect("failed to run Perfboard Designer");
}

#[cfg(test)]
mod tests {
    use super::commands::{BootstrapInfo, get_bootstrap_info};

    #[test]
    fn bootstrap_command_reports_shared_schema_version() {
        assert_eq!(
            get_bootstrap_info(),
            BootstrapInfo {
                app_name: "Perfboard Designer",
                schema_version: perfboard_project::PROJECT_SCHEMA_VERSION.0,
            }
        );
    }
}
