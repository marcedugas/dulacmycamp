//! Branded transactional emails.
//!
//! Table-based layout with inline styles only — flexbox, grid and CSS custom
//! properties are unreliable across mail clients. Every template returns
//! `(subject, html, text)` so the plain-text part always mirrors the HTML.

use crate::{bookings::Booking, email::Email};

const GREEN: &str = "#2d5a27";
const BROWN: &str = "#8b5e3c";
const CREAM: &str = "#faf7f2";
const CHARCOAL: &str = "#1a1a1a";
const RED: &str = "#9b3226";
const FONT: &str = "Georgia,'Times New Roman',serif";
const SANS: &str = "system-ui,-apple-system,'Segoe UI',sans-serif";

/// Minimal HTML escape for interpolated, guest-supplied values.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Shared chrome: dark header with the wordmark, cream body card, footer.
fn layout(title: &str, body_html: &str, actions_html: &str) -> String {
    format!(
        r#"<!doctype html>
<html><body style="margin:0;padding:0;background:#efe9df;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#efe9df;padding:28px 14px;">
<tr><td align="center">
  <table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:520px;border-radius:14px;overflow:hidden;border:1px solid #ddd2c2;">
    <tr><td style="background:{CHARCOAL};padding:22px 26px;font-family:{FONT};">
      <span style="font-size:21px;font-weight:700;color:{GREEN};letter-spacing:.3px;">Dulac</span>
      <span style="font-size:21px;font-weight:700;color:{CREAM};letter-spacing:.3px;"> My Camp</span>
    </td></tr>
    <tr><td style="background:{CREAM};padding:28px 26px;font-family:{SANS};">
      <h1 style="margin:0 0 16px;font-family:{FONT};font-size:20px;font-weight:700;color:{CHARCOAL};">{title}</h1>
      <div style="font-size:15px;line-height:1.65;color:{CHARCOAL};">{body_html}</div>
      {actions_html}
    </td></tr>
    <tr><td style="background:#efe9df;padding:16px 26px;font-family:{SANS};border-top:1px solid #ddd2c2;">
      <p style="margin:0;font-size:12px;color:#6b6255;">&copy; 2026 Dulac My Camp</p>
    </td></tr>
  </table>
</td></tr></table>
</body></html>"#
    )
}

/// A labelled detail row inside the body card.
fn row(label: &str, value: &str) -> String {
    format!(
        r#"<tr>
  <td style="padding:5px 14px 5px 0;font-size:13px;color:#6b6255;white-space:nowrap;">{label}</td>
  <td style="padding:5px 0;font-size:14px;color:{CHARCOAL};font-weight:600;">{value}</td>
</tr>"#
    )
}

fn button(label: &str, url: &str, color: &str) -> String {
    format!(
        r#"<td style="padding-right:10px;"><table role="presentation" cellpadding="0" cellspacing="0"><tr>
  <td style="border-radius:8px;background:{color};">
    <a href="{url}" style="display:inline-block;padding:13px 28px;font-family:{SANS};font-size:15px;font-weight:700;color:#ffffff;text-decoration:none;">{label}</a>
  </td></tr></table></td>"#
    )
}

fn nights(b: &Booking) -> i64 {
    (b.check_out - b.check_in).num_days()
}

fn pretty(d: chrono::NaiveDate) -> String {
    d.format("%a, %b %-d, %Y").to_string()
}

/// The booking facts shared by the owner and admin notification emails.
fn booking_rows(b: &Booking, guest: &str) -> String {
    let n = nights(b);
    let plural = if n == 1 { "night" } else { "nights" };
    let mut rows = String::new();
    rows.push_str(&row("Guest", &esc(guest)));
    rows.push_str(&row(
        "Dates",
        &format!(
            "{} &rarr; {} <span style=\"font-weight:400;color:#6b6255;\">({n} {plural})</span>",
            pretty(b.check_in),
            pretty(b.check_out)
        ),
    ));
    rows.push_str(&row("Adults", &b.guest_count_adults.to_string()));
    rows.push_str(&row("Kids", &b.guest_count_kids.to_string()));
    rows.push_str(&row("Pets", if b.has_pets { "Yes" } else { "No" }));
    if let Some(r) = b.other_requests.as_deref().filter(|s| !s.trim().is_empty()) {
        rows.push_str(&row("Requests", &esc(r)));
    }
    format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:6px 0 4px;">{rows}</table>"#
    )
}

