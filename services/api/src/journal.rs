//! The camp journal: a per-stay memory log tied 1:1 to a booking — the
//! story, what was caught, and photos from the trip. Deliberately not a
//! review system — no ratings, no stars, anywhere in this module or its
//! responses.
//!
//! Entries are self-published. There is no approval queue: what an author
//! writes is live the moment they write it, scoped by the `visibility` they
//! chose ([`VISIBILITY_PUBLIC`] or [`VISIBILITY_FAMILY`]). Admins and the
//! camp owner moderate after the fact — edit, archive, or delete — rather
//! than gating every story on the way in.
//!
//! "Public" here means *any registered account*, not the open internet: the
//! feed requires a login, so there is no anonymous tier to leak into.

use crate::{
    ApiResult, AppError, Shared,
    auth::AuthUser,
    email, email_templates, notifications,
    uploads::{bad_multipart, delete_upload_file, save_upload},
    users::{self, User},
};
use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

fn app_url(state: &Shared) -> &str {
    state.cfg.frontend_url.trim_end_matches('/')
}

/// Readable by any registered account.
pub const VISIBILITY_PUBLIC: &str = "public";
/// Readable only by family (`user` role) accounts, plus admins and the owner.
pub const VISIBILITY_FAMILY: &str = "family";

/// Mirrors `journal_entries_visibility_valid` in migration 0016.
pub const VISIBILITIES: [&str; 2] = [VISIBILITY_PUBLIC, VISIBILITY_FAMILY];

fn validate_visibility(visibility: &str) -> ApiResult<()> {
    if !VISIBILITIES.contains(&visibility) {
        return Err(AppError::BadRequest(format!(
            "'{visibility}' is not a visibility. Valid options: {}.",
            VISIBILITIES.join(", ")
        )));
    }
    Ok(())
}

// ─────────────────────────── eligibility ───────────────────────────

/// Whether a booking qualifies to receive a journal entry: approved, the
/// stay has *started* (`check_in <= today` — mid-stay, departure day, or
/// well after all count), and no entry exists for it yet. Whether checkout
/// has happened is irrelevant — a guest can start writing any time after
/// arrival, not only once they're on their way out. Pure mirror of the rule
/// `create()` and `eligible_bookings()` both enforce, kept here as the
/// tested spec of it.
pub fn is_journal_eligible(
    status: &str,
    check_in: NaiveDate,
    today: NaiveDate,
    has_existing_entry: bool,
) -> bool {
    status == "approved" && check_in <= today && !has_existing_entry
}

/// `create()` calls this once the booking is loaded — approved, but the
/// stay hasn't started yet.
fn require_stay_started(check_in: NaiveDate, today: NaiveDate) -> ApiResult<()> {
    if check_in > today {
        return Err(AppError::BadRequest(
            "You can share a story once your stay has started.".into(),
        ));
    }
    Ok(())
}

/// Only an approved booking was ever actually confirmed to happen.
fn require_approved(status: &str) -> ApiResult<()> {
    if status != "approved" {
        return Err(AppError::BadRequest(
            "Only approved stays can be journaled about.".into(),
        ));
    }
    Ok(())
}

/// Guards against journaling the same stay twice.
fn require_no_existing_entry(already: bool) -> ApiResult<()> {
    if already {
        return Err(AppError::Conflict(
            "You've already shared a story for this stay.".into(),
        ));
    }
    Ok(())
}

// ─────────────────────────── who may see what ───────────────────────────

/// A viewer's standing relative to one entry, reduced to the three facts the
/// rule actually turns on.
#[derive(Debug, Clone, Copy)]
pub struct ViewerContext {
    pub is_author: bool,
    /// Admin or camp owner — the moderation bypass.
    pub moderates: bool,
    /// The `user` (family) role. Admins and the owner come through
    /// `moderates` instead, so this stays a plain role check.
    pub is_family: bool,
}

impl ViewerContext {
    fn of(viewer: &User, author_id: Uuid) -> Self {
        Self {
            is_author: viewer.id == author_id,
            moderates: moderates(viewer),
            is_family: viewer.role == "user",
        }
    }
}

/// Admins and the camp owner see and moderate everything, the same bypass
/// the calendar gives them over private bookings.
fn moderates(user: &User) -> bool {
    user.is_admin() || user.is_owner
}

fn require_moderator(user: &User) -> ApiResult<()> {
    if !moderates(user) {
        return Err(AppError::Forbidden(
            "Only an admin or the camp owner can do that.".into(),
        ));
    }
    Ok(())
}

/// The whole visibility rule, in one place.
///
/// An author always sees their own entry — any visibility, archived or not —
/// because it is theirs and hiding it from them would just look like data
/// loss. A moderator sees everything for the same reason they can edit it.
/// Everyone else sees live entries their role reaches.
///
/// [`feed_where_clause`] is the SQL form of exactly this; the two are
/// checked against each other in the tests below.
pub fn may_view(v: ViewerContext, visibility: &str, archived: bool) -> bool {
    if v.is_author || v.moderates {
        return true;
    }
    if archived {
        return false;
    }
    match visibility {
        VISIBILITY_PUBLIC => true,
        VISIBILITY_FAMILY => v.is_family,
        _ => false,
    }
}

/// The SQL twin of [`may_view`], as a `WHERE` fragment over `je`.
///
/// `$1` is the viewer's id, `$2` whether they moderate, `$3` whether they are
/// family. Kept as one string so the feed and its `count(*)` cannot drift.
fn feed_where_clause() -> &'static str {
    "(je.user_id = $1
      OR $2
      OR (je.archived_at IS NULL
          AND (je.visibility = 'public' OR ($3 AND je.visibility = 'family'))))"
}

