//! Application Tauri de Vigie : elle ne fait que relier les commandes IPC aux crates métier
//! (ADR-004). Aucune logique métier ici.

mod commands;
mod error;
mod logging;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// Nom de l'application côté Rust (ADR-014). Seule autre source : `productName` dans
/// `tauri.conf.json` et la clé i18n `app.name`.
pub const APP_NAME: &str = "Vigie";

/// Étiquette de la fenêtre principale ; elle doit correspondre à `capabilities/main.json`.
const MAIN_WINDOW: &str = "main";

/// Démarre l'application.
pub fn run() {
    let result = tauri::Builder::default()
        .setup(|app| {
            // Sans dossier de journaux connu, on se replie sur la console : jamais sur un
            // dossier temporaire partagé, dont le nom serait prévisible pour un autre utilisateur.
            let guard = logging::init(app.path().app_log_dir().ok().as_deref());
            app.manage(guard);
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                "démarrage de {APP_NAME}"
            );

            // La fenêtre est créée ici plutôt que dans la configuration pour que son titre
            // vienne de APP_NAME. Elle ne charge que le contenu embarqué de l'application.
            WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::App("index.html".into()))
                .title(APP_NAME)
                .inner_size(1200.0, 780.0)
                .min_inner_size(800.0, 500.0)
                .center()
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::app::app_info])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        tracing::error!(%error, "arrêt sur erreur");
        eprintln!("{APP_NAME} n'a pas pu démarrer : {error}");
        std::process::exit(1);
    }
}