fn booking_text(b: &Booking, guest: &str) -> String {
    let mut s = format!(
        "Guest: {guest}\nDates: {} - {} ({} nights)\nAdults: {}  Kids: {}\nPets: {}\n",
        pretty(b.check_in),
        pretty(b.check_out),
        nights(b),
        b.guest_count_adults,
        b.guest_count_kids,
        if b.has_pets { "Yes" } else { "No" },
    );
    if let Some(r) = b.other_requests.as_deref().filter(|v| !v.trim().is_empty()) {
        s.push_str(&format!("Requests: {r}\n"));
    }
    s
}

// ─────────────────────────── templates ───────────────────────────

pub fn otp_email(code: &str, ttl_minutes: i64) -> Email {
    let subject = "Your Dulac My Camp login code".to_string();
    let body = format!(
        r#"<p style="margin:0 0 18px;">Your login code is:</p>
<p style="margin:0 0 18px;font-family:{FONT};font-size:38px;letter-spacing:9px;font-weight:700;color:{GREEN};">{code}</p>
<p style="margin:0;color:#6b6255;font-size:14px;">This code expires in {ttl_minutes} minutes. If you didn't ask for it, you can ignore this email.</p>"#
    );
    let text = format!(
        "Your Dulac My Camp login code\n\nYour login code is: {code}\n\nThis code expires in {ttl_minutes} minutes.\n\n(c) 2026 Dulac My Camp\n"
    );
    (subject, layout("Sign in to the camp", &body, ""), text)
}

/// Sent to the camp owner. This is the only email with action buttons.
pub fn booking_request_to_owner(
    b: &Booking,
    guest: &str,
    approve_url: &str,
    deny_url: &str,
) -> Email {
    let subject = format!("{guest} requested dates at the camp");
    let body = format!(
        r#"<p style="margin:0 0 14px;">A new request came in for the camp.</p>{}"#,
        booking_rows(b, guest)
    );
    let actions = format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:24px 0 10px;"><tr>{}{}</tr></table>
<p style="margin:6px 0 0;font-size:12px;color:#6b6255;">Buttons are valid for 48 hours.</p>"#,
        button("Approve", approve_url, GREEN),
        button("Deny", deny_url, RED),
    );
    let text = format!(
        "{guest} requested dates at the camp\n\n{}\nApprove: {approve_url}\nDeny: {deny_url}\n\nButtons are valid for 48 hours.\n",
        booking_text(b, guest)
    );
    (
        subject,
        layout("New booking request", &body, &actions),
        text,
    )
}

/// Informational copy for the admin — no action buttons, admin uses the app.
pub fn booking_request_to_admin(b: &Booking, guest: &str, app_url: &str) -> Email {
    let subject = format!("New booking request from {guest}");
    let body = format!(
        r#"<p style="margin:0 0 14px;">The owner has been emailed for approval. This copy is for your records.</p>{}"#,
        booking_rows(b, guest)
    );
    let actions = format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:22px 0 4px;"><tr>{}</tr></table>"#,
        button("Open admin panel", &format!("{app_url}/admin"), BROWN)
    );
    let text = format!(
        "New booking request from {guest}\n\n{}\nAdmin panel: {app_url}/admin\n",
        booking_text(b, guest)
    );
    (
        subject,
        layout("New booking request", &body, &actions),
        text,
    )
}

pub fn booking_pending_to_guest(b: &Booking, app_url: &str) -> Email {
    let subject = "Booking request received — Dulac My Camp".to_string();
    let body = format!(
        r#"<p style="margin:0 0 14px;">We received your booking request for <strong>{} &rarr; {}</strong>. The owner will review it shortly and you'll hear back soon!</p>"#,
        pretty(b.check_in),
        pretty(b.check_out)
    );
    let actions = format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:22px 0 4px;"><tr>{}</tr></table>"#,
        button("View my bookings", &format!("{app_url}/my-bookings"), GREEN)
    );
    let text = format!(
        "Booking request received\n\nWe received your booking request for {} - {}. The owner will review it shortly and you'll hear back soon!\n\n{app_url}/my-bookings\n",
        pretty(b.check_in),
        pretty(b.check_out)
    );
    (subject, layout("Request received", &body, &actions), text)
}

