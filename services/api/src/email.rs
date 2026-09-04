//! Outbound email via Resend.
//!
//! Sends are fire-and-forget: a booking must not fail because the mail
//! provider is slow, and the guest sees the booking row either way.

use crate::{Shared, users};

/// `(subject, html, text)` — the shape every template returns.
pub type Email = (String, String, String);

const RESEND_ENDPOINT: &str = "https://api.resend.com/emails";

/// Sends one message. Returns `Ok(())` without sending when no API key is
/// configured, which is the normal local-development path.
pub async fn send(state: &Shared, to: &str, mail: &Email) -> anyhow::Result<()> {
    let (subject, html, text) = mail;

    let Some(key) = state.cfg.resend_api_key.as_deref() else {
        tracing::info!(%subject, "RESEND_API_KEY unset — email not sent");
        tracing::debug!(%to, body = %text, "email contents");
        return Ok(());
    };

    let resp = state
        .http
        .post(RESEND_ENDPOINT)
        .bearer_auth(key)
        .json(&serde_json::json!({
            "from": state.cfg.email_from,
            "to": [to],
            "subject": subject,
            "html": html,
            "text": text,
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("resend returned {status}: {body}");
    }
    tracing::debug!(%to, %subject, "email sent");
    Ok(())
}

/// Queues a send on the runtime and returns immediately.
pub fn spawn(state: Shared, to: String, mail: Email) {
    tokio::spawn(async move {
        if let Err(e) = send(&state, &to, &mail).await {
            tracing::error!(error = ?e, subject = %mail.0, "email send failed");
        }
    });
}

/// Queues a send only when the address is configured.
pub fn spawn_opt(state: Shared, to: Option<String>, mail: Email) {
    match to {
        Some(addr) => spawn(state, addr, mail),
        None => tracing::warn!(subject = %mail.0, "no recipient configured — email skipped"),
    }
}

/// Queues the same message to several addresses.
pub fn spawn_all(state: Shared, to: Vec<String>, mail: Email) {
    if to.is_empty() {
        tracing::warn!(subject = %mail.0, "no recipient configured — email skipped");
        return;
    }
    for addr in to {
        spawn(state.clone(), addr, mail.clone());
    }
}

/// Who receives the owner's approve/deny mail, database first.
///
/// Never returns an error: a booking must not fail because the recipient
/// lookup did. A failed query is treated like "nobody is flagged", which
/// routes to the `OWNER_EMAIL` fallback rather than dropping the mail.
pub async fn owner_recipients(state: &Shared) -> Vec<String> {
    let flagged = match users::owner_emails(&state.db).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = ?e, "owner lookup failed — trying OWNER_EMAIL");
            Vec::new()
        }
    };
    resolve_owner_recipients(flagged, state.cfg.owner_email.as_deref())
}

/// Picks the recipient list from the flagged users and the env fallback.
///
/// Split out from [`owner_recipients`] so the precedence rules are testable
/// without a database.
pub fn resolve_owner_recipients(flagged: Vec<String>, fallback: Option<&str>) -> Vec<String> {
    let flagged: Vec<String> = flagged
        .into_iter()
        .filter(|e| !e.trim().is_empty())
        .collect();

    if !flagged.is_empty() {
        return flagged;
    }

    match fallback.map(str::trim).filter(|e| !e.is_empty()) {
        Some(addr) => {
            tracing::warn!(
                "no user is flagged is_owner — falling back to OWNER_EMAIL; \
                 flag the owner in the admin panel's Users tab"
            );
            vec![addr.to_string()]
        }
        None => {
            tracing::error!(
                "no user is flagged is_owner and OWNER_EMAIL is unset — \
                 nobody will receive booking approval mail"
            );
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_owner_recipients;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flagged_users_win_over_the_env_var() {
        let got = resolve_owner_recipients(v(&["jldugas@eatel.net"]), Some("stale@example.com"));
        assert_eq!(got, v(&["jldugas@eatel.net"]));
    }

    #[test]
    fn every_flagged_user_is_a_recipient() {
        let got = resolve_owner_recipients(
            v(&[
                "jldugas@eatel.net",
                "second@example.com",
                "third@example.com",
            ]),
            Some("stale@example.com"),
        );
        assert_eq!(
            got,
            v(&[
                "jldugas@eatel.net",
                "second@example.com",
                "third@example.com"
            ])
        );
    }

    #[test]
    fn falls_back_to_owner_email_when_nobody_is_flagged() {
        let got = resolve_owner_recipients(Vec::new(), Some("jldugas@eatel.net"));
        assert_eq!(got, v(&["jldugas@eatel.net"]));
    }

    #[test]
    fn no_flag_and_no_env_var_yields_nobody() {
        assert!(resolve_owner_recipients(Vec::new(), None).is_empty());
    }

    /// `Config::from_env` already blanks empty vars, but a whitespace-only
    /// value reaching here must not become a recipient.
    #[test]
    fn blank_addresses_are_not_recipients() {
        assert!(resolve_owner_recipients(v(&["  "]), Some("   ")).is_empty());
        assert_eq!(
            resolve_owner_recipients(v(&["", " "]), Some("jldugas@eatel.net")),
            v(&["jldugas@eatel.net"])
        );
    }
}
