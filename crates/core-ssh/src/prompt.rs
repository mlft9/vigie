//! Questions du serveur en authentification keyboard-interactive (AUTH-03) : codes TOTP,
//! mot de passe PAM…
//!
//! Le texte vient du serveur, donc d'une source non fiable (SECURITE.md §4.2, M3) : il est
//! nettoyé de tout caractère de contrôle et borné avant d'être transmis à l'interface, qui
//! l'affiche comme du texte brut. Le nombre de questions et de tours est lui aussi borné.

use secrecy::{ExposeSecret, SecretString};

use crate::SshError;
use crate::auth::MAX_SECRET_LEN;

/// Longueur maximale du nom d'une demande, en octets.
pub const MAX_PROMPT_NAME_LEN: usize = 256;
/// Longueur maximale des instructions d'une demande, en octets.
pub const MAX_PROMPT_INSTRUCTIONS_LEN: usize = 2048;
/// Longueur maximale d'une question, en octets.
pub const MAX_PROMPT_TEXT_LEN: usize = 512;
/// Nombre maximal de questions dans une même demande.
pub const MAX_PROMPTS: usize = 8;
/// Nombre maximal de demandes (tours) pour une authentification.
pub const MAX_PROMPT_ROUNDS: usize = 16;

/// Une question du serveur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptField {
    /// Texte de la question, nettoyé.
    pub text: String,
    /// `true` si la réponse peut s'afficher pendant la saisie (le serveur le décide).
    pub echo: bool,
}

/// Demande du serveur : un tour de keyboard-interactive. Ne contient aucun secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPrompt {
    /// Nom de la demande (souvent vide), nettoyé.
    pub name: String,
    /// Instructions (souvent vides), nettoyées ; les retours à la ligne sont conservés.
    pub instructions: String,
    /// Questions, dans l'ordre attendu des réponses.
    pub fields: Vec<PromptField>,
}

impl AuthPrompt {
    /// Construit une demande à partir du texte brut du serveur. Refuse une demande qui
    /// dépasse [`MAX_PROMPTS`] questions.
    pub fn from_server(
        name: &str,
        instructions: &str,
        prompts: impl IntoIterator<Item = (String, bool)>,
    ) -> Result<Self, SshError> {
        let mut fields = Vec::new();
        for (text, echo) in prompts {
            if fields.len() == MAX_PROMPTS {
                return Err(SshError::AuthPromptInvalid);
            }
            fields.push(PromptField {
                text: clean(&text, MAX_PROMPT_TEXT_LEN, false),
                echo,
            });
        }
        Ok(Self {
            name: clean(name, MAX_PROMPT_NAME_LEN, false),
            instructions: clean(instructions, MAX_PROMPT_INSTRUCTIONS_LEN, true),
            fields,
        })
    }

    /// Vrai si la demande est une question unique, sans écho, qui demande exactement le mot
    /// de passe (« Password: », « Mot de passe : »). Seul cas où le mot de passe enregistré
    /// peut être envoyé sans intervention de l'utilisateur.
    pub fn is_single_password_question(&self) -> bool {
        match self.fields.as_slice() {
            [field] if !field.echo => is_password_text(&field.text),
            _ => false,
        }
    }
}

/// Reconnaissance volontairement stricte : toute variante (« New password », « Password for
/// root », « Verification code »…) est soumise à l'utilisateur.
fn is_password_text(text: &str) -> bool {
    let normalized = text
        .trim_end_matches(|c: char| c == ':' || c.is_whitespace())
        .trim_start()
        .to_lowercase();
    normalized == "password" || normalized == "mot de passe"
}

/// Caractères invisibles ou de mise en forme (catégories Unicode Cf, Zl, Zp, liste explicite)
/// qui peuvent masquer, inverser ou mettre en page du texte pour imiter l'interface.
pub(crate) fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

/// Retire les caractères de contrôle (échappements ANSI, OSC…) et borne la longueur en
/// octets, sans couper un caractère. `multiline` conserve les retours à la ligne.
fn clean(text: &str, max: usize, multiline: bool) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let keep = match c {
            '\n' if multiline => Some('\n'),
            '\t' => Some(' '),
            c if c.is_control() || is_invisible(c) => None,
            c => Some(c),
        };
        if let Some(c) = keep {
            if out.len() + c.len_utf8() > max {
                break;
            }
            out.push(c);
        }
    }
    out
}

/// Réponses de l'utilisateur à une demande. Pas de `Debug` qui expose les secrets.
pub struct PromptAnswers(Vec<SecretString>);

