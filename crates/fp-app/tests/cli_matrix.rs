//! Hostile-input matrix for the command line.
//!
//! Every dispatched command is exercised once with arguments that work and then
//! with inputs that a user, a script or a damaged file can produce: a missing
//! file, an empty one, a constant column, a single row, `NaN`/`inf`, a column
//! that does not exist, an unknown enum value, a non-numeric number, zero and
//! extreme numeric options, and an option left without its value.
//!
//! What is asserted is the contract the executable owes its callers: it exits,
//! it does not panic, it does not hang, and a rejected invocation exits with
//! status 1 and writes a message to stderr. The numeric results themselves are
//! covered by the reference tests; this file is only about robustness.
//!
//! The coverage test reads the dispatcher in `src/cli.rs`, so a new command
//! cannot be added without a line here.

use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// A command that takes longer than this is stuck, not slow.
const TIMEOUT: Duration = Duration::from_secs(30);

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_FreePlots")
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/cli")
}

fn temp_dir(tag: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "fp-cli-matrix-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// One command with arguments that are known to work. `{in}` and `{out}` are
/// resolved against the committed fixtures and a fresh temporary directory.
struct Case {
    command: &'static str,
    argv: &'static [&'static str],
}

const fn case(command: &'static str, argv: &'static [&'static str]) -> Case {
    Case { command, argv }
}

/// Every dispatched command, one line each.
#[rustfmt::skip]
const MATRIX: &[Case] = &[
    case("agreement", &["{in}agreement.csv", "--part", "part", "--appraiser", "appraiser", "--rating", "rating"]),
    case("alt", &["{in}alt.csv", "--time", "time", "--stress", "stress", "--use-stress", "100", "--distribution", "weibull", "--model", "arrhenius"]),
    case("analyze", &["{in}normal.csv", "--column", "data", "--distribution", "weibull"]),
    case("anova", &["{in}groups.csv", "--response", "y", "--factor", "g"]),
    case("anova2", &["{in}twoway.csv", "--response", "y", "--factor-a", "A", "--factor-b", "B"]),
    case("attrchart", &["{in}attr.csv", "--column", "defects", "--chart", "p", "--size", "100"]),
    case("bartlett", &["{in}variance.csv", "--response", "y", "--factor", "g"]),
    case("bbdesign", &["{out}bb.csv", "--factors", "A,B,C", "--center-points", "3"]),
    case("boxcox", &["{in}boxcox.csv", "--column", "x"]),
    case("boxplot", &["{in}groups.csv", "--column", "y", "--by", "g"]),
    case("calc", &["{in}normal.csv", "--name", "twice", "--expression", "data*2", "--output", "{out}calc.csv"]),
    case("capability", &["{in}normal.csv", "--column", "data", "--lsl", "15", "--usl", "25"]),
    case("ccd", &["{out}ccd.csv", "--factors", "A,B", "--center-points", "4"]),
    case("controlchart", &["{in}normal.csv", "--column", "data", "--chart", "imr"]),
    case("correlate", &["{in}matrix.csv", "--columns", "a,b,c"]),
    case("crosstab", &["{in}contingency.csv", "--row", "row", "--column", "col"]),
    case("curvefit", &["{in}curve.csv", "--x", "x", "--y", "y", "--model", "poly2"]),
    case("dateadd", &["{in}dates.csv", "--column", "start", "--days", "10", "--name", "later", "--output", "{out}dates-add.csv"]),
    case("datediff", &["{in}dates.csv", "--first", "start", "--second", "end", "--name", "days", "--output", "{out}dates-out.csv"]),
    case("decompose", &["{in}series.csv", "--column", "value", "--period", "12"]),
    case("describe", &["{in}groups.csv", "--by", "g"]),
    case("desirability", &["{in}rsm.csv", "--factors", "A,B", "--response", "y:maximize:40:60"]),
    case("distcompare", &["{in}normal.csv", "--column", "data"]),
    case("doctor", &[]),
    case("doe", &["{in}doe.csv", "--response", "y", "--factors", "A,B"]),
    case("doealias", &["--base", "A,B,C", "--generators", "D=ABC"]),
    case("doedesign", &["{out}factorial.csv", "--factors", "A,B", "--replicates", "2", "--center-points", "1"]),
    case("dotplot", &["{in}groups.csv", "--column", "y", "--by", "g"]),
    case("ecdf", &["{in}normal.csv", "--column", "data"]),
    case("equiv1", &["{in}normal.csv", "--column", "data", "--margin", "1", "--mean", "20"]),
    case("equiv2", &["{in}two.csv", "--columns", "a,b", "--margin", "1"]),
    case("equivpaired", &["{in}two.csv", "--columns", "a,b", "--margin", "1"]),
    case("exporeliability", &["{in}survival.csv", "--time", "time", "--censor", "censor", "--censor-value", "0"]),
    case("fracdesign", &["{out}frac.csv", "--base", "A,B,C", "--generators", "D=ABC"]),
    case("friedman", &["{in}friedman.csv", "--columns", "a,b,c"]),
    case("gagerr", &["{in}gage.csv", "--measurement", "measure", "--part", "part", "--operator", "operator"]),
    case("gagetype1", &["{in}linearity.csv", "--measurement", "measurement", "--reference", "1", "--lsl", "0", "--usl", "2"]),
    case("gchart", &["{in}rare.csv", "--column", "events", "--times", "day"]),
    case("gof", &["{in}gof.csv", "--column", "count", "--expected", "20,20,20,20,20"]),
    case("grubbs", &["{in}outlier.csv", "--column", "x", "--alpha", "5"]),
    case("help", &[]),
    case("histogram", &["{in}hist.csv", "--column", "x", "--bins", "8"]),
    case("intervalplot", &["{in}groups.csv", "--response", "y", "--factor", "g"]),
    case("kmeans", &["{in}kmeans.csv", "--columns", "a,b", "--clusters", "3", "--seed", "2026"]),
    case("kruskal", &["{in}ranks.csv", "--response", "y", "--factor", "g"]),
    case("levene", &["{in}variance.csv", "--response", "y", "--factor", "g", "--center", "median"]),
    case("linearity", &["{in}linearity.csv", "--measurement", "measurement", "--reference", "reference"]),
    case("logistic", &["{in}logistic.csv", "--response", "y", "--predictors", "x1,x2"]),
    case("lognormalrel", &["{in}survival.csv", "--time", "time", "--censor", "censor", "--censor-value", "0"]),
    case("matrixplot", &["{in}matrix.csv", "--columns", "a,b,c"]),
    case("mood", &["{in}ranks.csv", "--response", "y", "--factor", "g"]),
    case("movingaverage", &["{in}normal.csv", "--column", "data", "--window", "3"]),
    case("mwu", &["{in}two.csv", "--columns", "a,b"]),
    case("normality", &["{in}normal.csv", "--column", "data", "--test", "ks"]),
    case("occurve", &["--sample", "20", "--accept", "1", "--lot", "1000"]),
    case("ordinal", &["{in}ordinal.csv", "--response", "y", "--predictors", "x1,x2"]),
    case("pareto", &["{in}contingency.csv", "--column", "row"]),
    case("partialcorr", &["{in}partial.csv", "--columns", "x,y,z"]),
    case("pbdesign", &["{out}pb.csv", "--runs", "12"]),
    case("pca", &["{in}pca.csv", "--columns", "a,b,c", "--keep", "2"]),
    case("plugin", &["data-quality", "{in}normal.csv"]),
    case("plugins", &[]),
    case("poisson", &["{in}poisson.csv", "--response", "y", "--predictors", "x1,x2"]),
    case("power", &["--test", "t1", "--effect", "0.5", "--n", "20", "--alpha", "5", "--power", "80"]),
    case("prop1", &["{in}prop.csv", "--column", "pass", "--p0", "0.5", "--success", "1", "--method", "exact"]),
    case("prop2", &["{in}prop.csv", "--columns", "pass,retest", "--success", "1"]),
    case("regress", &["{in}curve.csv", "--response", "y", "--predictors", "x"]),
    case("reliability", &["{in}survival.csv", "--time", "time", "--censor", "censor", "--censor-value", "0"]),
    case("rsm", &["{in}rsm.csv", "--response", "y", "--factors", "A,B", "--optimize", "--contours"]),
    case("runchart", &["{in}normal.csv", "--column", "data"]),
    case("sampleplan", &["--aql", "1", "--ltpd", "5", "--alpha", "5", "--beta", "10", "--lot", "1000"]),
    case("scatter", &["{in}two.csv", "--x", "a", "--y", "b"]),
    case("seriesplot", &["{in}series.csv", "--column", "value", "--time", "time"]),
    case("signtest", &["{in}sign.csv", "--column", "diff"]),
    case("smooth", &["{in}series.csv", "--column", "value", "--season", "12", "--horizon", "4"]),
    case("sort", &["{in}manip.csv", "{out}sorted.csv", "--by", "a:desc"]),
    case("stack", &["{in}manip.csv", "{out}stacked.csv", "--columns", "a,b"]),
    case("stemleaf", &["{in}normal.csv", "--column", "data"]),
    case("stepstress", &["{in}step.csv", "--duration", "duration", "--stress", "stress", "--unit", "unit", "--event", "event"]),
    case("stepwise", &["{in}matrix.csv", "--response", "c", "--predictors", "a,b"]),
    case("subset", &["{in}manip.csv", "{out}subset.csv", "--column", "g", "--op", "eq", "--value", "A"]),
    case("t2chart", &["{in}t2.csv", "--columns", "a,b,c", "--subgroup", "4"]),
    case("taguchi", &["{in}taguchi.csv", "--response", "response", "--factors", "A,B,C"]),
    case("taguchidesign", &["{out}taguchi.csv", "--runs", "18", "--factors", "8"]),
    case("tolerance", &["{in}tolerance.csv", "--column", "x", "--proportion", "95", "--confidence", "95"]),
    case("transpose", &["{in}manip.csv", "{out}transposed.csv"]),
    case("ttest1", &["{in}normal.csv", "--column", "data", "--mean", "20"]),
    case("ttest2", &["{in}two.csv", "--columns", "a,b"]),
    case("ttestpaired", &["{in}two.csv", "--columns", "a,b"]),
    case("unstack", &["{in}manip.csv", "{out}unstacked.csv", "--value", "a", "--group", "g"]),
    case("var1", &["{in}normal.csv", "--column", "data", "--variance", "9"]),
    case("var2", &["{in}two.csv", "--columns", "a,b", "--method", "bonett"]),
    case("version", &[]),
    case("weibullrel", &["{in}survival.csv", "--time", "time", "--censor", "censor", "--censor-value", "0"]),
    case("wilcoxon", &["{in}two.csv", "--columns", "a,b"]),
    case("xlsx", &["{in}normal.csv", "--output", "{out}normal.xlsx"]),
    case("xlsximport", &["{in}normal.xlsx", "--list-sheets"]),
];

struct Outcome {
    status: Option<i32>,
    stderr: String,
    timed_out: bool,
}

fn run(argv: &[String], dir: &Path) -> Outcome {
    let out_path = dir.join("stdout.txt");
    let err_path = dir.join("stderr.txt");
    let stdout = File::create(&out_path).expect("create stdout file");
    let stderr = File::create(&err_path).expect("create stderr file");
    let mut child = Command::new(bin())
        .args(argv)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .unwrap_or_else(|e| panic!("cannot start {}: {e}", bin()));
    let started = Instant::now();
    let status = loop {
        match child.try_wait().expect("wait for the process") {
            Some(status) => break Some(status.code()),
            None if started.elapsed() > TIMEOUT => {
                child.kill().ok();
                child.wait().ok();
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    let timed_out = status.is_none();
    Outcome {
        status: status.flatten(),
        stderr: fs::read_to_string(&err_path).unwrap_or_default(),
        timed_out,
    }
}

fn resolve(token: &str, out: &Path) -> String {
    if let Some(rest) = token.strip_prefix("{in}") {
        fixtures().join(rest).display().to_string()
    } else if let Some(rest) = token.strip_prefix("{out}") {
        out.join(rest).display().to_string()
    } else {
        token.to_owned()
    }
}

fn context(outcome: &Outcome) -> String {
    let status = outcome
        .status
        .map_or_else(|| "no status".to_owned(), |code| code.to_string());
    format!("status {status}\nstderr:\n{}", outcome.stderr.trim_end())
}

fn assert_no_crash(label: &str, outcome: &Outcome) {
    assert!(!outcome.timed_out, "{label}: did not finish in 30 s");
    assert!(
        !outcome.stderr.contains("panicked at"),
        "{label}: panicked\n{}",
        context(outcome)
    );
    match outcome.status {
        Some(101) => panic!("{label}: panic exit code\n{}", context(outcome)),
        Some(code) if code >= 128 => {
            panic!("{label}: killed by a signal\n{}", context(outcome));
        }
        Some(_) => {}
        None => panic!("{label}: ended without an exit code\n{}", context(outcome)),
    }
}

fn assert_rejected(label: &str, outcome: &Outcome) {
    assert_no_crash(label, outcome);
    assert_eq!(
        outcome.status,
        Some(1),
        "{label}: expected exit status 1\n{}",
        context(outcome)
    );
    assert!(
        !outcome.stderr.trim().is_empty(),
        "{label}: rejected without a message"
    );
}

/// `(option index, option, value)` for every `--option value` pair.
fn pairs(argv: &[String]) -> Vec<(usize, &str, &str)> {
    let mut out = Vec::new();
    let mut index = 0;
    while index + 1 < argv.len() {
        let option = argv[index].as_str();
        let value = argv[index + 1].as_str();
        if option.starts_with("--") && !value.starts_with("--") {
            out.push((index, option, value));
            index += 2;
        } else {
            index += 1;
        }
    }
    out
}

const COLUMN_OPTIONS: &[&str] = &[
    "--column",
    "--columns",
    "--by",
    "--group",
    "--response",
    "--factor",
    "--factor-a",
    "--factor-b",
    "--factors",
    "--predictors",
    "--x",
    "--y",
    "--row",
    "--time",
    "--censor",
    "--measurement",
    "--reference",
    "--part",
    "--appraiser",
    "--operator",
    "--rating",
    "--first",
    "--second",
    "--times",
    "--sizes",
    "--stress",
    "--duration",
    "--unit",
    "--event",
    "--mode",
    "--value",
];

const ENUM_OPTIONS: &[&str] = &[
    "--distribution",
    "--alternative",
    "--test",
    "--method",
    "--model",
    "--criterion",
    "--layout",
    "--display",
    "--p-value",
    "--chart",
    "--op",
    "--center",
    "--quality",
];

const NUMERIC_OPTIONS: &[&str] = &[
    "--mean",
    "--confidence",
    "--margin",
    "--variance",
    "--p0",
    "--success",
    "--lsl",
    "--usl",
    "--target",
    "--subgroup",
    "--cusum-k",
    "--cusum-h",
    "--ewma-lambda",
    "--ewma-l",
    "--window",
    "--period",
    "--season",
    "--horizon",
    "--reps",
    "--alpha",
    "--power",
    "--n",
    "--effect",
    "--null",
    "--bins",
    "--keep",
    "--clusters",
    "--seed",
    "--max-steps",
    "--alpha-enter",
    "--alpha-remove",
    "--runs",
    "--replicates",
    "--center-points",
    "--block-size",
    "--sample",
    "--accept",
    "--lot",
    "--aql",
    "--ltpd",
    "--beta",
    "--days",
    "--use-stress",
    "--censor-value",
    "--size",
    "--proportion",
    "--reference",
];

fn looks_numeric(value: &str) -> bool {
    value.parse::<f64>().is_ok()
}

/// A value that names a column: not a number, not an enum word such as
/// `weibull`, and not an expression such as `D=ABC`.
fn looks_like_column(value: &str) -> bool {
    !looks_numeric(value)
        && !value.is_empty()
        && !value.contains('=')
        && value.chars().any(|c| c.is_ascii_alphabetic())
        && !value.contains('*')
}

fn numeric_pair(argv: &[String]) -> Option<(usize, &str, &str)> {
    pairs(argv).into_iter().find(|(_, option, value)| {
        (NUMERIC_OPTIONS.contains(option) || *option == "--factors") && looks_numeric(value)
    })
}

#[derive(Clone, Copy)]
enum Mutation {
    Empty,
    Constant,
    OneRow,
    Nan,
    Inf,
}

impl Mutation {
    fn name(self) -> &'static str {
        match self {
            Mutation::Empty => "empty",
            Mutation::Constant => "constant",
            Mutation::OneRow => "one-row",
            Mutation::Nan => "nan",
            Mutation::Inf => "inf",
        }
    }
}

fn write_mutation(kind: Mutation, src: &Path, dest: &Path) {
    if matches!(kind, Mutation::Empty) {
        fs::write(dest, "").expect("write empty fixture");
        return;
    }
    let text = fs::read_to_string(src).unwrap_or_else(|e| panic!("read {}: {e}", src.display()));
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("");
    let separator = if header.contains('\t') {
        '\t'
    } else if header.contains(';') {
        ';'
    } else {
        ','
    };
    let rows: Vec<Vec<&str>> = lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split(separator).collect())
        .collect();
    let keep = if matches!(kind, Mutation::OneRow) {
        rows.len().min(1)
    } else {
        rows.len()
    };
    let mut out = String::new();
    out.push_str(header);
    out.push('\n');
    for row in &rows[..keep] {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| {
                let trimmed = cell.trim();
                if trimmed.parse::<f64>().is_err() {
                    return (*cell).to_owned();
                }
                match kind {
                    Mutation::Constant => "5".to_owned(),
                    Mutation::Nan => "NaN".to_owned(),
                    Mutation::Inf => "inf".to_owned(),
                    Mutation::Empty | Mutation::OneRow => trimmed.to_owned(),
                }
            })
            .collect();
        out.push_str(&cells.join(&separator.to_string()));
        out.push('\n');
    }
    fs::write(dest, out).expect("write mutated fixture");
}

fn exercise(case: &Case, dir: &Path) {
    let raw: Vec<&str> = std::iter::once(case.command)
        .chain(case.argv.iter().copied())
        .collect();
    let valid: Vec<String> = raw.iter().map(|token| resolve(token, dir)).collect();
    let inputs: Vec<usize> = raw
        .iter()
        .enumerate()
        .filter(|(_, token)| token.starts_with("{in}"))
        .map(|(index, _)| index)
        .collect();
    let pairs = pairs(&valid);

    let label = |what: &str| format!("{} {what}", case.command);

    let outcome = run(&valid, dir);
    assert_no_crash(&label("valid"), &outcome);
    assert_eq!(
        outcome.status,
        Some(0),
        "{}: the reference invocation failed\n{}",
        case.command,
        context(&outcome)
    );

    if let Some(&index) = inputs.first() {
        let missing = dir.join("__missing__.csv");
        let mut argv = valid.clone();
        argv[index] = missing.display().to_string();
        assert_rejected(&label("missing file"), &run(&argv, dir));

        let source = PathBuf::from(&valid[index]);
        if source.extension().and_then(|e| e.to_str()) == Some("csv") {
            for kind in [
                Mutation::Empty,
                Mutation::Constant,
                Mutation::OneRow,
                Mutation::Nan,
                Mutation::Inf,
            ] {
                let dest = dir.join(format!("{}-{}.csv", case.command, kind.name()));
                write_mutation(kind, &source, &dest);
                let mut argv = valid.clone();
                argv[index] = dest.display().to_string();
                let outcome = run(&argv, dir);
                assert_no_crash(&label(kind.name()), &outcome);
            }
        }
    }

    // Design builders take factor names as labels, not as columns of a file,
    // so a name that does not exist is not an error there.
    if !inputs.is_empty()
        && let Some(option_index) = pairs.iter().find_map(|(index, option, value)| {
            (COLUMN_OPTIONS.contains(option) && looks_like_column(value)).then_some(*index)
        })
    {
        let mut argv = valid.clone();
        argv[option_index + 1] = "__no_such_column__".to_owned();
        assert_rejected(&label("bad column"), &run(&argv, dir));
    }

    if let Some(option_index) = pairs
        .iter()
        .find_map(|(index, option, _)| ENUM_OPTIONS.contains(option).then_some(*index))
    {
        let mut argv = valid.clone();
        argv[option_index + 1] = "banana".to_owned();
        assert_rejected(&label("bad enum"), &run(&argv, dir));
    }

    if let Some((option_index, _, _)) = numeric_pair(&valid) {
        let mut argv = valid.clone();
        argv[option_index + 1] = "abc".to_owned();
        assert_rejected(&label("non-numeric"), &run(&argv, dir));

        let mut zero = valid.clone();
        zero[option_index + 1] = "0".to_owned();
        assert_no_crash(&label("zero"), &run(&zero, dir));

        let mut extreme = valid.clone();
        extreme[option_index + 1] = "1e308".to_owned();
        assert_no_crash(&label("extreme"), &run(&extreme, dir));
    }

    let dangling = match pairs.last().copied() {
        Some((option_index, _, _)) => {
            let mut argv = valid.clone();
            argv.truncate(option_index + 1);
            argv
        }
        None => {
            let mut argv = valid.clone();
            argv.push("--lang".to_owned());
            argv
        }
    };
    assert_rejected(&label("dangling value"), &run(&dangling, dir));
}

#[test]
fn every_cli_command_survives_hostile_inputs() {
    let dir = temp_dir("matrix");
    for case in MATRIX {
        exercise(case, &dir);
    }
    fs::remove_dir_all(&dir).ok();
}

/// Parses the dispatcher so every command arm must appear in `MATRIX`.
fn dispatched_commands(source: &str) -> Vec<&str> {
    let mut names = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("Some(") else {
            continue;
        };
        let Some(end) = rest.find(") =>") else {
            continue;
        };
        let head = &rest[..end];
        let Some(first) = head.split('|').next() else {
            continue;
        };
        let first = first.trim();
        let Some(name) = first.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
            continue;
        };
        names.push(match name {
            "-V" | "--version" => "version",
            other => other,
        });
    }
    // The help arm is written as `None | Some("help" | ...)`.
    names.push("help");
    names
}

#[test]
fn every_dispatched_command_is_in_the_matrix() {
    let source = include_str!("../src/cli.rs");
    let dispatched = dispatched_commands(source);
    assert!(
        dispatched.len() >= 90,
        "only {} commands parsed",
        dispatched.len()
    );
    for name in &dispatched {
        assert!(
            MATRIX.iter().any(|case| case.command == *name),
            "command {name:?} has no hostile-input case in MATRIX"
        );
    }
    for case in MATRIX {
        assert!(
            dispatched.contains(&case.command),
            "MATRIX names {:?}, which the dispatcher does not have",
            case.command
        );
    }
    let mut seen = Vec::new();
    for case in MATRIX {
        assert!(
            !seen.contains(&case.command),
            "MATRIX has two cases for {:?}",
            case.command
        );
        seen.push(case.command);
    }
}
