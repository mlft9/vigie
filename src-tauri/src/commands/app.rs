//! Informations générales sur l'application.

use serde::Serialize;

use crate::APP_NAME;

/// Réponse de `app_info` (miroir de `AppInfo` dans `src/lib/types.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: &'static str,
    version: &'static str,
}

fn current_app_info() -> AppInfo {
    AppInfo {
        name: APP_NAME,
        version: env!("CARGO_PKG_VERSION"),
    }
}

/// Renvoie le nom et la version de l'application. Aucune entrée, aucune donnée sensible.
#[tauri::command]
pub fn app_info() -> AppInfo {
    current_app_info()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_name_and_version() {
        let json = serde_json::to_value(current_app_info()).unwrap();
        assert_eq!(json["name"], "Vigie");
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json.as_object().unwrap().len(), 2);
    }
}