/// `checkin_items` is pulled at send time (see `checkin_info::all_for_email`)
/// so the email always reflects current content, never a snapshot from
/// whenever this template was written.
pub fn booking_confirmed_to_guest(
    b: &Booking,
    checkin_items: &[(String, String)],
    app_url: &str,
) -> Email {
    let subject = "Your camp booking is confirmed! 🎣".to_string();
    let mut body = format!(
        r#"<p style="margin:0 0 14px;">Great news! Your booking at Dulac My Camp has been approved.</p>
<p style="margin:0 0 14px;font-size:17px;"><strong>{} &rarr; {}</strong></p>
<p style="margin:0;">See you at the camp!</p>"#,
        pretty(b.check_in),
        pretty(b.check_out)
    );
    let mut text = format!(
        "Your camp booking is confirmed!\n\nGreat news! Your booking at Dulac My Camp has been approved.\nDates: {} - {}\n\nSee you at the camp!\n",
        pretty(b.check_in),
        pretty(b.check_out)
    );

    if !checkin_items.is_empty() {
        let mut items_html = String::new();
        let mut items_text = String::new();
        for (title, item_body) in checkin_items {
            items_html.push_str(&format!(
                r#"<div style="margin:0 0 14px;"><p style="margin:0 0 3px;font-weight:700;color:{CHARCOAL};">{}</p><p style="margin:0;color:#4a443b;">{}</p></div>"#,
                esc(title),
                esc(item_body)
            ));
            items_text.push_str(&format!("{title}\n{item_body}\n\n"));
        }
        body.push_str(&format!(
            r#"<hr style="margin:20px 0;border:none;border-top:1px solid #ddd2c2;">
<p style="margin:0 0 14px;font-weight:700;color:{CHARCOAL};">Here's what you need for your stay:</p>
{items_html}
<p style="margin:0;color:#6b6255;font-size:14px;">You can also find this anytime by logging in and visiting My Stay.</p>"#
        ));
        text.push_str(&format!(
            "\nHere's what you need for your stay:\n\n{items_text}You can also find this anytime by logging in and visiting My Stay.\n"
        ));
    }

    let actions = format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:22px 0 4px;"><tr>{}</tr></table>"#,
        button("View my bookings", &format!("{app_url}/my-bookings"), GREEN)
    );
    (subject, layout("Booking confirmed", &body, &actions), text)
}

pub fn booking_denied_to_guest(b: &Booking, reason: Option<&str>, app_url: &str) -> Email {
    let subject = "Booking update — Dulac My Camp".to_string();
    let reason_html = reason
        .filter(|r| !r.trim().is_empty())
        .map(|r| {
            format!(
                r#"<p style="margin:0 0 14px;padding:12px 14px;background:#efe9df;border-left:3px solid {BROWN};border-radius:4px;">{}</p>"#,
                esc(r)
            )
        })
        .unwrap_or_default();
    let body = format!(
        r#"<p style="margin:0 0 14px;">Unfortunately your booking request for <strong>{} &rarr; {}</strong> was not approved.</p>
{reason_html}
<p style="margin:0;">Feel free to request different dates!</p>"#,
        pretty(b.check_in),
        pretty(b.check_out)
    );
    let actions = format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:22px 0 4px;"><tr>{}</tr></table>"#,
        button("Pick new dates", &format!("{app_url}/book"), GREEN)
    );
    let text = format!(
        "Booking update\n\nUnfortunately your booking request for {} - {} was not approved.\n{}\nFeel free to request different dates! {app_url}/book\n",
        pretty(b.check_in),
        pretty(b.check_out),
        reason
            .filter(|r| !r.trim().is_empty())
            .map(|r| format!("\nReason: {r}\n"))
            .unwrap_or_default(),
    );
    (subject, layout("Booking update", &body, &actions), text)
}

/// Sent to the owner and admin when a guest cancels.
pub fn booking_cancelled_notice(b: &Booking, guest: &str) -> Email {
    let subject = format!("{guest} cancelled a camp booking");
    let body = format!(
        r#"<p style="margin:0 0 14px;">This booking has been cancelled and the dates are free again.</p>{}"#,
        booking_rows(b, guest)
    );
    let text = format!(
        "{guest} cancelled a camp booking\n\n{}",
        booking_text(b, guest)
    );
    (subject, layout("Booking cancelled", &body, ""), text)
}