impl std::fmt::Debug for PromptAnswers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PromptAnswers({} réponse(s), <masquées>)", self.0.len())
    }
}

impl PromptAnswers {
    /// Valide des réponses : chacune fait au plus [`MAX_SECRET_LEN`] octets (une réponse vide
    /// est permise), et il y en a au plus [`MAX_PROMPTS`].
    pub fn new(answers: Vec<SecretString>) -> Result<Self, SshError> {
        if answers.len() > MAX_PROMPTS
            || answers
                .iter()
                .any(|a| a.expose_secret().len() > MAX_SECRET_LEN)
        {
            return Err(SshError::InvalidAnswers);
        }
        Ok(Self(answers))
    }

    /// Nombre de réponses.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Vrai s'il n'y a aucune réponse.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Copie des réponses pour russh, qui n'accepte que des `String` ordinaires (même limite
    /// que pour le mot de passe : cette copie n'est pas effacée par nous).
    pub(crate) fn to_russh(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|a| a.expose_secret().to_owned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(fields: &[(&str, bool)]) -> AuthPrompt {
        AuthPrompt::from_server("", "", fields.iter().map(|(t, e)| ((*t).to_owned(), *e))).unwrap()
    }

    #[test]
    fn server_text_is_stripped_of_control_sequences() {
        let p = AuthPrompt::from_server(
            "nom\x1b[31m",
            "ligne 1\r\nligne\t2\x1b]52;c;ZXZpbA==\x07",
            [(
                "Co\u{00AD}de\u{202E} \u{2028}\u{FEFF}\u{E0041}:\x08".to_owned(),
                false,
            )],
        )
        .unwrap();
        assert_eq!(p.name, "nom[31m");
        assert_eq!(p.instructions, "ligne 1\nligne 2]52;c;ZXZpbA==");
        assert_eq!(p.fields[0].text, "Code :");
    }

    #[test]
    fn server_text_is_bounded_without_splitting_characters() {
        let long = "é".repeat(MAX_PROMPT_TEXT_LEN);
        let p = AuthPrompt::from_server(&long, &long, [(long.clone(), true)]).unwrap();
        assert!(p.name.len() <= MAX_PROMPT_NAME_LEN);
        assert!(p.fields[0].text.len() <= MAX_PROMPT_TEXT_LEN);
        assert!(p.instructions.len() <= MAX_PROMPT_INSTRUCTIONS_LEN);
        assert!(p.fields[0].text.chars().all(|c| c == 'é'));
    }

    #[test]
    fn too_many_questions_are_refused() {
        let many = (0..=MAX_PROMPTS).map(|i| (format!("q{i}"), false));
        assert_eq!(
            AuthPrompt::from_server("", "", many).unwrap_err(),
            SshError::AuthPromptInvalid
        );
        let enough = (0..MAX_PROMPTS).map(|i| (format!("q{i}"), false));
        assert!(AuthPrompt::from_server("", "", enough).is_ok());
    }

    #[test]
    fn only_plain_password_questions_are_auto_filled() {
        for text in ["Password: ", "password:", "  Password", "Mot de passe : "] {
            assert!(
                prompt(&[(text, false)]).is_single_password_question(),
                "{text}"
            );
        }
        for text in [
            "New password: ",
            "Retype new password: ",
            "Password for root: ",
            "Verification code: ",
            "Passphrase: ",
            "(Password)",
        ] {
            assert!(
                !prompt(&[(text, false)]).is_single_password_question(),
                "{text}"
            );
        }
        // Avec écho, ou accompagnée d'une autre question : jamais.
        assert!(!prompt(&[("Password:", true)]).is_single_password_question());
        assert!(!prompt(&[("Password:", false), ("Code:", false)]).is_single_password_question());
        assert!(!prompt(&[]).is_single_password_question());
    }

    #[test]
    fn answers_are_validated_and_masked() {
        let secret = |s: &str| SecretString::from(s.to_owned());
        let answers = PromptAnswers::new(vec![secret("123456"), secret("")]).unwrap();
        assert_eq!(answers.len(), 2);
        assert!(!format!("{answers:?}").contains("123456"));
        assert_eq!(
            PromptAnswers::new(vec![secret(&"x".repeat(MAX_SECRET_LEN + 1))]).unwrap_err(),
            SshError::InvalidAnswers
        );
        let too_many = (0..=MAX_PROMPTS).map(|_| secret("a")).collect();
        assert_eq!(
            PromptAnswers::new(too_many).unwrap_err(),
            SshError::InvalidAnswers
        );
    }
}
