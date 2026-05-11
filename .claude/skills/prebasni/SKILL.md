---
name: prebasni
description: Vezme delší nerýmovaný text uživatele, rozdělí ho na sloky a verše a přepíše konce veršů tak, aby se rýmovaly. Rýmy vybírá z Korpusu českého verše přes nástroj ./verse-search rhymes. Použij když uživatel volá /prebasni <text> nebo žádá z prózy/poznámek/odstavce udělat rýmovanou báseň. Volitelné argumenty (před textem): --scheme aabb|abab|abba|aaaa (default aabb), --lines-per-stanza N (default 4), --keep-stanzas (zachovat počet slok ze vstupu, default true).
---

# Přebásnit text — rozdělit na sloky/verše a zarýmovat

Vstupem je delší nerýmovaný text (próza, odstavce, volný verš). Výstupem je tentýž obsah přepsaný do rýmovaných slok pomocí Korpusu českého verše.

Nástroj `./verse-search` (v rootu projektu) má dva subkomandy:

- `./verse-search search <pattern>` — vyhledávání ve verších
- `./verse-search rhymes <slovo>` — vrací slova, se kterými básníci v korpusu rýmovali `<slovo>`, seřazená podle frekvence

Tato skill používá hlavně `rhymes`.

## Argumenty

V argumentech mohou být (před textem, libovolné pořadí):

- `--scheme aabb|abab|abba|aaaa` — schéma rýmů uvnitř sloky. Default `aabb` (nejjednodušší).
- `--lines-per-stanza N` — kolik veršů na sloku. Default `4` (čtyřverší).
- `--keep-stanzas` — pokud vstup má jasné sloky (oddělené prázdnými řádky), zachovej jejich počet. Default `true`. Pokud vstup je jeden blok, sloky vytvoříš sám.
- `--by-lemma` — při hledání rýmů používej `--by-lemma` (širší množina kandidátů, ale méně přesné). Default vypnuté.

Vše ostatní = text k přebásnění.

Pokud text chybí, zeptej se: „Jaký text mám přebásnit?"

## Postup

### 1. Rozdělit text na sloky

- Pokud `--keep-stanzas` a vstup má prázdné řádky → každý odstavec = jedna sloka.
- Jinak rozděl text na bloky podle smyslu (3–6 vět na sloku) a každý blok = jedna sloka.

### 2. Rozdělit každou sloku na verše

Cíl: `--lines-per-stanza` veršů na sloku (default 4).

- Hledej přirozené hranice: konce vět, středníky, čárky před spojkami (`a`, `ale`, `však`, `kde`, `když`).
- Verš by měl mít zhruba 6–11 slabik (typické v české lyrice). Žádné super-přesné počítání není nutné.
- Pokud má sloka víc obsahu, zhušti ho parafrází; nikdy nepřidávej obsah, který v textu není.

### 3. Určit cílové schéma rýmů

Schéma se aplikuje na pozice veršů uvnitř sloky:

- `aabb` → verš 1 rýmuje s 2, verš 3 rýmuje s 4
- `abab` → verš 1 rýmuje s 3, verš 2 rýmuje s 4
- `abba` → verš 1 rýmuje s 4, verš 2 rýmuje s 3
- `aaaa` → všechny verše ve sloce rýmují

### 4. Pro každou skupinu rýmujících veršů najít rým

Pro každou rýmovou skupinu (např. ve schématu `aabb` ve čtyřverší jsou skupiny `[v1, v2]` a `[v3, v4]`):

a) Vyber jeden verš jako **pivot** (typicky první ve skupině) — toho nech, jak je, a vezmi jeho poslední slovo `W`.

b) Spusť:

```bash
./verse-search rhymes "<W>" --limit 25 --format json
```

(Pokud `--by-lemma` je zapnuté, přidej `--by-lemma`.)

c) Z výsledků vyber kandidátní rýmy. Preferuj:

