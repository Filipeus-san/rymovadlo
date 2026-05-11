use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use rayon::prelude::*;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering as CmpOrdering;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Debug, Clone, ValueEnum)]
enum Mode {
    Text,
    Regex,
    Lemma,
}

#[derive(Debug, Clone, ValueEnum)]
enum Format {
    Text,
    Json,
}

#[derive(Debug, Clone, ValueEnum)]
enum CtxMode {
    None,
    Stanza,
    Poem,
}

#[derive(Parser, Debug)]
#[command(
    name = "verse-search",
    about = "Search the Corpus of Czech Verse (1305 books, ~3.8 GB JSON).",
    long_about = "Subcommands:\n\
                  \n\
                  \tsearch  — search verses by text, regex, or lemma; filter by author/year/metre.\n\
                  \trhymes  — find rhyme partners for a word using corpus rhyme annotations."
)]
struct Cli {
    /// Path to corpus directory (containing *.json files)
    #[arg(long, default_value = "ccv", global = true)]
    corpus: PathBuf,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Search verses across the corpus
    Search(SearchArgs),
    /// Find rhyme partners for a target word using corpus rhyme annotations
    Rhymes(RhymesArgs),
}

#[derive(Args, Debug)]
struct SearchArgs {
    /// Pattern to search for (literal substring, regex, or lemma form)
    pattern: String,

    /// Search mode
    #[arg(long, value_enum, default_value_t = Mode::Text)]
    mode: Mode,

    /// Filter by author name (substring on p_author/b_author identity or name)
    #[arg(long)]
    author: Option<String>,

    /// Lower bound on publication year (inclusive)
    #[arg(long)]
    year_from: Option<i32>,

    /// Upper bound on publication year (inclusive)
    #[arg(long)]
    year_to: Option<i32>,

    /// Filter by metre type (J=iamb, T=trochee, D=dactyl, A=amphibrach, X, Y, N, hexameter, pentameter)
    #[arg(long)]
    metre: Option<String>,

    /// Restrict search to a single book id (file stem, e.g. 0001)
    #[arg(long)]
    book_id: Option<String>,

    /// Maximum number of matches to return
    #[arg(long, default_value_t = 50)]
    limit: usize,

    /// Output format
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,

    /// Context to include around each match
    #[arg(long, value_enum, default_value_t = CtxMode::None)]
    context: CtxMode,

    /// Case-sensitive matching (default: case-insensitive for text/lemma; regex respects flags)
    #[arg(long)]
    case_sensitive: bool,
}

#[derive(Args, Debug)]
struct RhymesArgs {
    /// Target word: find words the corpus rhymes with this one
    word: String,

    /// Match the target by lemma instead of token (expands to all inflected forms)
    #[arg(long)]
    by_lemma: bool,

    /// Filter by author name (substring on p_author/b_author identity or name)
    #[arg(long)]
    author: Option<String>,

    /// Lower bound on publication year (inclusive)
    #[arg(long)]
    year_from: Option<i32>,

    /// Upper bound on publication year (inclusive)
    #[arg(long)]
    year_to: Option<i32>,

    /// Maximum number of distinct rhyme words to return
    #[arg(long, default_value_t = 30)]
    limit: usize,

    /// Minimum number of corpus occurrences for a rhyme to be reported
    #[arg(long, default_value_t = 1)]
    min_count: usize,

    /// Output format
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
}

