//! Garde de navigation de la fenêtre principale.
//!
//! La CSP ne contrôle pas la navigation de premier niveau : sans cette garde, un lien ou un
//! script pourrait charger une page distante dans la fenêtre de Vigie (qui pourrait par exemple
//! imiter la demande du mot de passe maître). Seule l'origine de l'application est autorisée ;
//! les liens externes seront ouverts dans le navigateur du système (plugin `opener`, plus tard).

use tauri::Url;

/// Origines du contenu embarqué : `tauri://localhost` (Linux) et `http(s)://tauri.localhost`
/// (Windows, WebView2).
fn is_embedded_app_url(url: &Url) -> bool {
    match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "http" | "https" => url.host_str() == Some("tauri.localhost"),
        _ => false,
    }
}

/// Indique si la fenêtre principale peut naviguer vers `url`.
///
/// `dev_url` est l'adresse du serveur Vite : elle n'est fournie qu'en développement.
pub fn is_allowed_navigation(url: &Url, dev_url: Option<&Url>) -> bool {
    if is_embedded_app_url(url) {
        return true;
    }
    dev_url.is_some_and(|dev| dev.origin() == url.origin())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn allows_the_embedded_application() {
        for allowed in [
            "tauri://localhost/",
            "tauri://localhost/index.html",
            "http://tauri.localhost/",
            "https://tauri.localhost/index.html#settings",
        ] {
            assert!(is_allowed_navigation(&url(allowed), None), "{allowed}");
        }
    }

    #[test]
    fn rejects_remote_and_lookalike_urls() {
        for denied in [
            "https://evil.example/",
            "http://localhost/",
            "http://tauri.localhost.evil.example/",
            "http://evil.example/?tauri.localhost",
            "http://tauri.localhost@evil.example/",
            "tauri://evil.example/",
            "file:///C:/Windows/System32/",
            "javascript:alert(1)",
            "data:text/html,<h1>faux</h1>",
            "about:blank",
            "ipc://localhost/app_info",
        ] {
            assert!(!is_allowed_navigation(&url(denied), None), "{denied}");
        }
    }

    #[test]
    fn allows_the_dev_server_only_when_provided() {
        let dev = url("http://127.0.0.1:1420");
        assert!(is_allowed_navigation(
            &url("http://127.0.0.1:1420/"),
            Some(&dev)
        ));
        assert!(!is_allowed_navigation(&url("http://127.0.0.1:1420/"), None));
        // Même hôte, autre port : refusé.
        assert!(!is_allowed_navigation(
            &url("http://127.0.0.1:8080/"),
            Some(&dev)
        ));
    }
}
