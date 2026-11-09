use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UnsubscribeMethod {
    OneClickPost,
    Mailto,
    WebUrl,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsubscribeInfo {
    pub method: UnsubscribeMethod,
    pub target: Option<String>,
    pub post_body: Option<String>,
}

/// Extract clean URLs or mailto entries from List-Unsubscribe header
/// RFC 2369: List-Unsubscribe: <mailto:list-request@example.com?subject=unsubscribe>, <https://example.com/unsub>
pub fn extract_unsubscribe_links(header_val: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut in_bracket = false;
    let mut current = String::new();

    for ch in header_val.chars() {
        match ch {
            '<' => {
                in_bracket = true;
                current.clear();
            }
            '>' => {
                if in_bracket {
                    in_bracket = false;
                    let trimmed = current.trim();
                    if !trimmed.is_empty() {
                        links.push(trimmed.to_string());
                    }
                    current.clear();
                }
            }
            _ => {
                if in_bracket {
                    current.push(ch);
                }
            }
        }
    }

    // Fallback if no <> found: split by comma or whitespace
    if links.is_empty() {
        for part in header_val.split(',') {
            let trimmed = part.trim().trim_matches(|c| c == '<' || c == '>');
            if trimmed.starts_with("http://")
                || trimmed.starts_with("https://")
                || trimmed.starts_with("mailto:")
            {
                links.push(trimmed.to_string());
            }
        }
    }

    links
}

/// Parse unsubscribe headers following RFC 8058 & RFC 2369 priorities:
/// Priority 1: One-Click Unsubscribe (RFC 8058: List-Unsubscribe-Post == One-Click + https URL)
/// Priority 2: Mailto (RFC 2369: mailto:... to send direct unsubscription email)
/// Priority 3: HTTPS / HTTP web URL (standard web unsubscription page)
pub fn parse_unsubscribe_headers(
    list_unsubscribe: Option<&str>,
    list_unsubscribe_post: Option<&str>,
) -> UnsubscribeInfo {
    let unsub = match list_unsubscribe {
        Some(s) if !s.trim().is_empty() => s.trim(),
        _ => {
            return UnsubscribeInfo {
                method: UnsubscribeMethod::None,
                target: None,
                post_body: None,
            }
        }
    };

    let links = extract_unsubscribe_links(unsub);
    let is_one_click_post = list_unsubscribe_post
        .map(|p| p.trim().eq_ignore_ascii_case("List-Unsubscribe=One-Click"))
        .unwrap_or(false);

    let https_link = links
        .iter()
        .find(|l| l.to_lowercase().starts_with("https://"))
        .cloned();

    let http_link = links
        .iter()
        .find(|l| l.to_lowercase().starts_with("http://"))
        .cloned();

    let mailto_link = links
        .iter()
        .find(|l| l.to_lowercase().starts_with("mailto:"))
        .cloned();

    // Priority 1: RFC 8058 One-Click POST
    if is_one_click_post && https_link.is_some() {
        return UnsubscribeInfo {
            method: UnsubscribeMethod::OneClickPost,
            target: https_link,
            post_body: Some("List-Unsubscribe=One-Click".to_string()),
        };
    }

    // Priority 2: RFC 2369 Mailto
    if let Some(mailto) = mailto_link {
        return UnsubscribeInfo {
            method: UnsubscribeMethod::Mailto,
            target: Some(mailto),
            post_body: None,
        };
    }

    // Priority 3: Web URL (HTTPS preferred over HTTP)
    if let Some(web_url) = https_link.or(http_link) {
        return UnsubscribeInfo {
            method: UnsubscribeMethod::WebUrl,
            target: Some(web_url),
            post_body: None,
        };
    }

    UnsubscribeInfo {
        method: UnsubscribeMethod::None,
        target: None,
        post_body: None,
    }
}

/// Execute unsubscription based on detected method.
pub fn execute_unsubscribe(info: &UnsubscribeInfo, dry_run: bool) -> Result<String, String> {
    tracing::info!(method = ?info.method, dry_run, "Exécution de la demande de désabonnement");
    match info.method {
        UnsubscribeMethod::None => {
            Err("Aucun mécanisme de désabonnement valide détecté".to_string())
        }
        UnsubscribeMethod::OneClickPost => {
            let target = info
                .target
                .as_deref()
                .ok_or_else(|| "URL One-Click manquante".to_string())?;

            if dry_run {
                return Ok(format!(
                    "[Simulation] Requête POST One-Click (RFC 8058) simulée vers {}",
                    target
                ));
            }

            let resp = ureq::post(target)
                .set("Content-Type", "application/x-www-form-urlencoded")
                .send_string("List-Unsubscribe=One-Click")
                .map_err(|e| format!("Échec de la désinscription One-Click HTTP: {e}"))?;

            Ok(format!(
                "Désinscription One-Click effectuée avec succès (Code HTTP: {})",
                resp.status()
            ))
        }
        UnsubscribeMethod::Mailto => {
            let target = info
                .target
                .as_deref()
                .ok_or_else(|| "Lien mailto manquant".to_string())?;

            if dry_run {
                return Ok(format!(
                    "[Simulation] Ouverture du client mail simulée pour {}",
                    target
                ));
            }

            open::that(target).map_err(|e| format!("Impossible d'ouvrir le client mail: {e}"))?;
            Ok(format!(
                "Votre client de messagerie par défaut a été ouvert pour envoyer l'email de désabonnement à {}",
                target
            ))
        }
        UnsubscribeMethod::WebUrl => {
            let target = info
                .target
                .as_deref()
                .ok_or_else(|| "URL web manquante".to_string())?;

            if dry_run {
                return Ok(format!(
                    "[Simulation] Ouverture du navigateur web simulée pour {}",
                    target
                ));
            }

            open::that(target)
                .map_err(|e| format!("Impossible d'ouvrir le navigateur web: {e}"))?;
            Ok(format!(
                "La page de désabonnement a été ouverte dans votre navigateur : {}",
                target
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rfc_8058_one_click() {
        let unsub_header =
            "<https://news.fnac.com/unsubscribe?token=abc123xyz>, <mailto:unsub@fnac.com>";
        let post_header = "List-Unsubscribe=One-Click";

        let info = parse_unsubscribe_headers(Some(unsub_header), Some(post_header));
        assert_eq!(info.method, UnsubscribeMethod::OneClickPost);
        assert_eq!(
            info.target,
            Some("https://news.fnac.com/unsubscribe?token=abc123xyz".to_string())
        );
        assert_eq!(
            info.post_body,
            Some("List-Unsubscribe=One-Click".to_string())
        );
    }

    #[test]
    fn test_rfc_2369_mailto_priority() {
        let unsub_header = "<mailto:desabonnement@boulanger.fr?subject=unsubscribe>, <https://boulanger.fr/mon-compte>";
        // No List-Unsubscribe-Post header
        let info = parse_unsubscribe_headers(Some(unsub_header), None);
        assert_eq!(info.method, UnsubscribeMethod::Mailto);
        assert_eq!(
            info.target,
            Some("mailto:desabonnement@boulanger.fr?subject=unsubscribe".to_string())
        );
    }

    #[test]
    fn test_web_url_priority() {
        let unsub_header = "<https://amazon.co.uk/communication-preferences>";
        let info = parse_unsubscribe_headers(Some(unsub_header), None);
        assert_eq!(info.method, UnsubscribeMethod::WebUrl);
        assert_eq!(
            info.target,
            Some("https://amazon.co.uk/communication-preferences".to_string())
        );
    }

    #[test]
    fn test_empty_or_missing() {
        let info = parse_unsubscribe_headers(None, None);
        assert_eq!(info.method, UnsubscribeMethod::None);
        assert!(info.target.is_none());
    }

    #[test]
    fn test_dry_run_execution() {
        let info = UnsubscribeInfo {
            method: UnsubscribeMethod::OneClickPost,
            target: Some("https://example.com/unsub".to_string()),
            post_body: Some("List-Unsubscribe=One-Click".to_string()),
        };
        let result = execute_unsubscribe(&info, true);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("[Simulation]"));
    }
}
