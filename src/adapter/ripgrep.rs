//! The floor: text search over the source tree.
//!
//! # Why this adapter is the one that matters most
//!
//! Every code-intelligence tool is sold against an agent that has `Read` and
//! `Grep` and nothing else. That agent is not a strawman — it is what most
//! coding agents actually do today, it needs no index, no server and no
//! warm-up, and on a small package it is *fast*. If a resolved-symbol index
//! cannot beat it, the index is not paying for itself.
//!
//! Two properties of this adapter keep that comparison honest:
//!
//! * It is given the same source tree the index was built from, so it is not
//!   handicapped by missing data.
//! * Its output is capped ([`MAX_OUTPUT_BYTES`]) and the cap is *reported*.
//!   An uncapped grep can score perfect recall on any scenario by printing
//!   the package, which would be a measurement artefact rather than a
//!   result. The cap is what makes "found it" mean "found it in a budget a
//!   real agent would spend", and the `truth.none_of` distractor sets are
//!   what stop a wide net from scoring as a clean answer.

use std::path::Path;
use std::process::Stdio;
use std::time::Instant;

use super::{Adapter, Attempt, AttemptBuilder};
use crate::scenario::{Scenario, TaskKind};

/// How much grep output a run is willing to put in an agent's context.
/// Roughly 25k tokens of source — already far past what a careful agent
/// would paste, and deliberately generous so the cap cannot be blamed for a
/// poor score.
pub const MAX_OUTPUT_BYTES: usize = 100_000;

/// Paths excluded from every search.
///
/// # Why this exists, and why it makes the floor *stronger*
///
/// Every scenario here asks about a library's own surface or its internal
/// call graph, and every ground truth in `scenarios/` was derived by reading
/// library source. A package's tests can therefore contribute false
/// positives but never an expected fact.
///
/// That is not a hypothesis. Auditing the stored payloads of
/// `run-2026-08-30-165221` (see `results/2026-08-30/payload-audit.md`) found
/// that on `go/uuid/callers-parse` **82% of the returned lines came from
/// `_test.go` files**, and that the only distractor recorded anywhere in that
/// run — `ParseBytes` — was dragged in by a `-B 12` window opened around a
/// call site inside `uuid_test.go`. The expected callers (`MustParse` in
/// `uuid.go`, `Scan` in `sql.go`) were both in library source and survive
/// this exclusion.
///
/// So the effect of this list is fewer tokens and fewer false positives, both
/// in ripgrep's favour. A developer asking "who calls this" scopes the search
/// the same way; leaving tests in was measuring the harness's invocation
/// rather than the tool.
const EXCLUDE: &[&str] = &[
    "!**/*_test.go",
    "!**/*_test.rs",
    "!**/test_*.py",
    "!**/*_test.py",
    "!**/*.test.ts",
    "!**/*.test.js",
    "!**/*.spec.ts",
    "!**/*.spec.js",
    "!**/tests/**",
    "!**/test/**",
    "!**/testdata/**",
    "!**/__tests__/**",
    "!**/node_modules/**",
    "!**/vendor/**",
    "!**/target/**",
];

/// `--glob` flags for [`EXCLUDE`], ready to splice in front of a pattern's
/// own arguments.
///
/// # Why the root has to be consulted
///
/// rg matches a glob against the path it prints, which includes the search
/// root. So `!**/node_modules/**` does not mean "skip nested dependencies"
/// when the root *is* `…/node_modules/commander` — it means "skip
/// everything", and the adapter returns nothing at all.
///
/// That is measured, not imagined: the first run with these exclusions turned
/// `ts/commander/public-surface` from a pass into 0% recall on 149 tokens,
/// because every file in the package sat under an excluded segment. A
/// directory exclusion is therefore dropped whenever the root is already
/// inside a directory of that name.
fn exclude_args(root: &Path) -> Vec<String> {
    let root = root.to_string_lossy().into_owned();
    EXCLUDE
        .iter()
        .filter(|glob| match dir_segment(glob) {
            Some(segment) => !root.split('/').any(|component| component == segment),
            None => true,
        })
        .flat_map(|glob| ["--glob".to_owned(), (*glob).to_owned()])
        .collect()
}

