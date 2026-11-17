//! Application Tauri de Younji : elle ne fait que relier les commandes IPC aux crates métier
//! (ADR-004). Aucune logique métier ici.

mod commands;
mod error;
mod logging;
mod navigation;

use tauri::webview::NewWindowResponse;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// Nom de l'application côté Rust (ADR-014). Seule autre source : `productName` dans
/// `tauri.conf.json` et la clé i18n `app.name`.
pub const APP_NAME: &str = "Younji";

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

            // Le serveur Vite n'est autorisé qu'en développement.
            let dev_url = if cfg!(dev) {
                app.config().build.dev_url.clone()
            } else {
                None
            };

            // La fenêtre est créée ici plutôt que dans la configuration pour que son titre
            // vienne de APP_NAME. Elle ne charge que le contenu embarqué de l'application,
            // ne peut pas naviguer ailleurs et ne peut pas ouvrir d'autre fenêtre.
            WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::App("index.html".into()))
                .title(APP_NAME)
                .on_navigation(move |url| {
                    let allowed = navigation::is_allowed_navigation(url, dev_url.as_ref());
                    if !allowed {
                        // Seul le schéma est journalisé : l'adresse complète pourrait contenir
                        // des données sensibles.
                        tracing::warn!(scheme = url.scheme(), "navigation refusée");
                    }
                    allowed
                })
                .on_new_window(|url, _| {
                    tracing::warn!(scheme = url.scheme(), "ouverture de fenêtre refusée");
                    NewWindowResponse::Deny
                })
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
