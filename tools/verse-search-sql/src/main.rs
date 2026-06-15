use anyhow::Context;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPoolOptions, PgRow};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

// ---------------- CLI ----------------

#[derive(Parser, Debug)]
#[command(
    name = "verse-search-sql",
    about = "REST API over the CCV PostgreSQL database."
)]
struct Cli {
    /// PostgreSQL connection URL. If absent, built from PGHOST/PGUSER/...
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,
    /// Bind address
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    /// Bind port
    #[arg(long, default_value_t = 3000)]
    port: u16,
    /// Connection pool max size
    #[arg(long, default_value_t = 8)]
    pool_size: u32,
}

fn resolve_database_url(cli: &Cli) -> String {
    if let Some(url) = &cli.database_url {
        return url.clone();
    }
    let host = std::env::var("PGHOST").unwrap_or_else(|_| "localhost".into());
    let port = std::env::var("PGPORT").unwrap_or_else(|_| "5432".into());
    let user = std::env::var("PGUSER")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "postgres".into());
    let password = std::env::var("PGPASSWORD").unwrap_or_default();
    let dbname = std::env::var("PGDATABASE").unwrap_or_else(|_| "ccv".into());
    if password.is_empty() {
        format!("postgres://{user}@{host}:{port}/{dbname}")
    } else {
        format!("postgres://{user}:{password}@{host}:{port}/{dbname}")
    }
}

// ---------------- AppState + bootstrap ----------------

#[derive(Clone)]
struct AppState {
    pool: PgPool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "verse_search_sql=info,tower_http=info".into());
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let cli = Cli::parse();
    let url = resolve_database_url(&cli);
    tracing::info!("connecting to PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(cli.pool_size)
        .connect(&url)
        .await
        .context("connecting to PostgreSQL")?;

    let state = AppState { pool };
    let app = Router::new()
        .route("/health", get(health))
        .route("/search", get(search))
        .route("/rhymes", get(rhymes))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cli.host, cli.port).parse()?;
    tracing::info!("listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;
    Ok(())
}

// ---------------- error wrapper ----------------

struct ApiError(anyhow::Error);
impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        ApiError(e.into())
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("API error: {:#}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": self.0.to_string()})),
        )
            .into_response()
    }
}

// ---------------- /health ----------------

async fn health(State(s): State<AppState>) -> impl IntoResponse {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&s.pool)
        .await
    {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"status": "ok"}))),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"status": "down", "error": e.to_string()})),
        ),
    }
}

// ---------------- /search ----------------

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    #[default]
    Text,
    Regex,
    Lemma,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum CtxMode {
    #[default]
    None,
    Stanza,
    Poem,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    pattern: String,
    #[serde(default)]
    mode: Mode,
    author: Option<String>,
    year_from: Option<i32>,
    year_to: Option<i32>,
    metre: Option<String>,
    book_id: Option<String>,
    #[serde(default = "default_search_limit")]
    limit: i64,
    #[serde(default)]
    context: CtxMode,
    #[serde(default)]
    case_sensitive: bool,
}
fn default_search_limit() -> i64 {
    50
}

#[derive(Debug, Serialize)]
struct Match {
    book_id: String,
    poem_id: String,    // corpus poem_id (string from JSON)
    poem_db_id: i64,    // internal BIGINT id
    author: Option<String>,
    year: Option<i32>,
    book_title: Option<String>,
    poem_title: Option<String>,
    stanza_idx: i32,
    line_idx: i32,
    text: String,
    matched_lemma: Option<String>,
    metres: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
}

#[derive(Debug, Serialize)]
struct SearchResponse {
    count: usize,
    matches: Vec<Match>,
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

async fn search(
    State(s): State<AppState>,
    Query(p): Query<SearchParams>,
) -> Result<Json<SearchResponse>, ApiError> {
    if p.pattern.is_empty() {
        return Err(anyhow::anyhow!("pattern is required").into());
    }
    if p.limit <= 0 || p.limit > 1000 {
        return Err(anyhow::anyhow!("limit must be in 1..=1000").into());
    }

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT
            v.id                                   AS verse_id,
            v.stanza_id                            AS stanza_id,
            s.poem_id                              AS poem_db_id,
            p.book_id                              AS book_id,
            p.corpus_poem_id                       AS poem_id,
            s.stanza_index                         AS stanza_idx,
            v.verse_index                          AS line_idx,
            v.text                                 AS text,
            b.b_title                              AS book_title,
            p.p_title                              AS poem_title,
            COALESCE(pa.name, ba.name)             AS author,
            (substring(b.year FROM '\\d{4}'))::int AS year_int
         FROM verses v
         JOIN stanzas s ON s.id = v.stanza_id
         JOIN poems   p ON p.id = s.poem_id
         JOIN books   b ON b.id = p.book_id
         LEFT JOIN authors pa ON pa.id = p.author_id
         LEFT JOIN authors ba ON ba.id = b.author_id
         WHERE 1=1",
    );

