# Pokyny pro AI agenty (Claude Code, Cursor, Codex, …)

Tento projekt přebásňuje český text do rýmovaných slok. Rýmy bere z [Korpusu českého verše](http://versologie.cz/v2/web_content/corpus.php) (CCV) přes lokální Rust CLI `verse-search`.

Detailní popis projektu, instalace a uživatelské použití viz [README.md](./README.md).

## Architektura — tři vrstvy

1. **`ccv/`** — korpus 1305 knih (~3,8 GB JSON). Neměnit, je to vstupní data.
2. **`tools/verse-search/`** — Rust zdrojový kód CLI. Build do `tools/verse-search/target/release/verse-search` se ručně kopíruje do rootu jako `./verse-search` (binárka v rootu je to, co se volá z agenta).
3. **Skill `/prebasni`** — definice je v `.claude/skills/prebasni/SKILL.md` (zdroj pravdy). Skill volá `./verse-search rhymes <slovo>` a přepisuje verše. Pro ostatní agenty jsou jen tenké wrappery, které ukazují zpět na tento soubor:
   - **Claude Code** — `.claude/skills/prebasni/SKILL.md` (přímo).
   - **OpenAI Codex CLI** — `.agents/skills/prebasni` (symlink na složku v `.claude/skills/`, stejný formát SKILL.md).
   - **Gemini CLI** — `.gemini/commands/prebasni.toml` (TOML wrapper, embeduje obsah SKILL.md přes `@{...}` syntax).

### Alternativní SQL backend (`tools/verse-search-sql/`)

Rust REST API server nad PostgreSQL dumpem korpusu (`tools/ccv-to-sql/` generuje schema + per-table data SQL). Endpointy `/health`, `/search`, `/rhymes` (parametry zhruba sedí s CLI `verse-search`). DB připojení přes `--database-url` flag nebo `PGHOST/PGUSER/PGPASSWORD/PGDATABASE/PGPORT` env vars. Před prvním spuštěním nahrát `tools/verse-search-sql/perf_indexes.sql` (pg_trgm GIN nad `verses.text`, materializovaný `verse_last_word` pro rhymes). Build: `cd tools/verse-search-sql && cargo run --release -- --database-url postgres://…`. Skill `/prebasni` zatím **nepřepojen** — pořád volá `./verse-search` (JSON scan).

## Klíčové příkazy

```bash
# Rebuild CLI po změně v tools/verse-search/src/
cd tools/verse-search && cargo build --release && cp target/release/verse-search ../../verse-search

# Otestovat rhymes lookup
./verse-search rhymes "noc" --limit 10 --format json

# Plnotextové hledání ve verších
./verse-search search "měsíc" --author "Mácha" --format text
```

`verse-search` má dva subkomandy: `search` (text/regex/lemma + filtry autor/rok/metrum) a `rhymes` (rýmoví partneři pro slovo).

## Pravidla

- **Nikdy necommituj `tools/verse-search/target/`** — je v `.gitignore`, je to build artifact (stovky MB).
- **Binárka `./verse-search` v rootu JE commitnutá** — je to runtime závislost skillu. Po změně Rust kódu ji nezapomeň zkopírovat z `target/release/` a commitnout.
- **Korpus `ccv/` neupravuj** — je to read-only vstupní dataset pod CC-BY-SA licencí (UČL AV ČR).
- Skill `/prebasni` parsuje JSON výstup `verse-search rhymes` — pokud měníš formát výstupu CLI, aktualizuj i `.claude/skills/prebasni/SKILL.md` (wrappery pro Codex a Gemini se aktualizují automaticky, protože ukazují na tento soubor).
- Defaultní `--corpus` cesta v CLI je relativní (`ccv`), takže CLI musí běžet z rootu projektu, jinak se předává `--corpus <abs-cesta>`.

## Když přidáváš nový subkomand do `verse-search`

1. Edit `tools/verse-search/src/main.rs`.
2. `cargo build --release` v `tools/verse-search/`.
3. Zkopíruj binárku do rootu (`cp target/release/verse-search ../../verse-search`).
4. Otestuj přímým voláním z rootu.
5. Pokud má subkomand sloužit i skillu `/prebasni`, doplň ho do `.claude/skills/prebasni/SKILL.md`.

## Jazyk

Uživatelská komunikace, skill `prebasni` i README jsou v češtině. Komentáře v Rust kódu a tento soubor smí být v češtině i angličtině — drž se stylu daného souboru.