/// The directory name a `!**/<name>/**` exclusion is about, or `None` for a
/// file-name pattern like `!**/*_test.go`, which can never swallow a root.
fn dir_segment(glob: &str) -> Option<&str> {
    glob.strip_prefix("!**/")?.strip_suffix("/**")
}

/// Text search over a package checkout.
pub struct RipgrepAdapter {
    keep_responses: bool,
}

impl RipgrepAdapter {
    pub fn new(keep_responses: bool) -> Self {
        Self { keep_responses }
    }

    /// The patterns this task would send a developer to type, in order.
    ///
    /// Multiple patterns are allowed because that is what a developer does:
    /// the first grep for a name is rarely the last. Each one is a separate
    /// step and is charged for separately.
    fn patterns(scenario: &Scenario) -> Vec<(String, Vec<String>)> {
        let Some(literal) = scenario.probe.literal() else {
            return Vec::new();
        };
        let word = regex_escape(literal);
        match scenario.task {
            TaskKind::Definition | TaskKind::Signature => vec![(
                format!("declaration of {literal}"),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    // A signature spans lines: `pub fn memchr3(` puts every
                    // parameter and the return type on the lines that follow.
                    // Grepping the one matching line would score this adapter
                    // on a formatting convention rather than on what it found.
                    "-A".into(),
                    "8".into(),
                    "-e".into(),
                    format!(
                        r"(fn|struct|trait|enum|type|const|static|class|def|func|interface)\s+{word}\b"
                    ),
                ],
            )],
            TaskKind::Callers => vec![(
                format!("call sites of {literal}"),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    // The line holding a call names the callee, never the
                    // caller. Answering "who calls this" from grep output
                    // means reading back up to the enclosing declaration, so
                    // the leading context is part of the honest baseline —
                    // and the tokens it costs are part of the honest price.
                    "-B".into(),
                    "12".into(),
                    "-e".into(),
                    format!(r"\b{word}\s*\("),
                ],
            )],
            TaskKind::Outbound => vec![(
                format!("body of {literal}"),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    "-A".into(),
                    "40".into(),
                    "-e".into(),
                    format!(r"(fn|func|def)\s+{word}\b"),
                ],
            )],
            TaskKind::Implementors => vec![(
                format!("impls of {literal}"),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    "-e".into(),
                    format!(r"impl(<[^>]*>)?\s+.*\b{word}\b.*\s+for\s+"),
                ],
            )],
            TaskKind::Members => vec![(
                format!("body of {literal}"),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    "-A".into(),
                    "60".into(),
                    "-e".into(),
                    format!(r"(struct|enum|trait|class|interface)\s+{word}\b"),
                ],
            )],
            TaskKind::Overview => vec![(
                "exported surface".into(),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    "-e".into(),
                    r"^\s*(pub |export |public )".into(),
                ],
            )],
            TaskKind::Search | TaskKind::Implement => vec![(
                format!("mentions of {literal}"),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    "-e".into(),
                    word.clone(),
                ],
            )],
            // Comparing two versions needs two checkouts; a scenario carries
            // one root, so this is declined rather than faked.
            TaskKind::Diff => Vec::new(),
        }
    }
}

#[async_trait::async_trait]
impl Adapter for RipgrepAdapter {
    fn name(&self) -> &str {
        "ripgrep"
    }

    fn description(&self) -> String {
        format!(
            "ripgrep over the package checkout, output capped at {} bytes",
            MAX_OUTPUT_BYTES
        )
    }