    let mut want_lemma_backfill = false;

    match p.mode {
        Mode::Text => {
            let pat = format!("%{}%", escape_like(&p.pattern));
            if p.case_sensitive {
                qb.push(" AND v.text LIKE ").push_bind(pat);
            } else {
                qb.push(" AND v.text ILIKE ").push_bind(pat);
            }
        }
        Mode::Regex => {
            if p.case_sensitive {
                qb.push(" AND v.text ~ ").push_bind(p.pattern.clone());
            } else {
                qb.push(" AND v.text ~* ").push_bind(p.pattern.clone());
            }
        }
        Mode::Lemma => {
            want_lemma_backfill = true;
            let needle = if p.case_sensitive {
                p.pattern.clone()
            } else {
                p.pattern.to_lowercase()
            };
            let pat = format!("%{}%", escape_like(&needle));
            if p.case_sensitive {
                qb.push(" AND EXISTS (SELECT 1 FROM words w WHERE w.verse_id = v.id AND w.lemma LIKE ")
                    .push_bind(pat)
                    .push(")");
            } else {
                qb.push(" AND EXISTS (SELECT 1 FROM words w WHERE w.verse_id = v.id AND lower(COALESCE(w.lemma,'')) LIKE ")
                    .push_bind(pat)
                    .push(")");
            }
        }
    }

    if let Some(bid) = &p.book_id {
        qb.push(" AND b.id = ").push_bind(bid.clone());
    }
    if let Some(yf) = p.year_from {
        qb.push(" AND (substring(b.year FROM '\\d{4}'))::int >= ")
            .push_bind(yf);
    }
    if let Some(yt) = p.year_to {
        qb.push(" AND (substring(b.year FROM '\\d{4}'))::int <= ")
            .push_bind(yt);
    }
    if let Some(a) = &p.author {
        let needle = format!("%{}%", a.to_lowercase());
        qb.push(" AND (lower(COALESCE(pa.name,'')) LIKE ")
            .push_bind(needle.clone())
            .push(" OR lower(COALESCE(pa.identity,'')) LIKE ")
            .push_bind(needle.clone())
            .push(" OR lower(COALESCE(ba.name,'')) LIKE ")
            .push_bind(needle.clone())
            .push(" OR lower(COALESCE(ba.identity,'')) LIKE ")
            .push_bind(needle)
            .push(")");
    }
    if let Some(m) = &p.metre {
        qb.push(" AND EXISTS (SELECT 1 FROM metres m WHERE m.verse_id = v.id AND m.type = ")
            .push_bind(m.clone())
            .push(")");
    }

    qb.push(" ORDER BY p.book_id, p.corpus_poem_id, s.stanza_index, v.verse_index LIMIT ")
        .push_bind(p.limit);

    let rows = qb
        .build()
        .fetch_all(&s.pool)
        .await
        .context("search query failed")?;

    let verse_ids: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("verse_id")).collect();
    let stanza_ids: Vec<i64> = rows
        .iter()
        .map(|r| r.get::<i64, _>("stanza_id"))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let poem_ids: Vec<i64> = rows
        .iter()
        .map(|r| r.get::<i64, _>("poem_db_id"))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let metres_by_verse = fetch_metres_for_verses(&s.pool, &verse_ids).await?;
    let lemma_by_verse = if want_lemma_backfill {
        fetch_matched_lemma(&s.pool, &verse_ids, &p).await?
    } else {
        HashMap::new()
    };
    let stanza_ctx = if p.context == CtxMode::Stanza {
        fetch_stanza_context(&s.pool, &stanza_ids).await?
    } else {
        HashMap::new()
    };
    let poem_ctx = if p.context == CtxMode::Poem {
        fetch_poem_context(&s.pool, &poem_ids).await?
    } else {
        HashMap::new()
    };

    let mut matches = Vec::with_capacity(rows.len());
    for row in rows {
        let verse_id: i64 = row.get("verse_id");
        let stanza_id: i64 = row.get("stanza_id");
        let poem_db_id: i64 = row.get("poem_db_id");
        matches.push(Match {
            book_id: row.get("book_id"),
            poem_id: row.get("poem_id"),
            poem_db_id,
            author: row.get("author"),
            year: row.get("year_int"),
            book_title: row.get("book_title"),
            poem_title: row.get("poem_title"),
            stanza_idx: row.get("stanza_idx"),
            line_idx: row.get("line_idx"),
            text: row.get("text"),
            matched_lemma: lemma_by_verse.get(&verse_id).cloned(),
            metres: metres_by_verse.get(&verse_id).cloned().unwrap_or_default(),
            context: match p.context {
                CtxMode::None => None,
                CtxMode::Stanza => stanza_ctx.get(&stanza_id).cloned(),
                CtxMode::Poem => poem_ctx.get(&poem_db_id).cloned(),
            },
        });
    }

    Ok(Json(SearchResponse {
        count: matches.len(),
        matches,
    }))
}