/// Standalone confirmation page rendered after a one-click owner action.
/// Deliberately self-contained: the owner is in their mail client, not the app.
pub fn action_result_page(heading: &str, detail: &str, ok: bool) -> String {
    let accent = if ok { GREEN } else { BROWN };
    let mark = if ok { "&check;" } else { "&times;" };
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Dulac My Camp</title></head>
<body style="margin:0;background:#efe9df;font-family:{SANS};">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="padding:56px 16px;">
<tr><td align="center">
  <table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:460px;border-radius:14px;overflow:hidden;border:1px solid #ddd2c2;">
    <tr><td style="background:{CHARCOAL};padding:20px 26px;font-family:{FONT};">
      <span style="font-size:19px;font-weight:700;color:{GREEN};">Dulac</span><span style="font-size:19px;font-weight:700;color:{CREAM};"> My Camp</span>
    </td></tr>
    <tr><td style="background:{CREAM};padding:36px 26px;text-align:center;">
      <div style="font-size:40px;color:{accent};line-height:1;">{mark}</div>
      <h1 style="margin:14px 0 10px;font-family:{FONT};font-size:22px;color:{CHARCOAL};">{heading}</h1>
      <p style="margin:0;font-size:15px;line-height:1.6;color:#4a443b;">{detail}</p>
    </td></tr>
    <tr><td style="background:#efe9df;padding:14px 26px;border-top:1px solid #ddd2c2;">
      <p style="margin:0;font-size:12px;color:#6b6255;">&copy; 2026 Dulac My Camp</p>
    </td></tr>
  </table>
</td></tr></table>
</body></html>"#
    )
}

// ─────────────────────────── checkout / journal ───────────────────────────

/// Sent to the admin only when a guest actually flags something at
/// checkout — routine checkouts with nothing to report generate no email.
pub fn checkout_notes_to_admin(
    guest: &str,
    check_in: chrono::NaiveDate,
    check_out: chrono::NaiveDate,
    notes: &str,
    app_url: &str,
) -> Email {
    let subject = format!("{guest} flagged something at checkout");
    let mut rows = String::new();
    rows.push_str(&row("Guest", &esc(guest)));
    rows.push_str(&row(
        "Dates",
        &format!("{} &rarr; {}", pretty(check_in), pretty(check_out)),
    ));
    let body = format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:6px 0 16px;">{rows}</table>
<p style="margin:0 0 6px;font-size:13px;color:#6b6255;">Note</p>
<p style="margin:0;padding:12px 14px;background:#efe9df;border-left:3px solid {BROWN};border-radius:4px;">{}</p>"#,
        esc(notes)
    );
    let actions = format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:22px 0 4px;"><tr>{}</tr></table>"#,
        button("Open admin panel", &format!("{app_url}/admin"), BROWN)
    );
    let text = format!(
        "{guest} flagged something at checkout\n\nGuest: {guest}\nDates: {} - {}\n\nNote:\n{notes}\n\n{app_url}/admin\n",
        pretty(check_in),
        pretty(check_out),
    );
    (subject, layout("Checkout note", &body, &actions), text)
}

/// Informational — no one-click buttons, unlike booking approval. The admin
/// should read the story before deciding, not act blind from an email.
pub fn journal_submitted_to_admin(
    guest: &str,
    check_in: chrono::NaiveDate,
    check_out: chrono::NaiveDate,
    title: &str,
    app_url: &str,
) -> Email {
    let subject = format!("{guest} submitted a journal entry");
    let mut rows = String::new();
    rows.push_str(&row("Guest", &esc(guest)));
    rows.push_str(&row(
        "Stay",
        &format!("{} &rarr; {}", pretty(check_in), pretty(check_out)),
    ));
    rows.push_str(&row("Title", &esc(title)));
    let body = format!(
        r#"<p style="margin:0 0 14px;">A new camp journal story is waiting for review.</p><table role="presentation" cellpadding="0" cellspacing="0" style="margin:6px 0 4px;">{rows}</table>"#
    );
    let actions = format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:22px 0 4px;"><tr>{}</tr></table>"#,
        button("Review in admin panel", &format!("{app_url}/admin"), BROWN)
    );
    let text = format!(
        "{guest} submitted a journal entry\n\nGuest: {guest}\nStay: {} - {}\nTitle: {title}\n\n{app_url}/admin\n",
        pretty(check_in),
        pretty(check_out),
    );
    (subject, layout("New journal entry", &body, &actions), text)
}

