-- Secondary indexes. Run after data load for fastest import.

CREATE INDEX idx_books_author       ON books(author_id);
CREATE INDEX idx_poems_book         ON poems(book_id);
CREATE INDEX idx_poems_author       ON poems(author_id);
CREATE INDEX idx_stanzas_poem       ON stanzas(poem_id);
CREATE INDEX idx_verses_stanza      ON verses(stanza_id);
CREATE INDEX idx_verses_rhyme       ON verses(rhyme_group);
CREATE INDEX idx_verses_last_token  ON verses(last_token);
CREATE INDEX idx_verses_last_lemma  ON verses(last_lemma);

CREATE INDEX idx_tokens_lemma       ON tokens(lemma);
CREATE INDEX idx_rhymes_a_score     ON rhymes(token_a_id, score DESC);
CREATE INDEX idx_rhymes_b_score     ON rhymes(token_b_id, score DESC);