async fn fetch_metres_for_verses(
    pool: &PgPool,
    verse_ids: &[i64],
) -> anyhow::Result<HashMap<i64, Vec<String>>> {
    if verse_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT verse_id, type FROM metres
         WHERE verse_id = ANY($1)
         ORDER BY verse_id, metre_index",
    )
    .bind(verse_ids)
    .fetch_all(pool)
    .await?;
    let mut out: HashMap<i64, Vec<String>> = HashMap::new();
    for row in rows {
        let vid: i64 = row.get("verse_id");
        let t: Option<String> = row.get("type");
        if let Some(t) = t {
            out.entry(vid).or_default().push(t);
        }
    }
    Ok(out)
}

async fn fetch_matched_lemma(
    pool: &PgPool,
    verse_ids: &[i64],
    p: &SearchParams,
) -> anyhow::Result<HashMap<i64, String>> {
    if verse_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let needle = if p.case_sensitive {
        p.pattern.clone()
    } else {
        p.pattern.to_lowercase()
    };
    let pat = format!("%{}%", escape_like(&needle));
    let cmp = if p.case_sensitive {
        "w.lemma LIKE $2"
    } else {
        "lower(COALESCE(w.lemma,'')) LIKE $2"
    };
    let sql = format!(
        "SELECT DISTINCT ON (w.verse_id) w.verse_id, w.lemma
         FROM words w
         WHERE w.verse_id = ANY($1) AND {cmp}
         ORDER BY w.verse_id, w.word_index"
    );
    let rows = sqlx::query(&sql)
        .bind(verse_ids)
        .bind(pat)
        .fetch_all(pool)
        .await?;
    let mut out = HashMap::new();
    for row in rows {
        let vid: i64 = row.get("verse_id");
        let lemma: Option<String> = row.get("lemma");
        if let Some(l) = lemma {
            out.insert(vid, l);
        }
    }
    Ok(out)
}

async fn fetch_stanza_context(
    pool: &PgPool,
    stanza_ids: &[i64],
) -> anyhow::Result<HashMap<i64, String>> {
    if stanza_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT stanza_id, text FROM verses
         WHERE stanza_id = ANY($1)
         ORDER BY stanza_id, verse_index",
    )
    .bind(stanza_ids)
    .fetch_all(pool)
    .await?;
    let mut by: HashMap<i64, Vec<String>> = HashMap::new();
    for row in rows {
        let sid: i64 = row.get("stanza_id");
        let text: String = row.get("text");
        by.entry(sid).or_default().push(text);
    }
    Ok(by.into_iter().map(|(k, v)| (k, v.join("\n"))).collect())
}

async fn fetch_poem_context(
    pool: &PgPool,
    poem_ids: &[i64],
) -> anyhow::Result<HashMap<i64, String>> {
    if poem_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT s.poem_id AS poem_id, s.stanza_index AS stanza_idx, v.text AS text
         FROM stanzas s
         JOIN verses v ON v.stanza_id = s.id
         WHERE s.poem_id = ANY($1)
         ORDER BY s.poem_id, s.stanza_index, v.verse_index",
    )
    .bind(poem_ids)
    .fetch_all(pool)
    .await?;
    let mut by: HashMap<i64, Vec<(i32, String)>> = HashMap::new();
    for row in rows {
        let pid: i64 = row.get("poem_id");
        let sidx: i32 = row.get("stanza_idx");
        let text: String = row.get("text");
        by.entry(pid).or_default().push((sidx, text));
    }
    let mut out = HashMap::new();
    for (pid, lines) in by {
        let mut grouped: Vec<Vec<String>> = Vec::new();
        let mut cur_idx: i32 = i32::MIN;
        for (sidx, text) in lines {
            if sidx != cur_idx {
                grouped.push(Vec::new());
                cur_idx = sidx;
            }
            grouped.last_mut().unwrap().push(text);
        }
        let formatted = grouped
            .into_iter()
            .map(|st| st.join("\n"))
            .collect::<Vec<_>>()
            .join("\n\n");
        out.insert(pid, formatted);
    }
    Ok(out)
}

// ---------------- /rhymes ----------------

