-- Performance indexes for verse-search-sql.
-- Run AFTER schema.sql + data + indexes.sql + sequences.sql.
--
-- Requires the pg_trgm extension (ships with PostgreSQL contrib).

CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Substring / regex search on verses.text (used by /search mode=text|regex).
-- Without this, ILIKE '%foo%' on 2.3M rows is a seq scan (~3-8 s per request).
CREATE INDEX IF NOT EXISTS idx_verses_text_trgm
    ON verses USING gin (text gin_trgm_ops);

-- Self-join on (stanza_id, rhyme_group) used by /rhymes.
CREATE INDEX IF NOT EXISTS idx_verses_stanza_rhyme
    ON verses(stanza_id, rhyme_group)
    WHERE rhyme_group IS NOT NULL;

-- books.year is TEXT (sometimes "[1923]", "1923-24"); index extracts a 4-digit
-- year so /search?year_from=...&year_to=... can use it.
CREATE INDEX IF NOT EXISTS idx_books_year_int
    ON books (((substring(year FROM '\d{4}'))::int));

-- Last word of every verse, materialized for /rhymes.
-- ~2.3M rows. Refresh with: REFRESH MATERIALIZED VIEW verse_last_word;
CREATE MATERIALIZED VIEW IF NOT EXISTS verse_last_word AS
SELECT DISTINCT ON (w.verse_id)
    w.verse_id,
    lower(w.token) AS token_lc,
    w.lemma
FROM words w
ORDER BY w.verse_id, w.word_index DESC;

CREATE UNIQUE INDEX IF NOT EXISTS idx_verse_last_word_verse
    ON verse_last_word(verse_id);
CREATE INDEX IF NOT EXISTS idx_verse_last_word_token
    ON verse_last_word(token_lc);
CREATE INDEX IF NOT EXISTS idx_verse_last_word_lemma
    ON verse_last_word(lemma);