    async fn attempt(&mut self, scenario: &Scenario) -> Attempt {
        let Some(root) = scenario.target.root.as_ref() else {
            return Attempt::unsupported(
                self.name(),
                scenario,
                "scenario has no target.root; a text search needs the source on disk",
            );
        };
        if !root.exists() {
            return Attempt::unsupported(
                self.name(),
                scenario,
                format!("target.root does not exist: {}", root.display()),
            );
        }
        if scenario.task == TaskKind::Diff {
            return Attempt::unsupported(
                self.name(),
                scenario,
                "comparing two versions needs two checkouts; the scenario carries one",
            );
        }

        let patterns = Self::patterns(scenario);
        if patterns.is_empty() {
            return Attempt::unsupported(self.name(), scenario, "no probe literal to search for");
        }

        let mut builder =
            AttemptBuilder::new(self.name(), scenario, Default::default(), self.keep_responses);
        let mut error = None;

        for (label, args) in patterns {
            let started = Instant::now();
            // The exclusions are part of the invocation, so they are rendered
            // into the recorded command and charged for like any other flag.
            let mut args = args;
            let mut full = exclude_args(root);
            full.append(&mut args);
            let args = full;
            let rendered = format!("rg {} {}", args.join(" "), root.display());
            let output = tokio::process::Command::new("rg")
                .args(&args)
                .arg(root)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await;

            let (text, is_error) = match output {
                Ok(out) => {
                    let mut stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                    let truncated = stdout.len() > MAX_OUTPUT_BYTES;
                    if truncated {
                        // Cut on a line boundary so the transcript stays
                        // readable, and on a character boundary so cutting is
                        // possible at all: source text is full of multi-byte
                        // characters and both the slice and the truncate
                        // below panic in the middle of one.
                        let limit = crate::text::floor_char_boundary(&stdout, MAX_OUTPUT_BYTES);
                        let cut = stdout[..limit].rfind('\n').unwrap_or(limit);
                        stdout.truncate(cut);
                        stdout.push_str(&format!(
                            "\n… output truncated at {MAX_OUTPUT_BYTES} bytes; \
                             the remaining matches were not read\n"
                        ));
                    }
                    // rg exits 1 for "no matches", which is an answer, not a failure.
                    let failed = !out.status.success() && out.status.code() != Some(1);
                    if failed {
                        let err = String::from_utf8_lossy(&out.stderr).into_owned();
                        (format!("ripgrep failed: {err}"), true)
                    } else if stdout.trim().is_empty() {
                        ("(no matches)".to_owned(), false)
                    } else {
                        (stdout, false)
                    }
                }
                Err(e) => (format!("could not run ripgrep: {e}"), true),
            };

            if is_error {
                error.get_or_insert_with(|| text.clone());
            }
            let matched = matched_lines(&text);
            builder.step(label, rendered, &text, started.elapsed(), is_error);
            builder.primary(&matched);
        }

        builder.finish(error)
    }
}