// ─────────────────────────── rows ───────────────────────────

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct JournalEntry {
    pub id: Uuid,
    pub user_id: Uuid,
    pub booking_id: Uuid,
    pub title: String,
    pub body: String,
    pub visibility: String,
    /// Set when a moderator has quietly hidden this entry. Independent of
    /// visibility: an archived entry is hidden from everyone but its author
    /// and the moderators, whatever it says it is.
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const ENTRY_COLUMNS: &str =
    "id, user_id, booking_id, title, body, visibility, archived_at, created_at, updated_at";

/// One logged catch. Every measurement is optional — "a mess of trout" is a
/// perfectly good catch log.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CatchRow {
    pub id: Uuid,
    pub species_id: Option<Uuid>,
    /// Resolved from `fish_species`, so a renamed species reads correctly
    /// everywhere without touching the catch rows.
    pub species_name: Option<String>,
    pub length_inches: Option<f64>,
    pub weight_lbs: Option<f64>,
    pub quantity: i32,
    pub notes: Option<String>,
    pub sort_order: i32,
    /// Photos attached to this catch specifically. Filled by [`hydrate`]
    /// from a separate query, never selected alongside the row itself —
    /// hence `skip`, which keeps it out of the decode entirely.
    #[sqlx(skip)]
    pub photos: Vec<PhotoRow>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PhotoRow {
    pub id: Uuid,
    pub url: String,
    pub caption: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    /// `None` is a general entry photo — the gallery's, and the only kind
    /// that existed before migration 0017. `Some` means it belongs to that
    /// catch row and travels with it instead.
    pub journal_catch_id: Option<Uuid>,
}

/// A child row plus the entry it hangs off, so one query can fetch them for
/// a whole page of entries and [`hydrate`] can sort them out afterwards.
#[derive(FromRow)]
struct Owned<T> {
    journal_entry_id: Uuid,
    #[sqlx(flatten)]
    child: T,
}

async fn catches_for(db: &PgPool, entry_ids: &[Uuid]) -> Result<Vec<Owned<CatchRow>>, sqlx::Error> {
    sqlx::query_as::<_, Owned<CatchRow>>(
        "SELECT c.journal_entry_id, c.id, c.species_id, s.name AS species_name,
                c.length_inches, c.weight_lbs, c.quantity, c.notes, c.sort_order
         FROM journal_catches c
         LEFT JOIN fish_species s ON s.id = c.species_id
         WHERE c.journal_entry_id = ANY($1)
         ORDER BY c.sort_order, c.id",
    )
    .bind(entry_ids)
    .fetch_all(db)
    .await
}

async fn photos_for(db: &PgPool, entry_ids: &[Uuid]) -> Result<Vec<Owned<PhotoRow>>, sqlx::Error> {
    sqlx::query_as::<_, Owned<PhotoRow>>(
        "SELECT journal_entry_id, id, url, caption, sort_order, created_at, journal_catch_id
         FROM journal_photos
         WHERE journal_entry_id = ANY($1)
         ORDER BY sort_order, created_at",
    )
    .bind(entry_ids)
    .fetch_all(db)
    .await
}

/// Attaches each entry's catches and photos in two queries rather than two
/// per entry.
///
/// Photos are partitioned rather than listed twice: one tagged with a
/// `journal_catch_id` travels inside that catch, and only untagged ones
/// reach the entry's own gallery. Nothing renders in both places, and a
/// client that only knows about `photos` still sees exactly the general
/// gallery it always did.
async fn hydrate<T, F>(db: &PgPool, items: &mut [T], id_of: F) -> Result<(), sqlx::Error>
where
    F: Fn(&T) -> Uuid,
    T: Hydratable,
{
    let ids: Vec<Uuid> = items.iter().map(&id_of).collect();
    if ids.is_empty() {
        return Ok(());
    }
    let catches = catches_for(db, &ids).await?;
    let photos = photos_for(db, &ids).await?;

    for item in items.iter_mut() {
        let id = id_of(item);
        item.set_catches(
            catches
                .iter()
                .filter(|o| o.journal_entry_id == id)
                .map(|o| {
                    let mut c = o.child.clone();
                    c.photos = photos
                        .iter()
                        .filter(|p| p.child.journal_catch_id == Some(c.id))
                        .map(|p| p.child.clone())
                        .collect();
                    c
                })
                .collect(),
        );
        item.set_photos(
            photos
                .iter()
                .filter(|o| o.journal_entry_id == id && o.child.journal_catch_id.is_none())
                .map(|o| o.child.clone())
                .collect(),
        );
    }
    Ok(())
}

/// Lets [`hydrate`] fill any response shape that carries catches and photos.
trait Hydratable {
    fn set_catches(&mut self, catches: Vec<CatchRow>);
    fn set_photos(&mut self, photos: Vec<PhotoRow>);
}

// ─────────────────────────── feed ───────────────────────────

#[derive(Debug, Serialize)]
pub struct FeedEntry {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub visibility: String,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub guest_first_name: String,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
    /// So the reader's own entries can be labelled and offered an edit link
    /// without the client having to know its own user id.
    pub is_mine: bool,
    pub catches: Vec<CatchRow>,
    pub photos: Vec<PhotoRow>,
}

impl Hydratable for FeedEntry {
    fn set_catches(&mut self, catches: Vec<CatchRow>) {
        self.catches = catches;
    }
    fn set_photos(&mut self, photos: Vec<PhotoRow>) {
        self.photos = photos;
    }
}

#[derive(FromRow)]
struct FeedRow {
    id: Uuid,
    user_id: Uuid,
    title: String,
    body: String,
    visibility: String,
    archived_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    full_name: Option<String>,
    check_in: NaiveDate,
    check_out: NaiveDate,
}

const PAGE_SIZE: i64 = 10;

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    pub page: Option<i64>,
}

/// `GET /api/journal` — **requires a login.**
///
/// This endpoint used to be anonymous. It no longer can be: "public" now
/// means "any registered account" rather than "the open internet", so there
/// is no tier left that an anonymous caller belongs to. See [`may_view`] for
/// what each role gets back.
pub async fn list_feed(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Query(q): Query<PageQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PAGE_SIZE;
    let moderator = moderates(&user);
    let is_family = user.role == "user";

    let rows = sqlx::query_as::<_, FeedRow>(&format!(
        "SELECT je.id, je.user_id, je.title, je.body, je.visibility, je.archived_at,
                je.created_at, je.updated_at, u.full_name, b.check_in, b.check_out
         FROM journal_entries je
         JOIN users u ON u.id = je.user_id
         JOIN bookings b ON b.id = je.booking_id
         WHERE {where_clause}
         ORDER BY je.created_at DESC
         LIMIT $4 OFFSET $5",
        where_clause = feed_where_clause()
    ))
    .bind(user.id)
    .bind(moderator)
    .bind(is_family)
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let (total,): (i64,) = sqlx::query_as(&format!(
        "SELECT count(*) FROM journal_entries je WHERE {where_clause}",
        where_clause = feed_where_clause()
    ))
    .bind(user.id)
    .bind(moderator)
    .bind(is_family)
    .fetch_one(&state.db)
    .await?;

    let mut entries: Vec<FeedEntry> = rows
        .into_iter()
        .map(|r| FeedEntry {
            id: r.id,
            title: r.title,
            body: r.body,
            visibility: r.visibility,
            archived_at: r.archived_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
            guest_first_name: users::first_name(r.full_name.as_deref()),
            check_in: r.check_in,
            check_out: r.check_out,
            is_mine: r.user_id == user.id,
            catches: vec![],
            photos: vec![],
        })
        .collect();
    hydrate(&state.db, &mut entries, |e| e.id).await?;

    Ok(Json(json!({
        "entries": entries,
        "page": page,
        "page_size": PAGE_SIZE,
        "total": total,
        "total_pages": (total as f64 / PAGE_SIZE as f64).ceil().max(1.0) as i64,
        "moderator": moderator,
    })))
}

