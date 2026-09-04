//! Outbound email via Resend.
//!
//! Sends are fire-and-forget: a booking must not fail because the mail
//! provider is slow, and the guest sees the booking row either way.

use crate::Shared;

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