/// Keep only the lines ripgrep *matched*, discarding the `-A`/`-B` window.
///
/// With `-n --no-heading`, rg writes a matched line as `path:line:text` and a
/// context line as `path-line-text`. That single character is the whole
/// signal, and it is the only place in this harness where "the tool found
/// this" can be told apart from "the tool printed this nearby".
///
/// The separator is located after a path ending in a file extension, because a
/// path can itself contain both `:` and `-` (`uuid@v1.6.0/`), so scanning for
/// the first `[:-]` would misread most Go module paths.
fn matched_lines(text: &str) -> String {
    text.lines()
        .filter(|line| is_match_line(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether one rendered rg line is a match rather than context.
fn is_match_line(line: &str) -> bool {
    // Find `<path><sep><digits><sep>` where <path> ends at a file extension.
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b':' || c == b'-' {
            // Digits must follow, then the same class of separator again.
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && j < bytes.len() && (bytes[j] == b':' || bytes[j] == b'-') {
                // Only accept it if what precedes looks like a file, so that
                // `uuid@v1.6.0` and similar are not mistaken for the anchor.
                let head = &line[..i];
                if head.rsplit('/').next().is_some_and(|f| f.contains('.')) {
                    return c == b':';
                }
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    false
}

/// Escape a literal for use inside a regex.
fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regex_metacharacters_are_escaped() {
        assert_eq!(regex_escape("Vec<T>"), "Vec<T>");
        assert_eq!(regex_escape("a.b"), r"a\.b");
        assert_eq!(regex_escape("f(x)"), r"f\(x\)");
    }

    #[test]
    fn diff_is_declined_rather_than_faked() {
        let scenario = crate::testing::diff_scenario();
        assert!(RipgrepAdapter::patterns(&scenario).is_empty());
    }

    /// The whole matched-versus-context distinction rests on one character,
    /// and the paths it has to survive are hostile: a Go module path carries
    /// both `@` and dots, and a cargo registry path carries dashes and dots.
    #[test]
    fn match_lines_are_told_apart_from_context_lines() {
        // The real shapes, taken from run-2026-08-30-165221.
        let m = "/home/leaf/go/pkg/mod/github.com/google/uuid@v1.6.0/uuid.go:245:\tif err := uuid.Parse(s); err != nil {";
        let c = "/home/leaf/go/pkg/mod/github.com/google/uuid@v1.6.0/uuid.go-164-// MustParse is like Parse but panics";
        assert!(is_match_line(m), "matched line must be recognised");
        assert!(!is_match_line(c), "context line must not count as a match");

        let rs = "/home/leaf/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/memchr-2.8.3/src/memchr.rs:31:    search_slice_with_raw(";
        assert!(is_match_line(rs), "a registry path full of dashes is still a match");

        // A separator inside the version must not be mistaken for the anchor.
        let ctx_rs = "/home/leaf/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/memchr-2.8.3/src/memchr.rs-29-pub fn memchr(";
        assert!(!is_match_line(ctx_rs));

        assert!(!is_match_line("(no matches)"));
        assert!(!is_match_line(""));
    }

    #[test]
    fn only_matched_lines_survive_the_filter() {
        let payload = "a/b.go:10:hit\na/b.go-9-near\na/b.go:12:hit2\n";
        assert_eq!(matched_lines(payload), "a/b.go:10:hit\na/b.go:12:hit2");
    }

    /// Tests are excluded from every search: they can contribute false
    /// positives but never an expected fact, which the payload audit of
    /// run-2026-08-30-165221 demonstrated rather than assumed.
    #[test]
    fn test_files_are_excluded_from_every_search() {
        let args = exclude_args(Path::new("/pkgs/memchr-2.8.3"));
        assert!(args.iter().any(|a| a == "!**/*_test.go"));
        assert!(args.iter().any(|a| a == "!**/tests/**"));
        assert_eq!(args.iter().filter(|a| *a == "--glob").count(), EXCLUDE.len());
    }

    /// A directory exclusion must never swallow the search root. This is the
    /// `ts/commander/public-surface` regression, pinned: the package lives
    /// *inside* `node_modules`, so excluding that directory excluded the
    /// entire package and scored 0% on 149 tokens.
    #[test]
    fn an_exclusion_never_swallows_the_root() {
        let args = exclude_args(Path::new("/home/x/context-bench/node_modules/commander"));
        assert!(
            !args.iter().any(|a| a == "!**/node_modules/**"),
            "the root is inside node_modules, so that exclusion must be dropped"
        );
        // File-level exclusions are unaffected: they cannot match a directory.
        assert!(args.iter().any(|a| a == "!**/*.test.js"));
        // And an unrelated directory exclusion still applies.
        assert!(args.iter().any(|a| a == "!**/vendor/**"));
    }

    #[test]
    fn directory_segments_are_read_off_the_glob() {
        assert_eq!(dir_segment("!**/node_modules/**"), Some("node_modules"));
        assert_eq!(dir_segment("!**/*_test.go"), None);
    }
}