// ─────────────────────────── the author's own ───────────────────────────

/// An entry as its author (or a moderator) works on it: the whole thing,
/// catches and photos included, so the edit form can seed itself from one
/// request.
#[derive(Debug, Serialize)]
pub struct FullEntry {
    #[serde(flatten)]
    pub entry: JournalEntry,
    pub catches: Vec<CatchRow>,
    pub photos: Vec<PhotoRow>,
}

impl Hydratable for FullEntry {
    fn set_catches(&mut self, catches: Vec<CatchRow>) {
        self.catches = catches;
    }
    fn set_photos(&mut self, photos: Vec<PhotoRow>) {
        self.photos = photos;
    }
}

impl FullEntry {
    fn of(entry: JournalEntry) -> Self {
        Self {
            entry,
            catches: vec![],
            photos: vec![],
        }
    }
}

async fn load_full(db: &PgPool, id: Uuid) -> ApiResult<FullEntry> {
    let entry = sqlx::query_as::<_, JournalEntry>(&format!(
        "SELECT {ENTRY_COLUMNS} FROM journal_entries WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("Journal entry not found.".into()))?;

    let mut one = [FullEntry::of(entry)];
    hydrate(db, &mut one, |e| e.entry.id).await?;
    let [full] = one;
    Ok(full)
}

/// `GET /api/journal/mine` — every entry the caller has ever written.
pub async fn list_mine(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Vec<FullEntry>>> {
    let rows = sqlx::query_as::<_, JournalEntry>(&format!(
        "SELECT {ENTRY_COLUMNS} FROM journal_entries WHERE user_id = $1 ORDER BY created_at DESC"
    ))
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;

    let mut entries: Vec<FullEntry> = rows.into_iter().map(FullEntry::of).collect();
    hydrate(&state.db, &mut entries, |e| e.entry.id).await?;
    Ok(Json(entries))
}

/// `GET /api/journal/{id}` — one entry, if the caller may see it.
pub async fn get_one(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<FullEntry>> {
    let full = load_full(&state.db, id).await?;
    let ctx = ViewerContext::of(&user, full.entry.user_id);
    if !may_view(
        ctx,
        &full.entry.visibility,
        full.entry.archived_at.is_some(),
    ) {
        // Indistinguishable from a genuinely missing entry on purpose: a
        // family-only story should not be discoverable by id.
        return Err(AppError::NotFound("Journal entry not found.".into()));
    }
    Ok(Json(full))
}

#[derive(Debug, Serialize, FromRow)]
pub struct EligibleBooking {
    pub booking_id: Uuid,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
}

/// `GET /api/journal/eligible-bookings` — approved, started stays with no
/// journal entry yet (see [`is_journal_eligible`]). Drives `/journal/new`.
pub async fn eligible_bookings(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Vec<EligibleBooking>>> {
    let rows = sqlx::query_as::<_, EligibleBooking>(
        "SELECT b.id AS booking_id, b.check_in, b.check_out
         FROM bookings b
         WHERE b.user_id = $1 AND b.status = 'approved' AND b.check_in <= $2
           AND NOT EXISTS (SELECT 1 FROM journal_entries je WHERE je.booking_id = b.id)
         ORDER BY b.check_out DESC",
    )
    .bind(user.id)
    .bind(Utc::now().date_naive())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// One catch as submitted from the form.
///
/// `id` is the id of a row already saved against this entry, and it is how a
/// catch keeps its identity across a save. That matters now that photos hang
/// off `journal_catches.id`: a save that deleted and reinserted every row
/// would cascade away every attached photo along with it. An `id` that names
/// no row of *this* entry is ignored rather than adopted, so a submitted id
/// can never reach across entries. Omitted means a brand-new row.
#[derive(Debug, Deserialize)]
pub struct CatchInput {
    pub id: Option<Uuid>,
    pub species_id: Option<Uuid>,
    pub length_inches: Option<f64>,
    pub weight_lbs: Option<f64>,
    pub quantity: Option<i32>,
    pub notes: Option<String>,
}

/// Most catch rows one entry may carry. Not a storage concern — a row is a
/// few numbers — but past this the form stops being usable, and a log that
/// long is a spreadsheet, not a memory. The web form enforces the same
/// number while composing; this is what stops a direct API call.
pub const MAX_CATCHES_PER_ENTRY: usize = 25;

/// Most photos one entry may carry, gallery and per-catch combined. Photos
/// are compressed on the way in (see [`crate::uploads`]), so this keeps a
/// story page manageable rather than protecting the disk.
pub const MAX_PHOTOS_PER_ENTRY: i64 = 20;

/// A submitted catch after validation — the shape that actually reaches the
/// database.
#[derive(Debug, PartialEq)]
struct CleanCatch {
    id: Option<Uuid>,
    species_id: Option<Uuid>,
    length_inches: Option<f64>,
    weight_lbs: Option<f64>,
    /// `None` when the client didn't send one — which the web form no longer
    /// does. [`write_catches`] then keeps an existing row's stored quantity
    /// and gives a new row the column's 1, so older logs that recorded "3
    /// redfish" aren't silently rewritten to 1 by an unrelated edit.
    quantity: Option<i32>,
    notes: Option<String>,
}

fn clean_catches(catches: &[CatchInput]) -> ApiResult<Vec<CleanCatch>> {
    if catches.len() > MAX_CATCHES_PER_ENTRY {
        return Err(AppError::BadRequest(format!(
            "A story can log up to {MAX_CATCHES_PER_ENTRY} catches."
        )));
    }
    let mut out = Vec::with_capacity(catches.len());
    for c in catches {
        if c.quantity.is_some_and(|q| q < 1) {
            return Err(AppError::BadRequest(
                "A catch needs a quantity of at least 1.".into(),
            ));
        }
        for (label, value) in [("length", c.length_inches), ("weight", c.weight_lbs)] {
            if let Some(v) = value
                && (v < 0.0 || !v.is_finite())
            {
                return Err(AppError::BadRequest(format!(
                    "That {label} doesn't look right."
                )));
            }
        }
        out.push(CleanCatch {
            id: c.id,
            species_id: c.species_id,
            length_inches: c.length_inches,
            weight_lbs: c.weight_lbs,
            quantity: c.quantity,
            notes: c
                .notes
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        });
    }
    Ok(out)
}

/// Which of this entry's existing catch rows a submitted list keeps alive.
///
/// An id is honoured only if it names a row of *this* entry. That is the
/// security half of the rule as much as the correctness half: without it a
/// submitted id could reach into somebody else's catch log and have this
/// entry's save overwrite it. An unrecognised id is not an error — it simply
/// doesn't match anything, so the row it came with is inserted fresh.
fn surviving_catch_ids(submitted: &[CleanCatch], mine: &[Uuid]) -> Vec<Uuid> {
    submitted
        .iter()
        .filter_map(|c| c.id)
        .filter(|id| mine.contains(id))
        .collect()
}

/// Writes an entry's catch rows inside the caller's transaction, keeping the
/// identity of rows that were already there.
///
/// This used to delete every row and reinsert the list, which is the simplest
/// thing that works for a repeatable-row form — until photos started hanging
/// off `journal_catches.id`. Under `ON DELETE CASCADE`, a delete-and-reinsert
/// save would take every attached photo with it on a save that changed
/// nothing. So a row that submits an id it already owns is updated in place,
/// and only rows genuinely dropped from the list are deleted.
///
/// Returns the upload URLs of photos that belonged to the deleted rows. Their
/// database rows cascade away here; the files are the caller's to unlink once
/// the transaction has actually committed.
async fn write_catches(
    tx: &mut Transaction<'_, Postgres>,
    entry_id: Uuid,
    catches: &[CleanCatch],
) -> Result<Vec<String>, sqlx::Error> {
    let mine: Vec<(Uuid,)> =
        sqlx::query_as("SELECT id FROM journal_catches WHERE journal_entry_id = $1")
            .bind(entry_id)
            .fetch_all(&mut **tx)
            .await?;
    let mine: Vec<Uuid> = mine.into_iter().map(|(id,)| id).collect();
    let keep = surviving_catch_ids(catches, &mine);

    // Collected before the delete: once the rows cascade there is nothing
    // left to say which files were theirs.
    let orphaned: Vec<(String,)> = sqlx::query_as(
        "SELECT p.url FROM journal_photos p
         JOIN journal_catches c ON c.id = p.journal_catch_id
         WHERE c.journal_entry_id = $1 AND NOT (c.id = ANY($2))",
    )
    .bind(entry_id)
    .bind(&keep)
    .fetch_all(&mut **tx)
    .await?;

    sqlx::query("DELETE FROM journal_catches WHERE journal_entry_id = $1 AND NOT (id = ANY($2))")
        .bind(entry_id)
        .bind(&keep)
        .execute(&mut **tx)
        .await?;

    // `sort_order` is the row's position in the submitted list, exactly — the
    // web form relies on this to match each staged catch photo to the row it
    // was attached to (catch `sort_order == i` is submitted row `i`), so this
    // must stay a plain index and never be compacted or reused otherwise.
    for (i, c) in catches.iter().enumerate() {
        let sort_order = i as i32;
        match c.id.filter(|id| keep.contains(id)) {
            Some(id) => {
                sqlx::query(
                    "UPDATE journal_catches
                     SET species_id = $2, length_inches = $3, weight_lbs = $4,
                         quantity = COALESCE($5, quantity), notes = $6, sort_order = $7
                     WHERE id = $1",
                )
                .bind(id)
                .bind(c.species_id)
                .bind(c.length_inches)
                .bind(c.weight_lbs)
                .bind(c.quantity)
                .bind(&c.notes)
                .bind(sort_order)
                .execute(&mut **tx)
                .await?;
            }
            None => {
                sqlx::query(
                    "INSERT INTO journal_catches
                        (journal_entry_id, species_id, length_inches, weight_lbs, quantity, notes, sort_order)
                     VALUES ($1, $2, $3, $4, COALESCE($5, 1), $6, $7)",
                )
                .bind(entry_id)
                .bind(c.species_id)
                .bind(c.length_inches)
                .bind(c.weight_lbs)
                .bind(c.quantity)
                .bind(&c.notes)
                .bind(sort_order)
                .execute(&mut **tx)
                .await?;
            }
        }
    }
    Ok(orphaned.into_iter().map(|(url,)| url).collect())
}

#[derive(Debug, Deserialize)]
pub struct CreateEntry {
    pub booking_id: Uuid,
    pub title: String,
    pub body: String,
    /// Omitted means family — the private default, matching the column's.
    pub visibility: Option<String>,
    #[serde(default)]
    pub catches: Vec<CatchInput>,
}

fn require_title_and_body<'a>(title: &'a str, body: &'a str) -> ApiResult<(&'a str, &'a str)> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("Title is required.".into()));
    }
    let body = body.trim();
    if body.is_empty() {
        return Err(AppError::BadRequest("Story can't be empty.".into()));
    }
    Ok((title, body))
}

/// `POST /api/journal` — writes the entry and publishes it in one step.
///
/// There is no review: the entry is readable by whoever its `visibility`
/// admits the moment this returns. Eligibility still checks the stay has
/// actually *started* (`check_in <= today`).
pub async fn create(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateEntry>,
) -> ApiResult<Json<FullEntry>> {
    let (title, story) = require_title_and_body(&body.title, &body.body)?;
    let visibility = body.visibility.as_deref().unwrap_or(VISIBILITY_FAMILY);
    validate_visibility(visibility)?;
    let catches = clean_catches(&body.catches)?;

    let booking: Option<(String, NaiveDate, NaiveDate)> = sqlx::query_as(
        "SELECT status, check_in, check_out FROM bookings WHERE id = $1 AND user_id = $2",
    )
    .bind(body.booking_id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?;
    let Some((status, check_in, check_out)) = booking else {
        return Err(AppError::NotFound("Booking not found.".into()));
    };
    require_approved(&status)?;
    require_stay_started(check_in, Utc::now().date_naive())?;

    let (already,): (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM journal_entries WHERE booking_id = $1)")
            .bind(body.booking_id)
            .fetch_one(&state.db)
            .await?;
    require_no_existing_entry(already)?;

    let mut tx = state.db.begin().await?;
    let entry = sqlx::query_as::<_, JournalEntry>(&format!(
        "INSERT INTO journal_entries (user_id, booking_id, title, body, visibility)
         VALUES ($1, $2, $3, $4, $5) RETURNING {ENTRY_COLUMNS}"
    ))
    .bind(user.id)
    .bind(body.booking_id)
    .bind(title)
    .bind(story)
    .bind(visibility)
    .fetch_one(&mut *tx)
    .await?;
    // A brand-new entry has no earlier catch rows, so nothing can be orphaned.
    write_catches(&mut tx, entry.id, &catches).await?;
    tx.commit().await?;

    // Not a review request any more — the story is already live. It is a
    // heads-up, which is what after-the-fact moderation actually needs.
    let guest = user.display_name();
    email::spawn_opt(
        state.clone(),
        state.cfg.admin_email.clone(),
        email_templates::journal_posted_to_admin(
            &guest,
            check_in,
            check_out,
            title,
            visibility,
            app_url(&state),
        ),
    );
    if let Some(admin) = users::first_admin(&state.db).await? {
        notifications::system_message(
            &state.db,
            admin.id,
            "New journal entry",
            &format!("{guest} posted a journal entry: \"{title}\"."),
            Some(body.booking_id),
        )
        .await?;
    }

    Ok(Json(load_full(&state.db, entry.id).await?))
}

#[derive(Debug, Deserialize)]
pub struct UpdateEntry {
    pub title: String,
    pub body: String,
    pub visibility: Option<String>,
    #[serde(default)]
    pub catches: Vec<CatchInput>,
}

/// `PUT /api/journal/{id}` — the author editing their own entry, or a
/// moderator editing anyone's.
///
/// One endpoint rather than a separate admin twin: the edit is the same
/// write either way, and the only thing that differs is who is allowed
/// through the door. Entries are no longer locked after submission — with
/// no review to be "past", there is nothing for a lock to protect.
pub async fn update(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateEntry>,
) -> ApiResult<Json<FullEntry>> {
    let (title, story) = require_title_and_body(&body.title, &body.body)?;
    let catches = clean_catches(&body.catches)?;

    let existing: Option<(Uuid, String)> =
        sqlx::query_as("SELECT user_id, visibility FROM journal_entries WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let Some((owner_id, current_visibility)) = existing else {
        return Err(AppError::NotFound("Journal entry not found.".into()));
    };
    if owner_id != user.id && !moderates(&user) {
        return Err(AppError::Forbidden("That isn't your journal entry.".into()));
    }
    let visibility = body.visibility.as_deref().unwrap_or(&current_visibility);
    validate_visibility(visibility)?;

    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE journal_entries SET title = $2, body = $3, visibility = $4, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(title)
    .bind(story)
    .bind(visibility)
    .execute(&mut *tx)
    .await?;
    let orphaned = write_catches(&mut tx, id, &catches).await?;
    tx.commit().await?;

    // Only once the delete is actually committed: unlinking a file for a
    // transaction that then rolled back would lose a photo that still exists.
    for url in &orphaned {
        delete_upload_file(&state, url).await;
    }

    Ok(Json(load_full(&state.db, id).await?))
}

/// `DELETE /api/journal/{id}` — the entry's author or a moderator.
///
/// Irreversible, and takes the whole memory with it: catches and photo rows
/// cascade, and every photo file is removed from `UPLOAD_DIR` so nothing is
/// orphaned on disk.
pub async fn remove(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM journal_entries WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let Some((owner_id,)) = existing else {
        return Err(AppError::NotFound("Journal entry not found.".into()));
    };
    if owner_id != user.id && !moderates(&user) {
        return Err(AppError::Forbidden("That isn't your journal entry.".into()));
    }

    // Collected before the delete: once the rows cascade away there is
    // nothing left to say which files belonged to this entry.
    let urls: Vec<(String,)> =
        sqlx::query_as("SELECT url FROM journal_photos WHERE journal_entry_id = $1")
            .bind(id)
            .fetch_all(&state.db)
            .await?;

    sqlx::query("DELETE FROM journal_entries WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    for (url,) in &urls {
        delete_upload_file(&state, url).await;
    }

    Ok(Json(
        json!({ "deleted": true, "photos_removed": urls.len() }),
    ))
}

// ─────────────────────────── photos ───────────────────────────

/// Loads an entry's author, refusing anyone who may not write to it.
async fn require_can_edit(state: &Shared, user: &User, entry_id: Uuid) -> ApiResult<()> {
    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM journal_entries WHERE id = $1")
            .bind(entry_id)
            .fetch_optional(&state.db)
            .await?;
    let Some((owner_id,)) = existing else {
        return Err(AppError::NotFound("Journal entry not found.".into()));
    };
    if owner_id != user.id && !moderates(user) {
        return Err(AppError::Forbidden("That isn't your journal entry.".into()));
    }
    Ok(())
}

/// `POST /api/journal/{id}/photos` — multipart, field name `file`, optional
/// `caption`, optional `journal_catch_id`. The entry's author or a moderator.
/// Same upload plumbing as the site gallery (see [`crate::uploads`]), so the
/// type and size rules are identical by construction.
///
/// With `journal_catch_id` the photo belongs to that one catch and shows with
/// it; without, it is a general entry photo for the gallery, exactly as
/// before. A catch photo is *not* also a gallery photo — the two sets are
/// disjoint, so nothing is ever shown twice (see [`hydrate`]).
///
/// Refused once the entry holds [`MAX_PHOTOS_PER_ENTRY`] photos, both kinds
/// counted together — checked before the file is read, so a photo over the
/// cap is never decoded or written at all.
pub async fn upload_photo(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> ApiResult<Json<PhotoRow>> {
    require_can_edit(&state, &user, id).await?;

    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM journal_photos WHERE journal_entry_id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
    require_photo_room(count)?;

    let mut url = None;
    let mut caption = None;
    let mut catch_id: Option<Uuid> = None;
    while let Some(field) = multipart.next_field().await.map_err(bad_multipart)? {
        match field.name() {
            Some("file") => url = Some(save_upload(&state, field).await?),
            Some("caption") => {
                let text = field.text().await.map_err(bad_multipart)?;
                caption = Some(text).filter(|s: &String| !s.trim().is_empty());
            }
            Some("journal_catch_id") => {
                let text = field.text().await.map_err(bad_multipart)?;
                let text = text.trim();
                if !text.is_empty() {
                    catch_id = Some(Uuid::parse_str(text).map_err(|_| {
                        AppError::BadRequest("That isn't a valid catch id.".into())
                    })?);
                }
            }
            _ => {}
        }
    }
    let url = url.ok_or_else(|| AppError::BadRequest("No file provided.".into()))?;

    // The catch must belong to the entry named in the path, or the photo
    // would be reachable through an entry its author cannot edit.
    if let Some(catch_id) = catch_id {
        let (belongs,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM journal_catches WHERE id = $1 AND journal_entry_id = $2)",
        )
        .bind(catch_id)
        .bind(id)
        .fetch_one(&state.db)
        .await?;
        if !belongs {
            // The file is already on disk at this point; drop it rather than
            // leaving an upload nothing will ever reference.
            delete_upload_file(&state, &url).await;
            return Err(AppError::NotFound(
                "That catch isn't part of this entry.".into(),
            ));
        }
    }

    let row = sqlx::query_as::<_, PhotoRow>(
        "INSERT INTO journal_photos (journal_entry_id, journal_catch_id, url, caption, sort_order)
         VALUES ($1, $2, $3, $4, COALESCE(
             (SELECT max(sort_order) + 1 FROM journal_photos WHERE journal_entry_id = $1), 0))
         RETURNING id, url, caption, sort_order, created_at, journal_catch_id",
    )
    .bind(id)
    .bind(catch_id)
    .bind(&url)
    .bind(caption)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(row))
}

/// A soft cap: two uploads racing past it at once can both land, leaving an
/// entry one over. That is harmless for a limit about page length, and not
/// worth a lock.
fn require_photo_room(existing: i64) -> ApiResult<()> {
    if existing >= MAX_PHOTOS_PER_ENTRY {
        return Err(AppError::BadRequest(format!(
            "A story can have up to {MAX_PHOTOS_PER_ENTRY} photos."
        )));
    }
    Ok(())
}

/// `DELETE /api/journal/photos/{photo_id}` — removes the row and the file.
pub async fn delete_photo(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(photo_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let existing: Option<(Uuid, String)> =
        sqlx::query_as("SELECT journal_entry_id, url FROM journal_photos WHERE id = $1")
            .bind(photo_id)
            .fetch_optional(&state.db)
            .await?;
    let Some((entry_id, url)) = existing else {
        return Err(AppError::NotFound("Photo not found.".into()));
    };
    require_can_edit(&state, &user, entry_id).await?;

    sqlx::query("DELETE FROM journal_photos WHERE id = $1")
        .bind(photo_id)
        .execute(&state.db)
        .await?;
    delete_upload_file(&state, &url).await;

    Ok(Json(json!({ "deleted": true })))
}

// ─────────────────────────── moderation ───────────────────────────

#[derive(Debug, Serialize, FromRow)]
pub struct AdminEntryRow {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub visibility: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub guest_name: Option<String>,
    pub guest_email: String,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
}

#[derive(Debug, Serialize)]
pub struct AdminEntry {
    #[serde(flatten)]
    pub row: AdminEntryRow,
    pub catches: Vec<CatchRow>,
    pub photos: Vec<PhotoRow>,
}

impl Hydratable for AdminEntry {
    fn set_catches(&mut self, catches: Vec<CatchRow>) {
        self.catches = catches;
    }
    fn set_photos(&mut self, photos: Vec<PhotoRow>) {
        self.photos = photos;
    }
}

/// `GET /api/journal/admin` — every entry, newest first, with the guest's
/// identity attached. Admin or owner.
///
/// Distinct from the feed, which is a reading experience and deliberately
/// shows only a first name; this is the moderation table.
pub async fn admin_list(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Vec<AdminEntry>>> {
    require_moderator(&user)?;

    let rows = sqlx::query_as::<_, AdminEntryRow>(
        "SELECT je.id, je.title, je.body, je.visibility, je.created_at, je.updated_at,
                je.archived_at, u.full_name AS guest_name, u.email AS guest_email,
                b.check_in, b.check_out
         FROM journal_entries je
         JOIN users u ON u.id = je.user_id
         JOIN bookings b ON b.id = je.booking_id
         ORDER BY je.created_at DESC",
    )
    .fetch_all(&state.db)
    .await?;

    let mut entries: Vec<AdminEntry> = rows
        .into_iter()
        .map(|row| AdminEntry {
            row,
            catches: vec![],
            photos: vec![],
        })
        .collect();
    hydrate(&state.db, &mut entries, |e| e.row.id).await?;
    Ok(Json(entries))
}

/// `PUT /api/journal/{id}/archive` — admin or owner. Quiet housekeeping:
/// hides an entry from everyone but its author and the moderators, without
/// editing it or emailing anyone.
///
/// No longer gated on status (there isn't one) — any entry can be hidden,
/// which is the point of having a lever short of editing or deleting.
pub async fn archive(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<JournalEntry>> {
    require_moderator(&user)?;
    set_archived(&state, id, true).await
}

/// `PUT /api/journal/{id}/unarchive` — the reverse of [`archive`], also
/// silent.
pub async fn unarchive(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<JournalEntry>> {
    require_moderator(&user)?;
    set_archived(&state, id, false).await
}

async fn set_archived(state: &Shared, id: Uuid, archived: bool) -> ApiResult<Json<JournalEntry>> {
    let entry = sqlx::query_as::<_, JournalEntry>(&format!(
        "UPDATE journal_entries
         SET archived_at = CASE WHEN $2 THEN now() ELSE NULL END, updated_at = now()
         WHERE id = $1 RETURNING {ENTRY_COLUMNS}"
    ))
    .bind(id)
    .bind(archived)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Journal entry not found.".into()))?;
    Ok(Json(entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    // ─────────────── eligibility (unchanged by the rework) ───────────────

    #[test]
    fn a_stay_that_has_started_is_eligible_even_without_a_completed_checkout() {
        // The whole point of the rule change: checkout is irrelevant now.
        let today = date(2026, 9, 6);
        let started_five_days_ago = date(2026, 9, 1);
        assert!(is_journal_eligible(
            "approved",
            started_five_days_ago,
            today,
            false
        ));
    }

    #[test]
    fn check_in_day_itself_counts_as_started() {
        let today = date(2026, 9, 6);
        assert!(is_journal_eligible("approved", today, today, false));
    }

    #[test]
    fn a_stay_that_has_not_started_yet_is_never_eligible() {
        let today = date(2026, 9, 6);
        let starts_next_week = date(2026, 9, 13);
        assert!(!is_journal_eligible(
            "approved",
            starts_next_week,
            today,
            false
        ));
        // Not eligible regardless of whether an entry already exists either.
        assert!(!is_journal_eligible(
            "approved",
            starts_next_week,
            today,
            true
        ));
    }

    #[test]
    fn only_an_approved_booking_is_eligible() {
        let today = date(2026, 9, 6);
        for status in ["pending", "denied", "cancelled"] {
            assert!(
                !is_journal_eligible(status, today, today, false),
                "{status} should not be eligible"
            );
        }
    }

    #[test]
    fn journal_eligibility_excludes_a_stay_that_already_has_an_entry() {
        let today = date(2026, 9, 6);
        assert!(!is_journal_eligible("approved", today, today, true));
    }

    #[test]
    fn require_stay_started_blocks_journaling_before_arrival() {
        let today = date(2026, 9, 6);
        assert!(require_stay_started(today, today).is_ok());
        assert!(require_stay_started(date(2026, 9, 1), today).is_ok());
        assert!(matches!(
            require_stay_started(date(2026, 9, 7), today),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn require_approved_rejects_anything_else() {
        assert!(require_approved("approved").is_ok());
        for status in ["pending", "denied", "cancelled"] {
            assert!(matches!(
                require_approved(status),
                Err(AppError::BadRequest(_))
            ));
        }
    }

    #[test]
    fn duplicate_journal_submission_is_blocked() {
        assert!(require_no_existing_entry(false).is_ok());
        assert!(matches!(
            require_no_existing_entry(true),
            Err(AppError::Conflict(_))
        ));
    }

    // ─────────────── visibility ───────────────

    fn viewer(is_author: bool, moderates: bool, is_family: bool) -> ViewerContext {
        ViewerContext {
            is_author,
            moderates,
            is_family,
        }
    }

    const GUEST: ViewerContext = ViewerContext {
        is_author: false,
        moderates: false,
        is_family: false,
    };
    const FAMILY: ViewerContext = ViewerContext {
        is_author: false,
        moderates: false,
        is_family: true,
    };
    const MODERATOR: ViewerContext = ViewerContext {
        is_author: false,
        moderates: true,
        is_family: false,
    };

    #[test]
    fn a_guest_sees_only_public_entries() {
        assert!(may_view(GUEST, VISIBILITY_PUBLIC, false));
        assert!(!may_view(GUEST, VISIBILITY_FAMILY, false));
    }

    #[test]
    fn a_family_user_sees_public_and_family_entries() {
        assert!(may_view(FAMILY, VISIBILITY_PUBLIC, false));
        assert!(may_view(FAMILY, VISIBILITY_FAMILY, false));
    }

    #[test]
    fn a_moderator_sees_everything_including_archived() {
        for visibility in VISIBILITIES {
            assert!(may_view(MODERATOR, visibility, false));
            assert!(may_view(MODERATOR, visibility, true));
        }
    }

    /// The author is the one viewer no state hides an entry from — otherwise
    /// archiving would read to them as their memories having been deleted.
    #[test]
    fn an_author_always_sees_their_own_entry() {
        let author = viewer(true, false, false);
        for visibility in VISIBILITIES {
            assert!(may_view(author, visibility, false));
            assert!(may_view(author, visibility, true));
        }
    }

    #[test]
    fn archiving_hides_an_entry_from_everyone_but_its_author_and_moderators() {
        assert!(!may_view(GUEST, VISIBILITY_PUBLIC, true));
        assert!(!may_view(FAMILY, VISIBILITY_PUBLIC, true));
        assert!(!may_view(FAMILY, VISIBILITY_FAMILY, true));
        assert!(may_view(MODERATOR, VISIBILITY_FAMILY, true));
        assert!(may_view(
            viewer(true, false, false),
            VISIBILITY_FAMILY,
            true
        ));
    }

    /// An unrecognised visibility fails closed. The CHECK constraint makes
    /// this unreachable through the database, which is exactly why the code
    /// should not assume it.
    #[test]
    fn an_unknown_visibility_is_visible_to_nobody_but_author_and_moderators() {
        assert!(!may_view(GUEST, "everyone", false));
        assert!(!may_view(FAMILY, "everyone", false));
        assert!(may_view(MODERATOR, "everyone", false));
    }

    #[test]
    fn only_the_two_documented_visibilities_validate() {
        assert!(validate_visibility(VISIBILITY_PUBLIC).is_ok());
        assert!(validate_visibility(VISIBILITY_FAMILY).is_ok());
        for bad in ["", "everyone", "private", "approved"] {
            assert!(
                matches!(validate_visibility(bad), Err(AppError::BadRequest(_))),
                "{bad} should not validate"
            );
        }
    }

    /// The SQL in [`feed_where_clause`] is the twin of [`may_view`]; if one
    /// grows a branch the other doesn't, this is the reminder.
    #[test]
    fn the_feed_sql_mentions_every_term_the_rule_turns_on() {
        let sql = feed_where_clause();
        assert!(sql.contains("je.user_id = $1"), "author bypass missing");
        assert!(sql.contains("$2"), "moderator bypass missing");
        assert!(
            sql.contains("archived_at IS NULL"),
            "archive filter missing"
        );
        assert!(sql.contains("visibility = 'public'"), "public tier missing");
        assert!(
            sql.contains("$3 AND je.visibility = 'family'"),
            "family tier missing"
        );
    }

    // ─────────────── catch input ───────────────

    fn bare_catch() -> CatchInput {
        CatchInput {
            id: None,
            species_id: None,
            length_inches: None,
            weight_lbs: None,
            quantity: None,
            notes: None,
        }
    }

    /// The form no longer sends a quantity. Leaving it unset (rather than
    /// defaulting to 1 here) is what lets an update keep a stored "3".
    #[test]
    fn an_omitted_quantity_stays_unset_for_the_database_to_resolve() {
        let cleaned = clean_catches(&[bare_catch()]).unwrap();
        assert_eq!(cleaned[0].quantity, None);
    }

    #[test]
    fn a_log_may_hold_up_to_the_catch_cap_and_no_more() {
        let at_cap: Vec<CatchInput> = (0..MAX_CATCHES_PER_ENTRY).map(|_| bare_catch()).collect();
        assert_eq!(clean_catches(&at_cap).unwrap().len(), MAX_CATCHES_PER_ENTRY);

        let over: Vec<CatchInput> = (0..=MAX_CATCHES_PER_ENTRY).map(|_| bare_catch()).collect();
        assert!(matches!(clean_catches(&over), Err(AppError::BadRequest(_))));
    }

    #[test]
    fn an_entry_takes_photos_until_the_cap_and_then_refuses() {
        assert!(require_photo_room(0).is_ok());
        assert!(require_photo_room(MAX_PHOTOS_PER_ENTRY - 1).is_ok());
        assert!(matches!(
            require_photo_room(MAX_PHOTOS_PER_ENTRY),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn a_catch_quantity_below_one_is_rejected() {
        for quantity in [0, -3] {
            assert!(matches!(
                clean_catches(&[CatchInput {
                    id: None,
                    species_id: None,
                    length_inches: None,
                    weight_lbs: None,
                    quantity: Some(quantity),
                    notes: None,
                }]),
                Err(AppError::BadRequest(_))
            ));
        }
    }

    #[test]
    fn negative_and_non_finite_measurements_are_rejected() {
        for bad in [-1.0, f64::NAN, f64::INFINITY] {
            assert!(
                matches!(
                    clean_catches(&[CatchInput {
                        id: None,
                        species_id: None,
                        length_inches: Some(bad),
                        weight_lbs: None,
                        quantity: None,
                        notes: None,
                    }]),
                    Err(AppError::BadRequest(_))
                ),
                "length {bad} should be rejected"
            );
            assert!(
                matches!(
                    clean_catches(&[CatchInput {
                        id: None,
                        species_id: None,
                        length_inches: None,
                        weight_lbs: Some(bad),
                        quantity: None,
                        notes: None,
                    }]),
                    Err(AppError::BadRequest(_))
                ),
                "weight {bad} should be rejected"
            );
        }
    }

    #[test]
    fn blank_catch_notes_are_stored_as_nothing_at_all() {
        let cleaned = clean_catches(&[CatchInput {
            id: None,
            species_id: None,
            length_inches: None,
            weight_lbs: None,
            quantity: Some(2),
            notes: Some("   ".into()),
        }])
        .unwrap();
        assert_eq!(cleaned[0].notes, None);
    }

    // ─────────────── catch identity across a save ───────────────

    fn catch_with_id(id: Option<Uuid>) -> CleanCatch {
        CleanCatch {
            id,
            species_id: None,
            length_inches: None,
            weight_lbs: None,
            quantity: None,
            notes: None,
        }
    }

    /// The reason this rule exists: a row that keeps its id keeps its photos,
    /// because `journal_photos.journal_catch_id` cascades on delete.
    #[test]
    fn a_resubmitted_row_of_this_entry_survives_the_save() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let submitted = [catch_with_id(Some(a)), catch_with_id(Some(b))];
        assert_eq!(surviving_catch_ids(&submitted, &[a, b]), vec![a, b]);
    }

    #[test]
    fn a_row_dropped_from_the_list_does_not_survive() {
        let kept = Uuid::new_v4();
        let dropped = Uuid::new_v4();
        let submitted = [catch_with_id(Some(kept))];
        assert_eq!(
            surviving_catch_ids(&submitted, &[kept, dropped]),
            vec![kept]
        );
    }

    /// A submitted id belonging to a different entry must not be adopted —
    /// otherwise this entry's save would overwrite someone else's catch.
    #[test]
    fn an_id_from_another_entry_is_ignored_rather_than_adopted() {
        let mine = Uuid::new_v4();
        let someone_elses = Uuid::new_v4();
        let submitted = [catch_with_id(Some(someone_elses))];
        assert!(surviving_catch_ids(&submitted, &[mine]).is_empty());
    }

    #[test]
    fn a_brand_new_row_carries_no_id_and_keeps_nothing_alive() {
        let mine = Uuid::new_v4();
        assert!(surviving_catch_ids(&[catch_with_id(None)], &[mine]).is_empty());
        // ...and on a brand-new entry there is nothing to keep either way.
        assert!(surviving_catch_ids(&[catch_with_id(None)], &[]).is_empty());
    }

    /// An empty submitted list clears the log — every existing row is dropped,
    /// which is what takes their photos with them.
    #[test]
    fn submitting_no_catches_keeps_nothing() {
        let mine = Uuid::new_v4();
        assert!(surviving_catch_ids(&[], &[mine]).is_empty());
    }

    #[test]
    fn a_submitted_id_survives_validation_into_the_clean_shape() {
        let id = Uuid::new_v4();
        let cleaned = clean_catches(&[CatchInput {
            id: Some(id),
            species_id: None,
            length_inches: None,
            weight_lbs: None,
            quantity: Some(3),
            notes: None,
        }])
        .unwrap();
        assert_eq!(cleaned[0].id, Some(id));
    }

    #[test]
    fn a_title_or_story_of_only_whitespace_is_refused() {
        assert!(matches!(
            require_title_and_body("   ", "a story"),
            Err(AppError::BadRequest(_))
        ));
        assert!(matches!(
            require_title_and_body("a title", "\n\t "),
            Err(AppError::BadRequest(_))
        ));
        assert_eq!(
            require_title_and_body("  a title  ", "  a story  ").unwrap(),
            ("a title", "a story")
        );
    }
}
