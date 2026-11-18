//! Commandes des paramètres d'apparence (APP-05). Aucune donnée sensible.

use tauri::State;

use crate::error::AppError;
use crate::settings::{Settings, SettingsDto, SettingsStore};

/// Paramètres actuels (lisibles avant le déverrouillage).
#[tauri::command]
pub fn settings_get(store: State<'_, SettingsStore>) -> SettingsDto {
    store.load()
}

/// Enregistre les paramètres, après validation de chaque valeur.
#[tauri::command]
pub fn settings_set(settings: Settings, store: State<'_, SettingsStore>) -> Result<(), AppError> {
    store.save(&settings)
}
