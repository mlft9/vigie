//! Paramètres d'apparence persistants (APP-05) : `settings.json` dans le dossier de données.
//!
//! Ce fichier est en clair, car il doit être lisible **avant** le déverrouillage (langue et
//! thème de l'écran de déverrouillage). Il ne contient **aucune donnée sensible** ; le délai de
//! verrouillage automatique, lui, est stocké dans le coffre (authentifié). Les valeurs relues
//! sont validées : toute valeur inattendue est remplacée par la valeur par défaut.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Taille maximale lue.
const MAX_FILE_SIZE: u64 = 64 * 1024;
/// Bornes de la taille de police du terminal.
pub const MIN_FONT_SIZE: u32 = 8;
/// Taille de police maximale.
pub const MAX_FONT_SIZE: u32 = 32;
/// Taille de police par défaut.
pub const DEFAULT_FONT_SIZE: u32 = 14;

const LANGUAGES: [&str; 3] = ["system", "fr", "en"];
const THEMES: [&str; 3] = ["system", "light", "dark"];

/// Paramètres d'apparence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    /// `system`, `fr` ou `en`.
    pub language: String,
    /// `system`, `light` ou `dark`.
    pub theme: String,
    /// Taille de police du terminal, en pixels.
    pub terminal_font_size: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "system".into(),
            theme: "system".into(),
            terminal_font_size: DEFAULT_FONT_SIZE,
        }
    }
}

impl Settings {
    /// Vérifie chaque valeur.
    pub fn validate(&self) -> Result<(), AppError> {
        let valid = LANGUAGES.contains(&self.language.as_str())
            && THEMES.contains(&self.theme.as_str())
            && (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&self.terminal_font_size);
        if valid {
            Ok(())
        } else {
            Err(AppError::SETTINGS_INVALID)
        }
    }

    /// Relit un contenu non fiable : chaque champ invalide reprend sa valeur par défaut.
    fn from_untrusted(text: &str) -> Self {
        let defaults = Self::default();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
            return defaults;
        };
        let text_field = |name: &str, allowed: &[&str], fallback: String| {
            value
                .get(name)
                .and_then(serde_json::Value::as_str)
                .filter(|v| allowed.contains(v))
                .map_or(fallback, str::to_owned)
        };
        let font = value
            .get("terminalFontSize")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(v))
            .unwrap_or(defaults.terminal_font_size);
        Self {
            language: text_field("language", &LANGUAGES, defaults.language),
            theme: text_field("theme", &THEMES, defaults.theme),
            terminal_font_size: font,
        }
    }
}

/// Réponse de `settings_get` : paramètres, et indication d'un fichier encore absent (pour
/// reprendre les anciennes préférences du `localStorage`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    /// Paramètres actuels.
    pub settings: Settings,
    /// Vrai si aucun fichier n'a encore été écrit.
    pub first_run: bool,
}

/// Accès à `settings.json`.
pub struct SettingsStore {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl SettingsStore {
    /// Fichier à l'emplacement donné.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            write_lock: Mutex::new(()),
        }
    }

    /// Paramètres actuels (valeurs par défaut si le fichier est absent ou illisible).
    pub fn load(&self) -> SettingsDto {
        match read_bounded(&self.path) {
            Ok(Some(text)) => SettingsDto {
                settings: Settings::from_untrusted(&text),
                first_run: false,
            },
            Ok(None) => SettingsDto {
                settings: Settings::default(),
                first_run: true,
            },
            Err(error) => {
                tracing::warn!(%error, "settings.json illisible : valeurs par défaut");
                SettingsDto {
                    settings: Settings::default(),
                    first_run: false,
                }
            }
        }
    }

    /// Enregistre des paramètres validés, de façon atomique.
    pub fn save(&self, settings: &Settings) -> Result<(), AppError> {
        settings.validate()?;
        let _guard = self.write_lock.lock().map_err(|_| AppError::INTERNAL)?;
        let bytes = serde_json::to_vec_pretty(settings).map_err(|_| AppError::INTERNAL)?;
        write_atomic(&self.path, &bytes).map_err(|error| {
            tracing::warn!(%error, "écriture de settings.json impossible");
            AppError::SETTINGS_IO
        })
    }
}

fn read_bounded(path: &Path) -> io::Result<Option<String>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut text = String::new();
    file.take(MAX_FILE_SIZE).read_to_string(&mut text)?;
    Ok(Some(text))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        builder.mode(0o700);
        builder.create(dir)?;
        // Dossier de données de l'application : ramené à 0700 s'il existait déjà.
        if fs::metadata(dir)?.permissions().mode() & 0o777 != 0o700 {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
    }
    #[cfg(not(unix))]
    builder.create(dir)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults_and_first_run() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("settings.json"));
        let loaded = store.load();
        assert!(loaded.first_run);
        assert_eq!(loaded.settings, Settings::default());
    }

    #[test]
    fn saves_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("sub").join("settings.json"));
        let settings = Settings {
            language: "fr".into(),
            theme: "dark".into(),
            terminal_font_size: 16,
        };
        store.save(&settings).unwrap();
        let loaded = store.load();
        assert!(!loaded.first_run);
        assert_eq!(loaded.settings, settings);
    }

    #[test]
    fn invalid_values_are_refused_on_save() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("settings.json"));
        for settings in [
            Settings {
                language: "de".into(),
                ..Settings::default()
            },
            Settings {
                theme: "rose".into(),
                ..Settings::default()
            },
            Settings {
                terminal_font_size: 200,
                ..Settings::default()
            },
        ] {
            assert_eq!(store.save(&settings), Err(AppError::SETTINGS_INVALID));
        }
    }

    #[test]
    fn tampered_file_falls_back_field_by_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{"language":"en","theme":"<script>","terminalFontSize":9999,"extra":1}"#,
        )
        .unwrap();
        let loaded = SettingsStore::new(path.clone()).load().settings;
        assert_eq!(loaded.language, "en");
        assert_eq!(loaded.theme, "system");
        assert_eq!(loaded.terminal_font_size, DEFAULT_FONT_SIZE);

        fs::write(&path, "pas du json").unwrap();
        assert_eq!(
            SettingsStore::new(path).load().settings,
            Settings::default()
        );
    }

    #[test]
    fn unknown_fields_are_refused_from_the_interface() {
        let result = serde_json::from_value::<Settings>(serde_json::json!({
            "language": "fr", "theme": "dark", "terminalFontSize": 14, "autoLockMinutes": 0
        }));
        assert!(result.is_err());
    }
}
