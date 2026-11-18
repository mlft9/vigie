//! Ouverture d'un lien dans le navigateur du système (liens du terminal, SECURITE.md §4.2).
//!
//! L'interface est non fiable : l'adresse est revalidée ici. Seuls `http:` et `https:` sont
//! admis (jamais `file:`, `javascript:`, schémas d'applications…), avec un hôte, sans
//! identifiants intégrés (`user:pass@`, qui servent à déguiser une adresse), et de longueur
//! raisonnable. C'est l'adresse normalisée par l'analyseur qui est transmise au système, en
//! argument d'un processus (jamais interprétée par un shell).

use tauri::Url;

use crate::error::AppError;

/// Longueur maximale d'une adresse ouverte.
const MAX_URL_LEN: usize = 2048;

/// Valide une adresse et renvoie sa forme normalisée.
pub fn validate_external_url(raw: &str) -> Result<String, AppError> {
    if raw.len() > MAX_URL_LEN || raw.chars().any(char::is_control) {
        return Err(AppError::URL_INVALID);
    }
    let url = Url::parse(raw).map_err(|_| AppError::URL_INVALID)?;
    let valid = matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some_and(|h| !h.is_empty())
        && url.username().is_empty()
        && url.password().is_none();
    if !valid {
        return Err(AppError::URL_INVALID);
    }
    let normalized = url.to_string();
    if normalized.len() > MAX_URL_LEN {
        return Err(AppError::URL_INVALID);
    }
    Ok(normalized)
}

/// Ouvre un lien `http(s)` dans le navigateur par défaut.
#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), AppError> {
    let url = validate_external_url(&url)?;
    tauri::async_runtime::spawn_blocking(move || open::that_detached(url))
        .await
        .map_err(|_| AppError::INTERNAL)?
        .map_err(|error| {
            tracing::warn!(%error, "ouverture du navigateur impossible");
            AppError::URL_OPEN_FAILED
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_and_https() {
        assert_eq!(
            validate_external_url("https://example.com/a?b=c&d=e#f").unwrap(),
            "https://example.com/a?b=c&d=e#f"
        );
        assert_eq!(
            validate_external_url("http://192.168.1.10:8080").unwrap(),
            "http://192.168.1.10:8080/"
        );
        // Le nom d'hôte est normalisé (punycode), ce qui révèle les homographes.
        assert_eq!(
            validate_external_url("https://exаmple.com/").unwrap(),
            "https://xn--exmple-4nf.com/"
        );
    }

    #[test]
    fn rejects_dangerous_or_malformed_urls() {
        let long = format!("https://example.com/{}", "a".repeat(MAX_URL_LEN));
        for bad in [
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "ms-settings:",
            "ssh://host",
            "ftp://example.com",
            "https://user:pass@example.com",
            "https://user@example.com",
            "https://exa\nmple.com",
            "\\\\server\\share",
            "C:\\Windows\\notepad.exe",
            "example.com",
            "",
            long.as_str(),
        ] {
            assert_eq!(
                validate_external_url(bad),
                Err(AppError::URL_INVALID),
                "{bad:?}"
            );
        }
    }
}