#[derive(Debug, Deserialize)]
struct RhymesParams {
    word: String,
    #[serde(default)]
    by_lemma: bool,
    author: Option<String>,
    year_from: Option<i32>,
    year_to: Option<i32>,
    #[serde(default = "default_rhymes_limit")]
    limit: i64,
    #[serde(default = "default_min_count")]
    min_count: i64,
}
fn default_rhymes_limit() -> i64 {
    30
}
fn default_min_count() -> i64 {
    1
}

#[derive(Debug, Serialize)]
struct RhymeEntry {
    word: String,
    count: i64,
    lemmas: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RhymesResponse {
    target: String,
    total_occurrences: i64,
    unique_in_response: usize,
    entries: Vec<RhymeEntry>,
}

async fn rhymes(
    State(s): State<AppState>,
    Query(p): Query<RhymesParams>,
) -> Result<Json<RhymesResponse>, ApiError> {
    if p.word.is_empty() {
        return Err(anyhow::anyhow!("word is required").into());
    }
    if p.limit <= 0 || p.limit > 1000 {
        return Err(anyhow::anyhow!("limit must be in 1..=1000").into());
    }

    let target = p.word.to_lowercase();

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "WITH target_verses AS (
            SELECT v.id AS verse_id, s.poem_id AS poem_id, v.rhyme_group AS rhyme_group
            FROM verse_last_word lw
            JOIN verses v  ON v.id = lw.verse_id
            JOIN stanzas s ON s.id = v.stanza_id
            JOIN poems   pp ON pp.id = s.poem_id
            JOIN books   b  ON b.id = pp.book_id
            LEFT JOIN authors pa ON pa.id = pp.author_id
            LEFT JOIN authors ba ON ba.id = b.author_id
            WHERE v.rhyme_group IS NOT NULL ",
    );
    if p.by_lemma {
        qb.push(" AND lower(COALESCE(lw.lemma,'')) = ")
            .push_bind(target.clone());
    } else {
        qb.push(" AND lw.token_lc = ").push_bind(target.clone());
    }
    if let Some(yf) = p.year_from {
        qb.push(" AND (substring(b.year FROM '\\d{4}'))::int >= ")
            .push_bind(yf);
    }
    if let Some(yt) = p.year_to {
        qb.push(" AND (substring(b.year FROM '\\d{4}'))::int <= ")
            .push_bind(yt);
    }
    if let Some(a) = &p.author {
        let needle = format!("%{}%", a.to_lowercase());
        qb.push(" AND (lower(COALESCE(pa.name,'')) LIKE ")
            .push_bind(needle.clone())
            .push(" OR lower(COALESCE(pa.identity,'')) LIKE ")
            .push_bind(needle.clone())
            .push(" OR lower(COALESCE(ba.name,'')) LIKE ")
            .push_bind(needle.clone())
            .push(" OR lower(COALESCE(ba.identity,'')) LIKE ")
            .push_bind(needle)
            .push(")");
    }
    qb.push(
        " )
         SELECT plw.token_lc                                  AS word,
                count(*)                                      AS cnt,
                array_remove(array_agg(DISTINCT plw.lemma), NULL) AS lemmas,
                sum(count(*)) OVER ()                         AS grand_total
         FROM target_verses t
         JOIN stanzas ps ON ps.poem_id = t.poem_id
         JOIN verses  pv ON pv.stanza_id = ps.id
                        AND pv.rhyme_group = t.rhyme_group
                        AND pv.id <> t.verse_id
         JOIN verse_last_word plw ON plw.verse_id = pv.id
         WHERE plw.token_lc IS NOT NULL AND plw.token_lc <> ''
         GROUP BY plw.token_lc
         HAVING count(*) >= ",
    );
    qb.push_bind(p.min_count);
    qb.push(" ORDER BY count(*) DESC, plw.token_lc ASC LIMIT ");
    qb.push_bind(p.limit);

    let rows: Vec<PgRow> = qb
        .build()
        .fetch_all(&s.pool)
        .await
        .context("rhymes query failed")?;

    let mut entries = Vec::with_capacity(rows.len());
    let mut grand_total: i64 = 0;
    for row in rows {
        let word: String = row.get("word");
        let cnt: i64 = row.get("cnt");
        let lemmas: Vec<String> = row
            .try_get::<Option<Vec<String>>, _>("lemmas")
            .ok()
            .flatten()
            .unwrap_or_default();
        grand_total = row.try_get::<Option<i64>, _>("grand_total").ok().flatten().unwrap_or(grand_total);
        entries.push(RhymeEntry {
            word,
            count: cnt,
            lemmas,
        });
    }

    Ok(Json(RhymesResponse {
        target,
        total_occurrences: grand_total,
        unique_in_response: entries.len(),
        entries,
    }))
}