- Slova s rozumným `count` (≥3 znamená, že to není raritní rým)
- Slova vhodná do kontextu sloky (sémanticky a slovnědruhově)
- Vyhni se identickému slovu (pokud `W="noc"`, `noc` taky rýmuje, ale je to slabý "identický rým")
- Pokud výsledky jsou prázdné, zkus `--by-lemma`. Pokud i tam prázdno, zkus zkrátit `W` (vzít poslední dvě slabiky jako lemma) nebo si vyber jiný pivot ve skupině.

d) **Přepiš ostatní verše ve skupině**, aby končily vybraným rýmem (nebo blízkým z téže rodiny). Smysl původní řádky musí zůstat — můžeš ji parafrázovat, přeházet, mírně rozšířit/zkrátit. Cílem je zachytit obraz a myšlenku, ne původní slovosled.

### 5. Doladit rytmus

Po prvním průchodu:

- Zkontroluj, že verše uvnitř sloky mají podobnou délku (rozdíl ±2 slabiky je OK).
- Pokud je verš drsně dlouhý/krátký, parafrázuj.
- Nezarýmuj násilně — pokud žádný kandidát z korpusu nesedí, je lepší volný verš se stejným počtem slabik než nesmyslný rým.

### 6. Výstup

Vypiš výsledek v tomto formátu:

```
<verš 1>
<verš 2>
<verš 3>
<verš 4>

<verš 5>
<verš 6>
...
```

Pod tím v jednom řádku napiš: použité schéma + poznámku, kde sis musel vzít volnost (např. „abab; ve 3. sloce volnější rým"). Žádné delší metakomentáře.

## Edge cases

- **Jednoslokový vstup s 1–2 větami** → vytvoř právě jednu sloku (čtyřverší), obsah rozviň obrazem.
- **Vstup co se nedá rozumně rozdělit na 4 verše na sloku** → změň `--lines-per-stanza`, oznam to v poznámce.
- **Slovo, které nemá rýmy v korpusu** (`rhymes` vrátí 0) → zkus `--by-lemma`. Pokud nic, použij přirozený asonanční rým ze slovníku (např. „věc" / „zpět") a v poznámce uveď, že rým není z korpusu.
- **Velmi dlouhý vstup (>500 slov)** → zhušti razantně. Drž max 6–8 slok.

## Doporučení

- Korpus je staršího jazyka (převážně 19. a začátek 20. století). Rýmové návrhy obsahují archaismy (např. *zoře*, *cháska*) — můžeš je použít, ale nepřeháněj. Současný čtenář by měl výsledek pochopit.
- `verse-search rhymes` je rychlý (~2 s pro full scan, méně s `--author`). Volej ho klidně 4–8× za báseň, je to v pohodě.
- JSON výstup parsuj (text mód je pro lidi).
- Defaultní cesta nástroje je `./verse-search` z rootu projektu (`/Users/filipeus/Projects/corpusCzechVerse`). Pokud CWD je jinde, použij absolutní cestu.

## Příklad

Vstup uživatele:

```
/prebasni Ráno bylo mlhavé. Šel jsem podél řeky a viděl, jak se voda valí pomalu. Stromy byly mokré od rosy a ptáci ještě nezpívali.
```

Postup:

1. Rozdělit na verše (jedna sloka, 4 verše):
   - V1: Ráno bylo mlhavé
   - V2: Šel jsem podél řeky
   - V3: Voda se valila pomalu
   - V4: Stromy mokré od rosy, ptáci tiší

2. Schéma `aabb` → skupiny `[V1, V2]` a `[V3, V4]`.

3. Rýmy pro „mlhavé" → `./verse-search rhymes "mlhavé" --limit 15 --format json` → např. `tmavé, hravé, plavé, žhavé`.

4. Přepsat V2 tak, aby končil rýmujícím slovem: „Šel jsem dál pěšinou tmavou."

5. Rýmy pro „pomalu" → kandidáty → přepsat V4. Atd.

Výstup:

```
Ráno bylo mlhavé,
šel jsem dál pěšinou tmavou.
Voda valila se pomalu,
stromy mokré stály v málu.

aabb; V4 volnější ("v málu" je archaický tvar).
```