pub fn journal_approved_to_guest(app_url: &str) -> Email {
    let subject = "Your journal entry is live! 📖".to_string();
    let body = r#"<p style="margin:0 0 14px;">Your story from your stay at Dulac My Camp is now posted in the camp journal.</p>"#.to_string();
    let actions = format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:22px 0 4px;"><tr>{}</tr></table>"#,
        button("View journal", &format!("{app_url}/journal"), GREEN)
    );
    let text = format!(
        "Your journal entry is live!\n\nYour story from your stay at Dulac My Camp is now posted in the camp journal.\n\n{app_url}/journal\n"
    );
    (subject, layout("Story posted", &body, &actions), text)
}

/// Deliberately warm, not bureaucratic — this is a private camp, not a
/// moderated public forum.
pub fn journal_rejected_to_guest(reason: Option<&str>, app_url: &str) -> Email {
    let subject = "About your journal entry".to_string();
    let reason_line = reason
        .filter(|r| !r.trim().is_empty())
        .map(|r| format!(" {}", esc(r)))
        .unwrap_or_default();
    let body = format!(
        r#"<p style="margin:0 0 14px;">Thanks for sharing your story from the camp. We didn't post this one.{reason_line}</p>
<p style="margin:0;">Feel free to submit again!</p>"#
    );
    let actions = format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:22px 0 4px;"><tr>{}</tr></table>"#,
        button("My bookings", &format!("{app_url}/my-bookings"), GREEN)
    );
    let text = format!(
        "About your journal entry\n\nThanks for sharing your story from the camp. We didn't post this one.{}\n\nFeel free to submit again!\n{app_url}/my-bookings\n",
        reason
            .filter(|r| !r.trim().is_empty())
            .map(|r| format!(" {r}"))
            .unwrap_or_default(),
    );
    (subject, layout("About your story", &body, &actions), text)
}

/// The optional-reason form shown by `GET /api/bookings/deny/{token}`.
pub fn deny_form_page(action_url: &str, guest: &str, dates: &str) -> String {
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Deny booking — Dulac My Camp</title></head>
<body style="margin:0;background:#efe9df;font-family:{SANS};">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="padding:56px 16px;">
<tr><td align="center">
  <table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:460px;border-radius:14px;overflow:hidden;border:1px solid #ddd2c2;">
    <tr><td style="background:{CHARCOAL};padding:20px 26px;font-family:{FONT};">
      <span style="font-size:19px;font-weight:700;color:{GREEN};">Dulac</span><span style="font-size:19px;font-weight:700;color:{CREAM};"> My Camp</span>
    </td></tr>
    <tr><td style="background:{CREAM};padding:30px 26px;">
      <h1 style="margin:0 0 8px;font-family:{FONT};font-size:21px;color:{CHARCOAL};">Deny this request?</h1>
      <p style="margin:0 0 20px;font-size:15px;color:#4a443b;">{guest} &middot; {dates}</p>
      <form method="post" action="{action_url}">
        <label style="display:block;font-size:13px;color:#6b6255;margin-bottom:6px;">Reason (optional — the guest will see this)</label>
        <textarea name="reason" rows="3" style="width:100%;box-sizing:border-box;padding:10px;font-family:{SANS};font-size:14px;border:1px solid #ddd2c2;border-radius:8px;background:#fff;color:{CHARCOAL};"></textarea>
        <button type="submit" style="margin-top:16px;width:100%;padding:13px;border:0;border-radius:8px;background:{RED};color:#fff;font-size:15px;font-weight:700;cursor:pointer;">Deny booking</button>
      </form>
    </td></tr>
    <tr><td style="background:#efe9df;padding:14px 26px;border-top:1px solid #ddd2c2;">
      <p style="margin:0;font-size:12px;color:#6b6255;">&copy; 2026 Dulac My Camp</p>
    </td></tr>
  </table>
</td></tr></table>
</body></html>"#
    )
}