#[derive(Debug, Deserialize)]
struct Author {
    #[serde(default)]
    identity: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Biblio {
    #[serde(default)]
    p_title: Option<String>,
    #[serde(default)]
    b_title: Option<String>,
    #[serde(default)]
    year: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct Word {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    lemma: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Metre {
    #[serde(default, rename = "type")]
    type_: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Line {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    words: Vec<Word>,
    #[serde(default, alias = "meter")]
    metre: Vec<Metre>,
    #[serde(default)]
    rhyme: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct Poem {
    #[serde(default)]
    book_id: Option<Value>,
    #[serde(default)]
    poem_id: Option<Value>,
    #[serde(default)]
    p_author: Option<Author>,
    #[serde(default)]
    b_author: Option<Author>,
    #[serde(default)]
    biblio: Option<Biblio>,
    #[serde(default)]
    body: Vec<Vec<Line>>,
}

#[derive(Debug, Serialize)]
struct Match {
    book_file: String,
    book_id: Option<String>,
    poem_id: Option<String>,
    author: Option<String>,
    year: Option<i32>,
    book_title: Option<String>,
    poem_title: Option<String>,
    stanza_idx: usize,
    line_idx: usize,
    text: String,
    matched_lemma: Option<String>,
    metres: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
}

#[derive(Debug, Serialize)]
struct RhymeEntry {
    word: String,
    count: usize,
    lemmas: Vec<String>,
}

fn value_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => None,
        _ => Some(v.to_string()),
    }
}

fn rhyme_index(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn parse_year(v: Option<&Value>) -> Option<i32> {
    let v = v?;
    match v {
        Value::Number(n) => n.as_i64().map(|x| x as i32),
        Value::String(s) => {
            let digits: String = s.chars().filter(|c| c.is_ascii_digit()).take(4).collect();
            if digits.len() == 4 {
                digits.parse().ok()
            } else {
                None
            }
        }
        _ => None,
    }
}

fn author_display(p: &Option<Author>, b: &Option<Author>) -> Option<String> {
    let pick = |a: &Author| {
        a.name
            .clone()
            .or_else(|| a.identity.clone())
            .filter(|s| !s.is_empty())
    };
    p.as_ref().and_then(pick).or_else(|| b.as_ref().and_then(pick))
}

fn author_matches(filter: &str, p: &Option<Author>, b: &Option<Author>) -> bool {
    let needle = filter.to_lowercase();
    let check = |a: &Author| {
        let n = a.name.as_deref().unwrap_or("").to_lowercase();
        let i = a.identity.as_deref().unwrap_or("").to_lowercase();
        n.contains(&needle) || i.contains(&needle)
    };
    p.as_ref().map_or(false, check) || b.as_ref().map_or(false, check)
}

fn poem_in_year_range(poem: &Poem, from: Option<i32>, to: Option<i32>) -> bool {
    if from.is_none() && to.is_none() {
        return true;
    }
    let year = parse_year(poem.biblio.as_ref().and_then(|b| b.year.as_ref()));
    if let Some(yf) = from {
        if year.map_or(true, |y| y < yf) {
            return false;
        }
    }
    if let Some(yt) = to {
        if year.map_or(true, |y| y > yt) {
            return false;
        }
    }
    true
}

enum Matcher {
    LiteralCi(String),
    LiteralCs(String),
    Regex(Regex),
}

impl Matcher {
    fn new(pattern: &str, mode: &Mode, case_sensitive: bool) -> Result<Self> {
        match mode {
            Mode::Regex => {
                let re = RegexBuilder::new(pattern)
                    .case_insensitive(!case_sensitive)
                    .build()
                    .with_context(|| format!("invalid regex: {pattern}"))?;
                Ok(Matcher::Regex(re))
            }
            Mode::Text | Mode::Lemma => {
                if case_sensitive {
                    Ok(Matcher::LiteralCs(pattern.to_string()))
                } else {
                    Ok(Matcher::LiteralCi(pattern.to_lowercase()))
                }
            }
        }
    }

    fn matches(&self, hay: &str) -> bool {
        match self {
            Matcher::LiteralCi(needle) => hay.to_lowercase().contains(needle),
            Matcher::LiteralCs(needle) => hay.contains(needle),
            Matcher::Regex(re) => re.is_match(hay),
        }
    }
}

fn metre_strings(line: &Line) -> Vec<String> {
    line.metre
        .iter()
        .filter_map(|m| m.type_.clone())
        .collect()
}

fn metre_matches(filter: &str, line: &Line) -> bool {
    line.metre
        .iter()
        .any(|m| m.type_.as_deref() == Some(filter))
}

fn build_context(poem: &Poem, stanza_idx: usize, ctx: &CtxMode) -> Option<String> {
    match ctx {
        CtxMode::None => None,
        CtxMode::Stanza => poem.body.get(stanza_idx).map(|stanza| {
            stanza
                .iter()
                .map(|l| l.text.as_deref().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n")
        }),
        CtxMode::Poem => Some(
            poem.body
                .iter()
                .map(|stanza| {
                    stanza
                        .iter()
                        .map(|l| l.text.as_deref().unwrap_or(""))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
        ),
    }
}

fn collect_files(corpus: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(corpus)
        .with_context(|| format!("reading corpus dir {}", corpus.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn load_poems(path: &Path) -> Result<Vec<Poem>> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let poems: Vec<Poem> = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing JSON in {}", path.display()))?;
    Ok(poems)
}

// ---------- search ----------

fn process_file_search(
    path: &Path,
    args: &SearchArgs,
    matcher: &Matcher,
    remaining: usize,
) -> Result<Vec<Match>> {
    if remaining == 0 {
        return Ok(Vec::new());
    }
    let book_file = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();

    if let Some(ref want) = args.book_id {
        if &book_file != want {
            return Ok(Vec::new());
        }
    }

    let poems = load_poems(path)?;

    let mut out = Vec::new();
    for poem in &poems {
        if out.len() >= remaining {
            break;
        }

        if let Some(ref a) = args.author {
            if !author_matches(a, &poem.p_author, &poem.b_author) {
                continue;
            }
        }
        if !poem_in_year_range(poem, args.year_from, args.year_to) {
            continue;
        }

        let year = parse_year(poem.biblio.as_ref().and_then(|b| b.year.as_ref()));
        let author = author_display(&poem.p_author, &poem.b_author);
        let book_title = poem.biblio.as_ref().and_then(|b| b.b_title.clone());
        let poem_title = poem.biblio.as_ref().and_then(|b| b.p_title.clone());
        let book_id = poem.book_id.as_ref().and_then(value_to_string);
        let poem_id = poem.poem_id.as_ref().and_then(value_to_string);

        for (si, stanza) in poem.body.iter().enumerate() {
            for (li, line) in stanza.iter().enumerate() {
                if out.len() >= remaining {
                    break;
                }
                if let Some(ref m) = args.metre {
                    if !metre_matches(m, line) {
                        continue;
                    }
                }
                let (hit, matched_lemma) = match args.mode {
                    Mode::Lemma => {
                        let mut found: Option<String> = None;
                        for w in &line.words {
                            if let Some(ref lemma) = w.lemma {
                                if matcher.matches(lemma) {
                                    found = Some(lemma.clone());
                                    break;
                                }
                            }
                        }
                        (found.is_some(), found)
                    }
                    Mode::Text | Mode::Regex => {
                        let text = line.text.as_deref().unwrap_or("");
                        (matcher.matches(text), None)
                    }
                };
                if !hit {
                    continue;
                }
                out.push(Match {
                    book_file: book_file.clone(),
                    book_id: book_id.clone(),
                    poem_id: poem_id.clone(),
                    author: author.clone(),
                    year,
                    book_title: book_title.clone(),
                    poem_title: poem_title.clone(),
                    stanza_idx: si,
                    line_idx: li,
                    text: line.text.clone().unwrap_or_default(),
                    matched_lemma,
                    metres: metre_strings(line),
                    context: build_context(poem, si, &args.context),
                });
            }
        }
    }
    Ok(out)
}

fn print_search_text(matches: &[Match]) {
    for m in matches {
        let author = m.author.as_deref().unwrap_or("?");
        let year = m
            .year
            .map(|y| y.to_string())
            .unwrap_or_else(|| "?".into());
        let b_title = m.book_title.as_deref().unwrap_or("");
        let p_title = m.poem_title.as_deref().unwrap_or("");
        let metre = if m.metres.is_empty() {
            String::new()
        } else {
            format!(" [{}]", m.metres.join(","))
        };
        let lemma = m
            .matched_lemma
            .as_ref()
            .map(|l| format!(" lemma={l}"))
            .unwrap_or_default();
        println!(
            "[{book}] {author} ({year}) — {b}{sep}{p}",
            book = m.book_file,
            author = author,
            year = year,
            b = b_title,
            sep = if !p_title.is_empty() { " / " } else { "" },
            p = p_title,
        );
        println!(
            "  s{si}:l{li}{metre}{lemma}  {text}",
            si = m.stanza_idx,
            li = m.line_idx,
            metre = metre,
            lemma = lemma,
            text = m.text,
        );
        if let Some(ref ctx) = m.context {
            for line in ctx.lines() {
                println!("    | {line}");
            }
        }
    }
    println!("\n{} match(es).", matches.len());
}

fn run_search(corpus: &Path, args: &SearchArgs) -> Result<()> {
    let matcher = Matcher::new(&args.pattern, &args.mode, args.case_sensitive)?;
    let files = collect_files(corpus)?;
    if files.is_empty() {
        anyhow::bail!("no .json files found in {}", corpus.display());
    }

    let limit = args.limit.max(1);
    let counter = AtomicUsize::new(0);
    let results: Mutex<Vec<Match>> = Mutex::new(Vec::with_capacity(limit));

    files.par_iter().for_each(|path| {
        let already = counter.load(Ordering::Relaxed);
        if already >= limit {
            return;
        }
        let remaining = limit - already;
        let local = match process_file_search(path, args, &matcher, remaining) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("warning: {}: {e:#}", path.display());
                return;
            }
        };
        if local.is_empty() {
            return;
        }
        let mut guard = results.lock().unwrap();
        for m in local {
            if guard.len() >= limit {
                break;
            }
            guard.push(m);
        }
        counter.store(guard.len(), Ordering::Relaxed);
    });

    let mut results = results.into_inner().unwrap();
    results.sort_by(|a, b| match a.book_file.cmp(&b.book_file) {
        CmpOrdering::Equal => match a.poem_id.cmp(&b.poem_id) {
            CmpOrdering::Equal => match a.stanza_idx.cmp(&b.stanza_idx) {
                CmpOrdering::Equal => a.line_idx.cmp(&b.line_idx),
                o => o,
            },
            o => o,
        },
        o => o,
    });

    match args.format {
        Format::Json => {
            let s = serde_json::to_string_pretty(&results)?;
            println!("{s}");
        }
        Format::Text => print_search_text(&results),
    }
    Ok(())
}

// ---------- rhymes ----------

/// Counts of (count, lemmas-set) per rhyme partner word (lowercased token).
type RhymeCounts = HashMap<String, (usize, Vec<String>)>;

fn merge_into(into: &mut RhymeCounts, other: RhymeCounts) {
    for (word, (count, lemmas)) in other {
        let entry = into.entry(word).or_insert_with(|| (0, Vec::new()));
        entry.0 += count;
        for l in lemmas {
            if !entry.1.contains(&l) {
                entry.1.push(l);
            }
        }
    }
}

fn last_word(line: &Line) -> Option<&Word> {
    line.words.last()
}

fn process_file_rhymes(path: &Path, args: &RhymesArgs, target: &str) -> Result<RhymeCounts> {
    let poems = load_poems(path)?;
    let mut counts: RhymeCounts = HashMap::new();

    for poem in &poems {
        if let Some(ref a) = args.author {
            if !author_matches(a, &poem.p_author, &poem.b_author) {
                continue;
            }
        }
        if !poem_in_year_range(poem, args.year_from, args.year_to) {
            continue;
        }

        // Build map: rhyme_index -> Vec<(stanza_idx, line_idx, last_word)>
        let mut buckets: HashMap<i64, Vec<(usize, usize, &Word)>> = HashMap::new();
        for (si, stanza) in poem.body.iter().enumerate() {
            for (li, line) in stanza.iter().enumerate() {
                let r = match rhyme_index(line.rhyme.as_ref()) {
                    Some(r) => r,
                    None => continue,
                };
                if let Some(w) = last_word(line) {
                    buckets.entry(r).or_default().push((si, li, w));
                }
            }
        }

        // For each line whose last word matches the target, look up partners
        // in the same rhyme bucket and increment their counts.
        for (si, stanza) in poem.body.iter().enumerate() {
            for (li, line) in stanza.iter().enumerate() {
                let r = match rhyme_index(line.rhyme.as_ref()) {
                    Some(r) => r,
                    None => continue,
                };
                let w = match last_word(line) {
                    Some(w) => w,
                    None => continue,
                };
                let hit = if args.by_lemma {
                    w.lemma
                        .as_ref()
                        .map(|s| s.to_lowercase())
                        .as_deref()
                        == Some(target)
                } else {
                    w.token
                        .as_ref()
                        .map(|s| s.to_lowercase())
                        .as_deref()
                        == Some(target)
                };
                if !hit {
                    continue;
                }
                if let Some(bucket) = buckets.get(&r) {
                    for (ps, pl, pw) in bucket {
                        if *ps == si && *pl == li {
                            continue;
                        }
                        let token = match &pw.token {
                            Some(t) if !t.is_empty() => t.to_lowercase(),
                            _ => continue,
                        };
                        // Skip self-token (e.g. when partner happens to be same surface form)
                        // — keep it; same word can still be a valid rhyme record.
                        let entry = counts.entry(token).or_insert_with(|| (0, Vec::new()));
                        entry.0 += 1;
                        if let Some(l) = &pw.lemma {
                            if !l.is_empty() && !entry.1.contains(l) {
                                entry.1.push(l.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(counts)
}

fn print_rhymes_text(target: &str, entries: &[RhymeEntry], total: usize) {
    if entries.is_empty() {
        println!("Rýmy pro \"{target}\": žádné nenalezeny.");
        return;
    }
    println!(
        "Rýmy pro \"{target}\" ({} výskytů, {} unikátních):",
        total,
        entries.len()
    );
    for e in entries {
        let lemmas = if e.lemmas.is_empty() {
            String::new()
        } else {
            format!(" (lemma: {})", e.lemmas.join(", "))
        };
        println!("  {:>5}x  {}{}", e.count, e.word, lemmas);
    }
}

fn run_rhymes(corpus: &Path, args: &RhymesArgs) -> Result<()> {
    let target = args.word.to_lowercase();
    let files = collect_files(corpus)?;
    if files.is_empty() {
        anyhow::bail!("no .json files found in {}", corpus.display());
    }

    let counts = files
        .par_iter()
        .fold(RhymeCounts::new, |mut acc, path| {
            match process_file_rhymes(path, args, &target) {
                Ok(c) => merge_into(&mut acc, c),
                Err(e) => eprintln!("warning: {}: {e:#}", path.display()),
            }
            acc
        })
        .reduce(RhymeCounts::new, |mut a, b| {
            merge_into(&mut a, b);
            a
        });

    let mut entries: Vec<RhymeEntry> = counts
        .into_iter()
        .filter(|(_, (c, _))| *c >= args.min_count)
        .map(|(word, (count, lemmas))| RhymeEntry {
            word,
            count,
            lemmas,
        })
        .collect();

    // Sort by count desc, then word asc.
    entries.sort_by(|a, b| match b.count.cmp(&a.count) {
        CmpOrdering::Equal => a.word.cmp(&b.word),
        o => o,
    });
    let total: usize = entries.iter().map(|e| e.count).sum();
    entries.truncate(args.limit.max(1));

    match args.format {
        Format::Json => {
            let s = serde_json::to_string_pretty(&entries)?;
            println!("{s}");
        }
        Format::Text => print_rhymes_text(&target, &entries, total),
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.cmd {
        Cmd::Search(args) => run_search(&cli.corpus, args),
        Cmd::Rhymes(args) => run_rhymes(&cli.corpus, args),
    }
}
