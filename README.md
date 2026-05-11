# rymovadlo

Nástroj pro **přebásnění libovolného českého textu do rýmovaných slok**. Rýmy vybírá z [Korpusu českého verše](http://versologie.cz/v2/web_content/corpus.php?lang=en) (CCV) — z reálných rýmů, které použili čeští básníci 19. a začátku 20. století.

Skládá se ze tří vrstev:

1. **`ccv/`** — korpus 1305 knih české poezie ve formátu JSON (anotované rýmy, metrum, fonetika, lemmatizace).
2. **`verse-search`** — Rust CLI (binárka v rootu) pro rychlé vyhledávání v korpusu. Má dva subkomandy:
   - `search <pattern>` — hledá verše podle textu, regexu nebo lemmatu (s filtry na autora/rok/metrum).
   - `rhymes <slovo>` — vrátí slova, se kterými básníci v korpusu rýmovali zadané slovo, seřazená podle frekvence.
3. **`.claude/skills/prebasni/`** — Claude Code skill `/prebasni`, který vezme nerýmovaný text, rozdělí ho na sloky a verše a přepíše konce veršů tak, aby se rýmovaly. Pro každou rýmovou skupinu si zavolá `verse-search rhymes` a vybere kandidáta z korpusu.

## Instalace

### 1. Požadavky

- macOS / Linux
- [Rust toolchain](https://rustup.rs/) (pro build `verse-search`)
- [Claude Code](https://claude.com/claude-code) (pro použití skillu `/prebasni`)

### 2. Naklonování repozitáře

```bash
git clone <url-tohoto-repa> rymovadlo
cd rymovadlo
```

Korpus `ccv/` (cca 3,8 GB JSON) je součástí repozitáře.

### 3. Build nástroje `verse-search`

```bash
cd tools/verse-search
cargo build --release
cp target/release/verse-search ../../verse-search
cd ../..
```

Binárka `./verse-search` v rootu projektu je to, co volá skill. (V repu už jedna zkompilovaná verze leží — pokud sedí na váš systém, build můžete přeskočit.)

Ověření, že to běží:

```bash
./verse-search rhymes "noc" --limit 5 --format json
```

Mělo by vypsat JSON se slovy, kterými básníci rýmovali „noc".

## Použití skillu `/prebasni` v Claude Code

Skill je registrovaný v `.claude/skills/prebasni/SKILL.md` a Claude Code ho automaticky načte, když otevřete tento projekt jako pracovní adresář.

### Spuštění

V Claude Code napište:

```
/prebasni Ráno bylo mlhavé. Šel jsem podél řeky a viděl, jak se voda valí pomalu. Stromy byly mokré od rosy a ptáci ještě nezpívali.
```

Agent text rozdělí na sloky a verše, pro každou rýmovou skupinu zavolá `./verse-search rhymes` a vybere vhodný rým z korpusu. Výstupem je rýmovaná báseň plus jednořádková poznámka o použitém schématu.

### Argumenty

Před textem můžete předat (v libovolném pořadí):

| Argument | Výchozí | Popis |
|---|---|---|
| `--scheme aabb\|abab\|abba\|aaaa` | `aabb` | Schéma rýmů uvnitř sloky. |
| `--lines-per-stanza N` | `4` | Kolik veršů na sloku. |
| `--keep-stanzas` | `true` | Pokud má vstup prázdné řádky, zachovat počet slok. |
| `--by-lemma` | vypnuto | Při hledání rýmů použít `--by-lemma` (širší množina kandidátů). |

Příklad:

```
/prebasni --scheme abab --lines-per-stanza 4 Šel jsem lesem, padalo listí, vítr foukal, slunce zacházelo za kopcem.
```

### Přímé volání `verse-search`

Nástroj se dá použít i bez agenta:

```bash
# Najít rýmy z celého korpusu
./verse-search rhymes "láska" --limit 25 --format json

# Hledat verše obsahující slovo
./verse-search search "měsíc" --author "Mácha" --format text

# Regex přes všechny verše
./verse-search search "^Když.*sníh$" --mode regex
```

## Struktura projektu

```
rymovadlo/
├── ccv/                          # Korpus českého verše (JSON, 1305 knih)
├── tools/verse-search/           # Zdrojový kód Rust nástroje
├── verse-search                  # Zkompilovaná binárka (volaná skillem)
├── .claude/
│   ├── settings.json             # Povolení pro Claude Code
│   └── skills/prebasni/SKILL.md  # Definice skillu /prebasni
└── README.md
```

## Licence korpusu

Korpus českého verše je sestavován v [Ústavu pro českou literaturu AV ČR](http://ucl.cas.cz) a je distribuován pod licencí [CC-BY-SA](https://creativecommons.org/licenses/by-sa/4.0/). Při použití dat citujte:

- Plecháč, P. & Kolár, R. (2015). *The Corpus of Czech Verse*. Studia Metrica et Poetica 2(1), 107–118. [DOI](https://doi.org/10.12697/smp.2015.2.1.05)
- Plecháč, P. (2016). *Czech Verse Processing System KVĚTA — Phonetic and Metrical Components*. Glottotheory 7(2), 159–174. [DOI](https://doi.org/10.1515/glot-2016-0013)
