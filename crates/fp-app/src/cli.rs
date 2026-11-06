//! Command-line interface.
//!
//! It is deliberately thin: reads arguments, calls the engine and writes files.
//! No formula here. It also serves as the E2E path for the tests, which
//! run the real binary instead of calling internal functions.

use std::path::PathBuf;

use std::sync::atomic::AtomicU8;

use fp_core::attribute::AttributeChartKind;
use fp_core::curvefit::CurveModel;
use fp_core::hypothesis::Alternative;
use fp_core::io;
use fp_core::plot::render_svg;
use fp_core::power::PowerProcedure;
use fp_core::proportion::ProportionMethod;
use fp_core::spc::ControlChartKind;
use fp_core::variance::{VarianceCenter, VarianceMethod};
use fp_core::workflow::{
    AnalysisSpec, DistributionKind, PValueMethod as CorePValueMethod, PlotDisplay, PlotLayout,
    run_analysis,
};

use crate::platform;

/// Seed for the p-value Monte Carlo (fixed: reproducible results).
pub const MC_SEED: u64 = 2026;

const USAGE: &str = "\
FreePlots {version}

Usage:
  FreePlots analyze <file.csv> [options]
  FreePlots describe <file.csv> [--column <name>] [--by <name>]
  FreePlots normality <file.csv> [--column <name>] [--test <ad|ks|rj|k2>]
  FreePlots distcompare <file.csv> [--column <name>]
  FreePlots capability <file.csv> [--column <name>] --lsl <x> --usl <x> [--target <x>] [--distribution <normal|lognormal|loglogistic|weibull|exponential|logistic|gamma|extremevalue|weibull3>] [--subgroup <n>] [--confidence <pct>] [--bootstrap <n>] [--box-cox]
  FreePlots controlchart <file.csv> --column <name> --chart <imr|xbar-r|xbar-s|ewma|cusum> [--subgroup <n>] [--subgroup-column <name>] [--phase <name>] [--tests <1:8|1,3,5>] [--sensitivity <test=K,...>] [--cusum-k <k>] [--cusum-h <h>] [--ewma-lambda <λ>] [--ewma-l <L>] [--svg <file>]
  FreePlots attrchart <file.csv> --column <defects> --chart <p|np|c|u> [--sizes <name>] [--size <n>] [--svg <file>]
  FreePlots runchart <file.csv> [--column <name>] [--svg <file>]
  FreePlots movingaverage <file.csv> --column <name> [--window <n>] [--svg <file>]
  FreePlots decompose <file.csv> --column <name> [--period <n>] [--model <additive|multiplicative>] [--svg <file>]
  FreePlots smooth <file.csv> --column <name> [--season <n>] [--horizon <n>] [--svg <file>]
  FreePlots ttest1 <file.csv> --column <name> [--mean <x>] [--alternative <two-sided|less|greater>] [--confidence <pct>]
  FreePlots ttest2 <file.csv> --columns <a,b> [--pooled] [--alternative <two-sided|less|greater>] [--confidence <pct>]
  FreePlots ttestpaired <file.csv> --columns <a,b> [--alternative <two-sided|less|greater>] [--confidence <pct>]
  FreePlots equiv1 <file.csv> --column <name> --margin <x> [--mean <x>] [--confidence <pct>]
  FreePlots equiv2 <file.csv> --columns <a,b> --margin <x> [--confidence <pct>]
  FreePlots equivpaired <file.csv> --columns <a,b> --margin <x> [--confidence <pct>]
  FreePlots mwu <file.csv> --columns <a,b> [--alternative <two-sided|less|greater>]
  FreePlots wilcoxon <file.csv> --columns <a,b> [--alternative <two-sided|less|greater>]
  FreePlots kruskal <file.csv> --response <y> --factor <f>
  FreePlots friedman <file.csv> --columns <a,b,c>
  FreePlots mood <file.csv> --response <y> --factor <f>
  FreePlots alt <file.csv> --time <name> --stress <name> [--censor <name>] [--censor-value <x>] [--use-stress <x>] [--mode <name>] [--horizon <x>] [--distribution <weibull|lognormal|exponential|normal|logistic|smallest-extreme>] [--model <arrhenius|inverse-power|exponential>] [--svg <file>]
  FreePlots stepstress <file.csv> --duration <name> --stress <name> --unit <name> --event <name> [--use-stress <x>] [--distribution <weibull|lognormal|exponential|normal|logistic|smallest-extreme>] [--model <arrhenius|inverse-power|exponential>] [--svg <file>]
  FreePlots desirability <file.csv> --factors <A,B> --response <name>:<goal>:<low>:<high>[:target=<t>][:weight=<w>][:importance=<i>] [--svg <file>]
  FreePlots calc <file.csv> --name <new> --expression <formula> [--output <file.csv>]
  FreePlots datediff <file.csv> --first <date> --second <date> [--name <new>] [--output <file.csv>]
  FreePlots dateadd <file.csv> --column <date> --days <n> [--name <new>] [--output <file.csv>]
  FreePlots signtest <file.csv> --column <name> [--median <x>]
  FreePlots seriesplot <file.csv> --column <name> [--time <name>] [--svg <file>]
  FreePlots stemleaf <file.csv> --column <name>
  FreePlots ecdf <file.csv> --column <name> [--confidence <pct>] [--svg <file>]
  FreePlots dotplot <file.csv> --column <name> [--by <factor>] [--svg <file>]
  FreePlots matrixplot <file.csv> --columns <a,b,c> [--bins <n>] [--svg <file>]
  FreePlots intervalplot <file.csv> --response <y> --factor <f> [--confidence <pct>] [--svg <file>]
  FreePlots pareto <file.csv> --column <name> [--svg <file>]
  FreePlots taguchidesign <output.csv> --runs <n> [--factors <n>] [--randomize] [--seed <n>]   (n = 8, 9, 12, 16, 18, 27)
  FreePlots taguchi <file.csv> --response <name> --factors <A,B> [--quality <smaller|larger|nominal>]
  FreePlots occurve --sample <n> --accept <c> [--lot <N>] [--svg <file>]
  FreePlots sampleplan --aql <pct> --ltpd <pct> [--alpha <pct>] [--beta <pct>] [--svg <file>]
  FreePlots t2chart <file.csv> --columns <a,b> [--subgroup <n>] [--confidence <pct>] [--svg <file>]
  FreePlots pca <file.csv> --columns <a,b,c,...> [--keep <n>] [--svg <file>]
  FreePlots kmeans <file.csv> --columns <a,b> --clusters <n> [--raw] [--seed <n>] [--svg <file>]
  FreePlots correlate <file.csv> --columns <a,b,c>
  FreePlots curvefit <file.csv> --x <name> --y <name> [--model <polynomial|poly2..poly5|exponential|power>] [--confidence <pct>] [--svg <file>]
  FreePlots anova2 <file.csv> --response <y> --factor-a <a> --factor-b <b>
  FreePlots crosstab <file.csv> --row <name> --column <name>
  FreePlots gchart <file.csv> --column <name> [--times <name>]
  FreePlots partialcorr <file.csv> --columns <x,y,z>
  FreePlots boxcox <file.csv> --column <name>
  FreePlots tolerance <file.csv> --column <name> [--proportion <pct>] [--confidence <pct>] [--method <normal|free>]
  FreePlots levene <file.csv> --response <y> --factor <f> [--center <mean|median>]
  FreePlots bartlett <file.csv> --response <y> --factor <f>
  FreePlots gof <file.csv> --column <name> --expected <v1,v2,...> [--parameters <n>]
  FreePlots grubbs <file.csv> --column <name> [--alpha <pct>]
  FreePlots power --test <t1|t2|proportion|correlation> --effect <x> [--null <p>] [--n <n>] [--alpha <pct>] [--power <pct>] [--alternative <two-sided|less|greater>]
  FreePlots prop1 <file.csv> --column <name> [--p0 <p>] [--success <0|1>] [--method <exact|normal>] [--alternative <two-sided|less|greater>] [--confidence <pct>]
  FreePlots prop2 <file.csv> --columns <a,b> [--success <0|1>] [--alternative <two-sided|less|greater>] [--confidence <pct>]
  FreePlots var1 <file.csv> --column <name> [--variance <x>] [--alternative <two-sided|less|greater>] [--confidence <pct>]
  FreePlots var2 <file.csv> --columns <a,b> [--method <f|bonett>] [--alternative <two-sided|less|greater>] [--confidence <pct>]
  FreePlots regress <file.csv> --response <name> --predictors <a,b> [--confidence <pct>] [--svg <file>]
  FreePlots stepwise <file.csv> --response <name> --predictors <a,b,...> [--method <forward|backward|stepwise>] [--criterion <p|aic|bic>] [--alpha-enter <p>] [--alpha-remove <p>] [--max-steps <n>] [--confidence <pct>] [--svg <file>]
  FreePlots logistic <file.csv> --response <name> --predictors <a,b>
  FreePlots poisson <file.csv> --response <name> --predictors <a,b>
  FreePlots ordinal <file.csv> --response <name> --predictors <a,b>
  FreePlots anova <file.csv> --response <name> --factor <name> [--confidence <pct>] [--svg <file>]
  FreePlots reliability <file.csv> --time <name> [--censor <name>] [--censor-value <x>] [--confidence <pct>] [--svg <file>]
  FreePlots weibullrel <file.csv> --time <name> [--censor <name>] [--censor-value <x>] [--svg <file>]
  FreePlots exporeliability <file.csv> --time <name> [--censor <name>] [--censor-value <x>] [--svg <file>]
  FreePlots lognormalrel <file.csv> --time <name> [--censor <name>] [--censor-value <x>] [--svg <file>]
  FreePlots linearity <file.csv> --measurement <name> --reference <column>
  FreePlots agreement <file.csv> --part <name> --appraiser <name> --rating <name>
  FreePlots gagerr <file.csv> --measurement <name> --part <name> --operator <name>
  FreePlots doe <file.csv> --response <name> --factors <A,B> [--main-effects] [--svg <file>]
  FreePlots doedesign <output.csv> --factors <A,B> [--replicates <n>] [--center-points <n>] [--block-size <n>] [--randomize] [--seed <n>]
  FreePlots fracdesign <output.csv> --base <A,B,C> --generators <D=ABC> [--replicates <n>] [--randomize] [--seed <n>]
  FreePlots doealias --base <A,B,C> --generators <D=ABC>
  FreePlots xlsx <file.csv> [--output <file.xlsx>]
  FreePlots xlsximport <book.xlsx> [--sheet <name>] [--list-sheets] [--output <file.csv>]
  FreePlots rsm <file.csv> --response <name> --factors <A,B> [--optimize] [--contours] [--svg <file>]
  FreePlots boxplot <file.csv> [--column <name>] [--by <name>] [--svg <file>]
  FreePlots scatter <file.csv> --x <name> --y <name> [--no-fit] [--svg <file>]
  FreePlots histogram <file.csv> [--column <name>] [--bins <n>] [--svg <file>]
  FreePlots ccd <output.csv> --factors <A,B> [--center-points <n>] [--alpha <rotatable|face|x>] [--randomize] [--seed <n>]
  FreePlots sort <in.csv> <out.csv> --by <col[:asc|desc],...>
  FreePlots transpose <in.csv> <out.csv>
  FreePlots stack <in.csv> <out.csv> --columns <a,b>
  FreePlots unstack <in.csv> <out.csv> --value <name> --group <name>
  FreePlots subset <in.csv> <out.csv> --column <name> --op <op> --value <text>
  FreePlots bbdesign <output.csv> --factors <A,B,C> [--center-points <n>] [--randomize] [--seed <n>]
  FreePlots pbdesign <output.csv> --runs <n> [--randomize] [--seed <n>]
  FreePlots doctor
  FreePlots --version
  FreePlots help

Desktop and integration:
  FreePlots --gui [file.csv] [--renderer <glow|wgpu>]  open the desktop window
  FreePlots --bridge <host:port> [--token <secret>]   live automation bridge
  FreePlots --mcp                                     MCP server over stdin/stdout
  FreePlots --mcp-http <host:port> [--token <secret>] MCP server over HTTP
  FreePlots --mcp-live <host:port> [--token <secret>] MCP proxy to a live window

  A non-loopback address (0.0.0.0) is refused without --token.

Options for every command:
  --lang <en|pt>    language of the report (default: en)

Options for analyze:
  --column <name>    Y column; repeat for a multi-Y overlay
  --columns <a,b>    comma-separated Y columns (alternative to repeating)
  --group <name>     group column for a single-Y overlay
  --layout <separate|overlay>
                     multi-Y layout (default: separate panels)
  --same-x           common X scale across panels
  --same-y           common Y scale across panels
  --distribution <normal|lognormal|loglogistic|weibull|exponential|logistic|gamma|extremevalue|weibull3>
                     distribution for the probability plot (default: normal)
  --display <both|symbols|fit>
                     what to draw (default: both)
  --svg <file>       export the probability plot as SVG
  --project <file>   save the dataset as a .fpp project
  --p-value <ad|mc>  p-value method: composite-AD approximation (default)
                     or Monte Carlo cross-check
  --reps <n>         Monte Carlo replications (with --p-value mc; default: 2000)

Example:
  FreePlots analyze weights.csv --column C1 --svg weights.svg
";

/// CLI entry point.
/// The language of the reports, chosen with `--lang`.
static LANGUAGE: AtomicU8 = AtomicU8::new(0);

/// The language of the reports.
fn language() -> fp_core::i18n::Lang {
    if LANGUAGE.load(std::sync::atomic::Ordering::Relaxed) == 1 {
        fp_core::i18n::Lang::Portuguese
    } else {
        fp_core::i18n::Lang::English
    }
}

/// Sets the language of the reports.
fn set_language(lang: fp_core::i18n::Lang) {
    let value = match lang {
        fp_core::i18n::Lang::English => 0,
        fp_core::i18n::Lang::Portuguese => 1,
    };
    LANGUAGE.store(value, std::sync::atomic::Ordering::Relaxed);
}

/// Removes the `--lang` options from the arguments and returns the language.
///
/// The option is accepted by every command, so the reports can be printed in
/// Portuguese without repeating the option in each command.
fn split_language(args: &[String]) -> Result<(Vec<String>, fp_core::i18n::Lang), String> {
    let mut rest = Vec::with_capacity(args.len());
    let mut lang = language();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--lang" {
            let code = args
                .get(index + 1)
                .ok_or_else(|| "--lang requires a language code (en or pt)".to_owned())?;
            lang = fp_core::i18n::Lang::parse(code)
                .ok_or_else(|| format!("invalid --lang: {code} (use en or pt)"))?;
            index += 2;
            continue;
        }
        if let Some(code) = arg.strip_prefix("--lang=") {
            lang = fp_core::i18n::Lang::parse(code)
                .ok_or_else(|| format!("invalid --lang: {code} (use en or pt)"))?;
            index += 1;
            continue;
        }
        rest.push(arg.clone());
        index += 1;
    }
    Ok((rest, lang))
}

/// Runs an analysis and applies the language of the command line.
fn run_local(
    df: &fp_core::data::DataFrame,
    spec: &AnalysisSpec,
) -> Result<fp_core::workflow::AnalysisOutput, fp_core::error::Error> {
    let mut out = run_analysis(df, spec)?;
    let lang = language();
    for result in &mut out.results {
        result.set_lang(lang);
    }
    Ok(out)
}

pub fn run(args: &[String]) -> Result<(), String> {
    let (args, lang) = split_language(args)?;
    set_language(lang);
    let args = args.as_slice();
    match args.first().map(String::as_str) {
        None | Some("help" | "-h" | "--help") => {
            print!("{}", USAGE.replace("{version}", fp_core::VERSION));
            Ok(())
        }
        Some("-V" | "--version" | "version") => {
            println!("FreePlots {}", fp_core::VERSION);
            Ok(())
        }
        Some("doctor") => doctor(),
        Some("analyze") => analyze(&args[1..]),
        Some("describe") => describe(&args[1..]),
        Some("normality") => normality(&args[1..]),
        Some("distcompare") => distcompare(&args[1..]),
        Some("capability") => capability(&args[1..]),
        Some("controlchart" | "control-chart" | "spc") => controlchart(&args[1..]),
        Some("attrchart" | "attribute-chart") => attrchart(&args[1..]),
        Some("runchart" | "run-chart") => runchart(&args[1..]),
        Some("movingaverage" | "ma") => moving_average(&args[1..]),
        Some("decompose" | "decomposition") => decompose(&args[1..]),
        Some("smooth" | "expsmooth") => exponential_smoothing(&args[1..]),
        Some("ttest1" | "t-test1" | "ttest") => ttest1(&args[1..]),
        Some("ttest2" | "t-test2") => ttest2(&args[1..]),
        Some("ttestpaired" | "t-test-paired" | "ttest-paired") => ttest_paired(&args[1..]),
        Some("equiv1" | "equiv") => equiv1(&args[1..]),
        Some("equiv2") => equiv2(&args[1..]),
        Some("equivpaired" | "equiv-paired") => equiv_paired(&args[1..]),
        Some("mwu" | "mann-whitney") => mann_whitney(&args[1..]),
        Some("wilcoxon" | "signed-rank") => wilcoxon(&args[1..]),
        Some("kruskal" | "kruskal-wallis") => kruskal(&args[1..]),
        Some("friedman") => friedman(&args[1..]),
        Some("mood" | "moods-median") => mood(&args[1..]),
        Some("alt" | "accelerated-life") => accelerated_life(&args[1..]),
        Some("stepstress" | "step-stress" | "alt-step-stress") => step_stress(&args[1..]),
        Some("desirability" | "optimizer" | "response-optimizer") => desirability(&args[1..]),
        Some("calc" | "calculator") => calc(&args[1..]),
        Some("datediff" | "date-difference") => date_difference(&args[1..]),
        Some("dateadd" | "date-add" | "dateshift") => date_add_days(&args[1..]),
        Some("signtest" | "sign-test") => sign_test(&args[1..]),
        Some("seriesplot" | "tsplot") => series_plot(&args[1..]),
        Some("stemleaf" | "stem-and-leaf") => stem_leaf(&args[1..]),
        Some("ecdf") => ecdf(&args[1..]),
        Some("dotplot") => dotplot(&args[1..]),
        Some("matrixplot" | "matrix-plot") => matrix_plot(&args[1..]),
        Some("intervalplot" | "interval-plot") => interval_plot(&args[1..]),
        Some("pareto") => pareto(&args[1..]),
        Some("taguchidesign" | "taguchi-design") => taguchi_design(&args[1..]),
        Some("taguchi") => taguchi(&args[1..]),
        Some("occurve" | "acceptance") => acceptance_curve(&args[1..]),
        Some("sampleplan" | "sampling-plan") => sampling_plan(&args[1..]),
        Some("t2chart" | "hotelling") => t2_chart(&args[1..]),
        Some("pca" | "principal-components") => pca(&args[1..]),
        Some("kmeans" | "cluster") => kmeans(&args[1..]),
        Some("correlate" | "correlation") => correlate(&args[1..]),
        Some("curvefit" | "curve-fit") => curve_fit(&args[1..]),
        Some("anova2" | "two-way-anova") => anova_two(&args[1..]),
        Some("crosstab" | "chi2") => crosstab(&args[1..]),
        Some("gchart" | "g-chart" | "rare") => g_chart(&args[1..]),
        Some("partialcorr" | "partial-correlation") => partial_correlation(&args[1..]),
        Some("boxcox" | "box-cox") => box_cox(&args[1..]),
        Some("tolerance" | "tolint") => tolerance(&args[1..]),
        Some("levene") => levene(&args[1..]),
        Some("bartlett") => bartlett(&args[1..]),
        Some("gof" | "goodness-of-fit") => goodness_of_fit(&args[1..]),
        Some("grubbs") => grubbs(&args[1..]),
        Some("power" | "powerandsample") => power(&args[1..]),
        Some("prop1" | "proportion1" | "prop-1") => prop1(&args[1..]),
        Some("prop2" | "proportion2" | "prop-2") => prop2(&args[1..]),
        Some("var1" | "variance1" | "var-1") => var1(&args[1..]),
        Some("var2" | "variance2" | "var-2") => var2(&args[1..]),
        Some("regress" | "regression") => regress(&args[1..]),
        Some("stepwise" | "stepwise-regression") => stepwise(&args[1..]),
        Some("logistic" | "logit") => logistic(&args[1..]),
        Some("poisson") => poisson(&args[1..]),
        Some("ordinal" | "ordinal-logistic") => ordinal(&args[1..]),
        Some("anova" | "one-way-anova") => anova(&args[1..]),
        Some("reliability" | "kaplan-meier" | "km") => reliability(&args[1..]),
        Some("weibullrel" | "weibull-reliability") => weibullrel(&args[1..]),
        Some("exporeliability" | "exponential-reliability" | "exp-rel") => {
            exporeliability(&args[1..])
        }
        Some("lognormalrel" | "lognormal-reliability") => lognormalrel(&args[1..]),
        Some("gagerr" | "gage-rr" | "gage") => gagerr(&args[1..]),
        Some("gagetype1" | "gage-type1") => gagetype1(&args[1..]),
        Some("linearity" | "gage-linearity") => gage_linearity(&args[1..]),
        Some("agreement" | "kappa") => agreement(&args[1..]),
        Some("doe" | "factorial" | "doe-factorial") => doe(&args[1..]),
        Some("doedesign" | "doe-design") => doedesign(&args[1..]),
        Some("fracdesign" | "fractional-design") => fracdesign(&args[1..]),
        Some("doealias" | "alias") => doealias(&args[1..]),
        Some("xlsx" | "excel") => xlsx(&args[1..]),
        Some("xlsximport" | "xlsx-in" | "import-xlsx") => xlsx_import(&args[1..]),
        Some("rsm" | "response-surface") => rsm(&args[1..]),
        Some("boxplot" | "box-plot") => boxplot(&args[1..]),
        Some("scatter" | "scatterplot") => scatter(&args[1..]),
        Some("histogram" | "hist") => histogram(&args[1..]),
        Some("ccd" | "central-composite") => ccd(&args[1..]),
        Some("bbdesign" | "box-behnken") => box_behnken(&args[1..]),
        Some("pbdesign" | "plackett-burman") => plackett_burman(&args[1..]),
        Some("sort") => sort_rows(&args[1..]),
        Some("transpose" | "t") => transpose(&args[1..]),
        Some("stack") => stack_columns(&args[1..]),
        Some("unstack") => unstack_column(&args[1..]),
        Some("subset" | "filter") => subset_rows(&args[1..]),
        Some("plugins") => plugins(&args[1..]),
        Some("plugin") => plugin(&args[1..]),
        Some(other) => Err(format!("unknown command: {other}\n\n{USAGE}")),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PValueMethod {
    Ad,
    MonteCarlo,
}

struct AnalyzeArgs {
    csv: PathBuf,
    columns: Vec<String>,
    group: Option<String>,
    svg: Option<PathBuf>,
    project: Option<PathBuf>,
    pdf: Option<PathBuf>,
    p_value: PValueMethod,
    reps: usize,
    distribution: DistributionKind,
    lang: fp_core::i18n::Lang,
    display: PlotDisplay,
    layout: PlotLayout,
    same_x: bool,
    same_y: bool,
}

fn parse_analyze_args(args: &[String]) -> Result<AnalyzeArgs, String> {
    let mut csv: Option<PathBuf> = None;
    let mut columns: Vec<String> = Vec::new();
    let mut group = None;
    let mut svg = None;
    let mut project = None;
    let mut pdf = None;
    let mut p_value = PValueMethod::Ad;
    let mut reps = 2000usize;
    let mut distribution = DistributionKind::Normal;
    let mut lang = fp_core::i18n::Lang::English;
    let mut display = PlotDisplay::Both;
    let mut layout = PlotLayout::Separate;
    let mut same_x = false;
    let mut same_y = false;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                columns.push(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--group" => {
                group = Some(
                    it.next()
                        .ok_or_else(|| "--group requires a name".to_owned())?
                        .clone(),
                );
            }
            "--columns" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--columns requires a comma-separated list".to_owned())?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        columns.push(name.to_owned());
                    }
                }
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            "--pdf" => {
                pdf = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--pdf requires a path".to_owned())?,
                ));
            }
            "--project" => {
                project = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--project requires a path".to_owned())?,
                ));
            }
            "--lang" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--lang requires en or pt".to_owned())?;
                lang = fp_core::i18n::Lang::parse(raw)
                    .ok_or_else(|| format!("invalid --lang: {raw} (use en or pt)"))?;
            }
            "--distribution" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--distribution requires a distribution name".to_owned())?;
                distribution = match raw.as_str() {
                    "normal" | "n" => DistributionKind::Normal,
                    "lognormal" | "ln" => DistributionKind::Lognormal,
                    "weibull" | "w" => DistributionKind::Weibull,
                    "exponential" | "exp" => DistributionKind::Exponential,
                    "logistic" | "log" => DistributionKind::Logistic,
                    "loglogistic" | "llog" => DistributionKind::Loglogistic,
                    "gamma" | "g" => DistributionKind::Gamma,
                    "extremevalue" | "ev" | "gumbel" => DistributionKind::ExtremeValue,
                    "weibull3" | "weibull-3" => DistributionKind::Weibull3,
                    "lognormal3" | "lognormal-3" => DistributionKind::Lognormal3,
                    other => {
                        return Err(format!(
                            "invalid --distribution: {other} (use normal, lognormal, loglogistic, weibull, exponential, logistic, gamma, extremevalue, weibull3 or lognormal3)"
                        ));
                    }
                };
            }
            "--display" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--display requires both, symbols or fit".to_owned())?;
                display = match raw.as_str() {
                    "both" => PlotDisplay::Both,
                    "symbols" => PlotDisplay::Symbols,
                    "fit" => PlotDisplay::Fit,
                    other => {
                        return Err(format!(
                            "invalid --display: {other} (use both, symbols or fit)"
                        ));
                    }
                };
            }
            "--same-x" => same_x = true,
            "--same-y" => same_y = true,
            "--layout" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--layout requires separate or overlay".to_owned())?;
                layout = match raw.as_str() {
                    "separate" | "panels" => PlotLayout::Separate,
                    "overlay" => PlotLayout::Overlay,
                    other => {
                        return Err(format!(
                            "invalid --layout: {other} (use separate or overlay)"
                        ));
                    }
                };
            }
            "--p-value" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--p-value requires ad or mc".to_owned())?;
                p_value = match raw.as_str() {
                    "ad" | "exact" => PValueMethod::Ad,
                    "mc" | "monte-carlo" => PValueMethod::MonteCarlo,
                    other => {
                        return Err(format!("invalid --p-value: {other} (use ad or mc)"));
                    }
                };
            }
            "--reps" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--reps requires a number".to_owned())?;
                reps = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --reps: {raw}"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }

    Ok(AnalyzeArgs {
        csv: csv.ok_or_else(|| "missing CSV file".to_owned())?,
        columns,
        group,
        svg,
        project,
        pdf,
        p_value,
        reps,
        distribution,
        lang,
        display,
        layout,
        same_x,
        same_y,
    })
}

fn analyze(args: &[String]) -> Result<(), String> {
    let parsed = parse_analyze_args(args)?;

    let (df, encoding) = io::read_csv_file(&parsed.csv).map_err(|e| e.to_string())?;

    let columns = if parsed.columns.is_empty() {
        vec![
            fp_core::workflow::first_numeric_column(&df)
                .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
                .to_owned(),
        ]
    } else {
        parsed.columns
    };

    let p_value = match parsed.p_value {
        PValueMethod::Ad => CorePValueMethod::Ad,
        PValueMethod::MonteCarlo => CorePValueMethod::MonteCarlo {
            reps: parsed.reps,
            seed: MC_SEED,
        },
    };
    let mut spec = AnalysisSpec::probability_plot_multi(columns)
        .with_distribution(parsed.distribution)
        .with_display(parsed.display)
        .with_layout(parsed.layout)
        .with_same_scale(parsed.same_x, parsed.same_y)
        .with_p_value(p_value);
    if let Some(group) = parsed.group {
        spec = spec.with_group(group);
    }
    let mut analysis = run_local(&df, &spec).map_err(|e| e.to_string())?;
    for result in &mut analysis.results {
        result.set_lang(parsed.lang);
    }

    let mut results = analysis.results;
    for result in &mut results {
        result
            .notes
            .insert(1, ("Detected encoding".to_owned(), encoding.to_owned()));
    }
    let spec = analysis
        .plot
        .clone()
        .ok_or_else(|| "this analysis does not produce a graph".to_owned())?;

    for (index, result) in results.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print!("{}", result.to_session_text());
    }

    if let Some(path) = &parsed.svg {
        std::fs::write(path, render_svg(&spec)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }

    if let Some(path) = &parsed.pdf {
        let session: String = results
            .iter()
            .map(|result| result.to_session_text())
            .collect::<Vec<_>>()
            .join("\n");
        let bytes = fp_core::pdf::session_pdf("FreePlots Session", &session, Some(&spec));
        std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("report written: {}", path.display());
    }

    if let Some(path) = &parsed.project {
        io::save_project(&df, path, &parsed.csv.display().to_string())
            .map_err(|e| e.to_string())?;
        eprintln!("project written: {}", path.display());
    }

    Ok(())
}

fn describe(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut by: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--by" => {
                by = Some(
                    it.next()
                        .ok_or_else(|| "--by requires a name".to_owned())?
                        .clone(),
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = match by {
        Some(by) => AnalysisSpec::descriptive_statistics_by(column, by),
        None => AnalysisSpec::descriptive_statistics(column),
    };
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    for (index, result) in out.results.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print!("{}", result.to_session_text());
    }
    Ok(())
}

fn normality(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut test = fp_core::workflow::NormalityTestKind::AndersonDarling;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--test" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--test requires ad, ks, rj or k2".to_owned())?;
                test = fp_core::workflow::NormalityTestKind::parse(raw)
                    .ok_or_else(|| format!("invalid --test: {raw} (use ad, ks, rj or k2)"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::normality_test(column).with_normality_test(test);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    for (index, result) in out.results.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print!("{}", result.to_session_text());
    }
    Ok(())
}

fn distcompare(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::distribution_comparison(column);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    for (index, result) in out.results.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print!("{}", result.to_session_text());
    }
    Ok(())
}

fn capability(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut lsl: Option<f64> = None;
    let mut usl: Option<f64> = None;
    let mut target: Option<f64> = None;
    let mut subgroup = 1usize;
    let mut confidence = 0.95;
    let mut bootstrap = 0usize;
    let mut box_cox = false;
    let mut require_stability = false;
    let mut distribution = DistributionKind::Normal;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--box-cox" => box_cox = true,
            "--require-stability" => require_stability = true,
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            "--bootstrap" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--bootstrap requires a number".to_owned())?;
                bootstrap = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --bootstrap: {raw}"))?;
            }
            "--lsl" | "--usl" | "--target" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a number"))?;
                let value = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid {arg}: {raw}"))?;
                match arg.as_str() {
                    "--lsl" => lsl = Some(value),
                    "--usl" => usl = Some(value),
                    _ => target = Some(value),
                }
            }
            "--subgroup" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--subgroup requires a number".to_owned())?;
                subgroup = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --subgroup: {raw}"))?;
            }
            "--distribution" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--distribution requires a name".to_owned())?;
                distribution = match raw.as_str() {
                    "normal" | "n" => DistributionKind::Normal,
                    "lognormal" | "ln" => DistributionKind::Lognormal,
                    "weibull" | "w" => DistributionKind::Weibull,
                    "exponential" | "exp" => DistributionKind::Exponential,
                    "logistic" | "log" => DistributionKind::Logistic,
                    "loglogistic" | "llog" => DistributionKind::Loglogistic,
                    "gamma" | "g" => DistributionKind::Gamma,
                    "extremevalue" | "ev" | "gumbel" => DistributionKind::ExtremeValue,
                    "weibull3" | "weibull-3" => DistributionKind::Weibull3,
                    "lognormal3" | "lognormal-3" => DistributionKind::Lognormal3,
                    other => {
                        return Err(format!(
                            "invalid --distribution: {other} (use normal, lognormal, loglogistic, weibull, exponential, logistic, gamma, extremevalue, weibull3 or lognormal3)"
                        ));
                    }
                };
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                let parsed = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --confidence: {raw}"))?;
                let parsed = if parsed > 1.0 { parsed / 100.0 } else { parsed };
                if !(0.5..1.0).contains(&parsed) {
                    return Err(format!(
                        "invalid --confidence: {raw} (expected 50–99.9 % or 0.5–0.999)"
                    ));
                }
                confidence = parsed;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let mut spec = AnalysisSpec::capability(column, lsl, usl)
        .with_capability_distribution(distribution)
        .with_capability_target(target)
        .with_capability_bootstrap(bootstrap)
        .with_capability_box_cox(box_cox)
        .with_capability_stability(require_stability);
    if let AnalysisSpec::Capability {
        subgroup: size,
        confidence: level,
        ..
    } = &mut spec
    {
        *size = subgroup;
        *level = confidence;
    }
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    for (index, result) in out.results.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print!("{}", result.to_session_text());
    }
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn controlchart(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut kind: Option<ControlChartKind> = None;
    let mut subgroup = 1usize;
    let mut tests = fp_core::spc::TestSet::default();
    let mut cusum_reference = 0.5f64;
    let mut cusum_decision = 4.0f64;
    let mut ewma_lambda = 0.2f64;
    let mut ewma_limit = 3.0f64;
    let mut subgroup_column: Option<String> = None;
    let mut phase_column: Option<String> = None;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--subgroup-column" | "--group" => {
                subgroup_column = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--phase" => {
                phase_column = Some(
                    it.next()
                        .ok_or_else(|| "--phase requires a column name".to_owned())?
                        .clone(),
                );
            }
            "--tests" => {
                let raw = it.next().ok_or_else(|| {
                    "--tests requires a list such as 1:8, 1 3 5 or none".to_owned()
                })?;
                tests = if raw.eq_ignore_ascii_case("none") {
                    fp_core::spc::TestSet::none()
                } else {
                    fp_core::spc::TestSet::parse_list(raw).map_err(|e| e.to_string())?
                };
            }
            "--sensitivity" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--sensitivity requires one or more test=K pairs".to_owned())?;
                for pair in raw.split(',') {
                    let (test, value) = pair.split_once('=').ok_or_else(|| {
                        format!("invalid --sensitivity: {pair} (use test=K, e.g. 2=8)")
                    })?;
                    let test: u8 = test
                        .trim()
                        .parse()
                        .map_err(|_| format!("invalid test number in --sensitivity: {pair}"))?;
                    let value: u8 = value
                        .trim()
                        .parse()
                        .map_err(|_| format!("invalid sensitivity in --sensitivity: {pair}"))?;
                    tests = tests
                        .with_sensitivity(test, value)
                        .map_err(|e| e.to_string())?;
                }
            }
            "--chart" => {
                let raw = it.next().ok_or_else(|| {
                    "--chart requires imr, xbar-r, xbar-s, ewma or cusum".to_owned()
                })?;
                kind = Some(ControlChartKind::parse(raw).ok_or_else(|| {
                    format!("invalid --chart: {raw} (use imr, xbar-r, xbar-s, ewma or cusum)")
                })?);
            }
            "--subgroup" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--subgroup requires a number".to_owned())?;
                subgroup = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --subgroup: {raw}"))?;
            }
            "--cusum-k" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--cusum-k requires a number".to_owned())?;
                cusum_reference = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --cusum-k: {raw}"))?;
            }
            "--cusum-h" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--cusum-h requires a number".to_owned())?;
                cusum_decision = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --cusum-h: {raw}"))?;
            }
            "--ewma-lambda" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--ewma-lambda requires a number".to_owned())?;
                ewma_lambda = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --ewma-lambda: {raw}"))?;
            }
            "--ewma-l" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--ewma-l requires a number".to_owned())?;
                ewma_limit = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --ewma-l: {raw}"))?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let kind =
        kind.ok_or_else(|| "missing --chart (imr, xbar-r, xbar-s, ewma or cusum)".to_owned())?;
    if subgroup_column.is_some()
        && !matches!(kind, ControlChartKind::XbarR | ControlChartKind::XbarS)
    {
        return Err("--subgroup-column is only used by the xbar-r and xbar-s charts".to_owned());
    }
    if phase_column.is_some()
        && !matches!(
            kind,
            ControlChartKind::IndividualsMovingRange
                | ControlChartKind::XbarR
                | ControlChartKind::XbarS
        )
    {
        return Err("--phase is only used by the imr, xbar-r and xbar-s charts".to_owned());
    }
    if phase_column.is_some() && subgroup_column.is_some() {
        return Err("choose either --phase or --subgroup-column, not both".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let subgroup = if kind == ControlChartKind::IndividualsMovingRange {
        1
    } else {
        subgroup
    };
    let cusum = fp_core::spc::CusumOptions::new(cusum_reference, cusum_decision)
        .map_err(|e| e.to_string())?;
    let ewma =
        fp_core::spc::EwmaOptions::new(ewma_lambda, ewma_limit).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::control_chart(column, kind, subgroup)
        .with_control_chart_tests(tests)
        .with_cusum_options(cusum)
        .with_ewma_options(ewma)
        .with_subgroup_column(subgroup_column)
        .with_phase_column(phase_column);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    for (index, result) in out.results.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print!("{}", result.to_session_text());
    }
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn runchart(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let out = run_analysis(&df, &AnalysisSpec::run_chart(column)).map_err(|e| e.to_string())?;
    for (index, result) in out.results.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print!("{}", result.to_session_text());
    }
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn parse_alternative(raw: &str) -> Result<Alternative, String> {
    Alternative::parse(raw)
        .ok_or_else(|| format!("invalid --alternative: {raw} (use two-sided, less or greater)"))
}

fn parse_confidence(raw: &str) -> Result<f64, String> {
    let parsed = raw
        .parse::<f64>()
        .map_err(|_| format!("invalid --confidence: {raw}"))?;
    let parsed = if parsed > 1.0 { parsed / 100.0 } else { parsed };
    if !(0.5..1.0).contains(&parsed) {
        return Err(format!(
            "invalid --confidence: {raw} (expected 50–99.9 % or 0.5–0.999)"
        ));
    }
    Ok(parsed)
}

fn print_analysis(out: &fp_core::workflow::AnalysisOutput) {
    for (index, result) in out.results.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print!("{}", result.to_session_text());
    }
}

fn ttest1(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut null_mean = 0.0;
    let mut alternative = Alternative::TwoSided;
    let mut confidence = 0.95;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--mean" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--mean requires a number".to_owned())?;
                null_mean = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --mean: {raw}"))?;
            }
            "--alternative" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--alternative requires a value".to_owned())?;
                alternative = parse_alternative(raw)?;
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = parse_confidence(raw)?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::one_sample_t(column, null_mean)
        .with_alternative(alternative)
        .with_t_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

type TwoColumnArgs = (PathBuf, String, String, bool, Alternative, f64, f64);

fn parse_two_columns(args: &[String], allow_pooled: bool) -> Result<TwoColumnArgs, String> {
    let mut csv: Option<PathBuf> = None;
    let mut columns: Vec<String> = Vec::new();
    let mut pooled = false;
    let mut alternative = Alternative::TwoSided;
    let mut confidence = 0.95;
    let mut success = 1.0;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                columns.push(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--columns" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--columns requires a comma-separated pair".to_owned())?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        columns.push(name.to_owned());
                    }
                }
            }
            "--pooled" if allow_pooled => pooled = true,
            "--success" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--success requires 0 or 1".to_owned())?;
                success = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --success: {raw}"))?;
            }
            "--alternative" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--alternative requires a value".to_owned())?;
                alternative = parse_alternative(raw)?;
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = parse_confidence(raw)?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    if columns.len() != 2 {
        return Err("specify exactly two columns with --columns a,b".to_owned());
    }
    let second = columns.pop().expect("two columns");
    let first = columns.pop().expect("two columns");
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    Ok((csv, first, second, pooled, alternative, confidence, success))
}

fn ttest2(args: &[String]) -> Result<(), String> {
    let (csv, first, second, pooled, alternative, confidence, _) = parse_two_columns(args, true)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::two_sample_t(first, second)
        .with_pooled(pooled)
        .with_alternative(alternative)
        .with_t_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn ttest_paired(args: &[String]) -> Result<(), String> {
    let (csv, first, second, _, alternative, confidence, _) = parse_two_columns(args, false)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::paired_t(first, second)
        .with_alternative(alternative)
        .with_t_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn equiv1(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut null_mean = 0.0;
    let mut margin: Option<f64> = None;
    let mut confidence = 0.95;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--mean" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--mean requires a number".to_owned())?;
                null_mean = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --mean: {raw}"))?;
            }
            "--margin" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--margin requires a number".to_owned())?;
                margin = Some(
                    raw.parse::<f64>()
                        .map_err(|_| format!("invalid --margin: {raw}"))?,
                );
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = parse_confidence(raw)?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let margin = margin.ok_or_else(|| "--margin is required".to_owned())?;
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::one_sample_equivalence(column, null_mean, margin)
        .with_equivalence_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn equiv2(args: &[String]) -> Result<(), String> {
    let (csv, first, second, margin, confidence) = parse_equivalence_columns(args)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::two_sample_equivalence(first, second, margin)
        .with_equivalence_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn equiv_paired(args: &[String]) -> Result<(), String> {
    let (csv, first, second, margin, confidence) = parse_equivalence_columns(args)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::paired_equivalence(first, second, margin)
        .with_equivalence_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

type EquivalenceArgs = (PathBuf, String, String, f64, f64);

fn parse_equivalence_columns(args: &[String]) -> Result<EquivalenceArgs, String> {
    let mut csv: Option<PathBuf> = None;
    let mut columns: Vec<String> = Vec::new();
    let mut margin: Option<f64> = None;
    let mut confidence = 0.95;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                columns.push(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--columns" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--columns requires a comma-separated pair".to_owned())?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        columns.push(name.to_owned());
                    }
                }
            }
            "--margin" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--margin requires a number".to_owned())?;
                margin = Some(
                    raw.parse::<f64>()
                        .map_err(|_| format!("invalid --margin: {raw}"))?,
                );
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = parse_confidence(raw)?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    if columns.len() != 2 {
        return Err("specify exactly two columns with --columns a,b".to_owned());
    }
    let margin = margin.ok_or_else(|| "--margin is required".to_owned())?;
    let second = columns.pop().expect("two columns");
    let first = columns.pop().expect("two columns");
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    Ok((csv, first, second, margin, confidence))
}

fn mann_whitney(args: &[String]) -> Result<(), String> {
    let (csv, first, second, _, alternative, _, _) = parse_two_columns(args, false)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::mann_whitney(first, second).with_alternative(alternative);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn wilcoxon(args: &[String]) -> Result<(), String> {
    let (csv, first, second, _, alternative, _, _) = parse_two_columns(args, false)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::wilcoxon_signed_rank(first, second).with_alternative(alternative);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn kruskal(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut factor: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--response" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| "--response requires a name".to_owned())?
                        .clone(),
                );
            }
            "--factor" => {
                factor = Some(
                    it.next()
                        .ok_or_else(|| "--factor requires a name".to_owned())?
                        .clone(),
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "--response is required".to_owned())?;
    let factor = factor.ok_or_else(|| "--factor is required".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::kruskal_wallis(response, factor);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn friedman(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut columns: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                columns.push(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--columns" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--columns requires a comma-separated list".to_owned())?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        columns.push(name.to_owned());
                    }
                }
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    if columns.len() < 2 {
        return Err("specify at least two columns with --columns a,b".to_owned());
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::friedman(columns);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn mood(args: &[String]) -> Result<(), String> {
    let (csv, response, factor, _) = parse_variance_test_args(args)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::moods_median(response, factor);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn correlate(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut columns: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                columns.push(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--columns" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--columns requires a comma-separated list".to_owned())?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        columns.push(name.to_owned());
                    }
                }
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    if columns.len() < 2 {
        return Err("specify at least two columns with --columns a,b".to_owned());
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::correlation(columns);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn curve_fit(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut x_column: Option<String> = None;
    let mut y_column: Option<String> = None;
    let mut model = CurveModel::Polynomial(2);
    let mut confidence = 0.95;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--x" => {
                x_column = Some(
                    it.next()
                        .ok_or_else(|| "--x requires a name".to_owned())?
                        .clone(),
                );
            }
            "--y" => {
                y_column = Some(
                    it.next()
                        .ok_or_else(|| "--y requires a name".to_owned())?
                        .clone(),
                );
            }
            "--model" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--model requires a name".to_owned())?;
                model = CurveModel::parse(raw).ok_or_else(|| {
                    format!("invalid --model: {raw} (use polynomial, poly2..poly5, exponential or power)")
                })?;
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = parse_confidence(raw)?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a file name".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let x_column = x_column.ok_or_else(|| "--x is required".to_owned())?;
    let y_column = y_column.ok_or_else(|| "--y is required".to_owned())?;
    let spec =
        AnalysisSpec::curve_fit(x_column, y_column, model).with_curve_fit_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn anova_two(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut first: Option<String> = None;
    let mut second: Option<String> = None;
    let mut interaction = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--interaction" => interaction = true,
            "--response" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| "--response requires a name".to_owned())?
                        .clone(),
                );
            }
            "--factor-a" => {
                first = Some(
                    it.next()
                        .ok_or_else(|| "--factor-a requires a name".to_owned())?
                        .clone(),
                );
            }
            "--factor-b" => {
                second = Some(
                    it.next()
                        .ok_or_else(|| "--factor-b requires a name".to_owned())?
                        .clone(),
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "--response is required".to_owned())?;
    let first = first.ok_or_else(|| "--factor-a is required".to_owned())?;
    let second = second.ok_or_else(|| "--factor-b is required".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::two_way_anova(response, first, second).with_interaction(interaction);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn crosstab(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut row: Option<String> = None;
    let mut column: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--row" => {
                row = Some(
                    it.next()
                        .ok_or_else(|| "--row requires a name".to_owned())?
                        .clone(),
                );
            }
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let row = row.ok_or_else(|| "--row is required".to_owned())?;
    let column = column.ok_or_else(|| "--column is required".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::chi_square_independence(row, column);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn g_chart(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut times: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--times" => {
                times = Some(
                    it.next()
                        .ok_or_else(|| "--times requires a name".to_owned())?
                        .clone(),
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::rare_events(column, times);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn partial_correlation(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut columns: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                columns.push(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--columns" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--columns requires a comma-separated list".to_owned())?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        columns.push(name.to_owned());
                    }
                }
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    if columns.len() != 3 {
        return Err("specify exactly three columns with --columns x,y,z".to_owned());
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let control = columns.pop().expect("three columns");
    let y_column = columns.pop().expect("three columns");
    let x_column = columns.pop().expect("three columns");
    let spec = AnalysisSpec::partial_correlation(x_column, y_column, control);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn box_cox(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::box_cox(column);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn tolerance(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut proportion = 95.0;
    let mut confidence = 95.0;
    let mut free = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--proportion" | "--content" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a percentage"))?;
                proportion = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid {arg}: {raw}"))?;
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --confidence: {raw}"))?;
            }
            "--method" | "--distribution-free" => {
                let raw = it.next().cloned().unwrap_or_else(|| "free".to_owned());
                free = match raw.as_str() {
                    "free" | "distribution-free" | "nonparametric" => true,
                    "normal" => false,
                    other => return Err(format!("invalid --method: {other}")),
                };
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::tolerance_interval(column)
        .with_tolerance_proportion(proportion / 100.0)
        .with_tolerance_confidence(confidence / 100.0)
        .with_tolerance_free(free);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn levene(args: &[String]) -> Result<(), String> {
    let (csv, response, factor, center) = parse_variance_test_args(args)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::levene(response, factor).with_variance_center(center);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn bartlett(args: &[String]) -> Result<(), String> {
    let (csv, response, factor, _) = parse_variance_test_args(args)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::bartlett(response, factor);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

type VarianceTestArgs = (PathBuf, String, String, VarianceCenter);

fn parse_variance_test_args(args: &[String]) -> Result<VarianceTestArgs, String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut factor: Option<String> = None;
    let mut center = VarianceCenter::Mean;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--response" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| "--response requires a name".to_owned())?
                        .clone(),
                );
            }
            "--factor" => {
                factor = Some(
                    it.next()
                        .ok_or_else(|| "--factor requires a name".to_owned())?
                        .clone(),
                );
            }
            "--center" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--center requires mean or median".to_owned())?;
                center = VarianceCenter::parse(raw)
                    .ok_or_else(|| format!("invalid --center: {raw} (use mean or median)"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "--response is required".to_owned())?;
    let factor = factor.ok_or_else(|| "--factor is required".to_owned())?;
    Ok((csv, response, factor, center))
}

type GoodnessOfFitArgs = (PathBuf, Option<String>, Vec<f64>, usize);

fn parse_goodness_of_fit_args(args: &[String]) -> Result<GoodnessOfFitArgs, String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut expected: Vec<f64> = Vec::new();
    let mut parameters = 0usize;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--expected" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--expected requires a comma-separated list".to_owned())?;
                for part in raw.split(',') {
                    let part = part.trim();
                    if part.is_empty() {
                        continue;
                    }
                    let value = part
                        .parse::<f64>()
                        .map_err(|_| format!("invalid --expected value: {part}"))?;
                    expected.push(value);
                }
            }
            "--parameters" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--parameters requires a number".to_owned())?;
                parameters = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --parameters: {raw}"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    if expected.len() < 2 {
        return Err("--expected requires at least two counts".to_owned());
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    Ok((csv, column, expected, parameters))
}

fn goodness_of_fit(args: &[String]) -> Result<(), String> {
    let (csv, column, expected, parameters) = parse_goodness_of_fit_args(args)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::chi_square_goodness_of_fit(column, expected)
        .with_estimated_parameters(parameters);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

type GrubbsArgs = (PathBuf, Option<String>, f64);

fn parse_grubbs_args(args: &[String]) -> Result<GrubbsArgs, String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut alpha = 0.05;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--alpha" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--alpha requires a percentage".to_owned())?;
                let percentage = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --alpha: {raw}"))?;
                alpha = percentage / 100.0;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    Ok((csv, column, alpha))
}

fn grubbs(args: &[String]) -> Result<(), String> {
    let (csv, column, alpha) = parse_grubbs_args(args)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::grubbs(column).with_grubbs_alpha(alpha);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

type PowerArgs = (
    PowerProcedure,
    f64,
    f64,
    usize,
    usize,
    f64,
    f64,
    Alternative,
);

fn parse_power_args(args: &[String]) -> Result<PowerArgs, String> {
    let mut procedure: Option<PowerProcedure> = None;
    let mut effect: Option<f64> = None;
    let mut null_value = 0.0;
    let mut n = 0usize;
    let mut groups = 2usize;
    let mut alpha = 0.05;
    let mut power = 0.8;
    let mut alternative = Alternative::TwoSided;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--test" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--test requires a name".to_owned())?;
                procedure = Some(PowerProcedure::parse(raw).ok_or_else(|| {
                    format!("invalid --test: {raw} (use t1, t2, proportion or correlation)")
                })?);
            }
            "--effect" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--effect requires a number".to_owned())?;
                effect = Some(
                    raw.parse::<f64>()
                        .map_err(|_| format!("invalid --effect: {raw}"))?,
                );
            }
            "--null" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--null requires a proportion".to_owned())?;
                null_value = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --null: {raw}"))?;
            }
            "--n" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--n requires a number".to_owned())?;
                n = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --n: {raw}"))?;
            }
            "--groups" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--groups requires a number".to_owned())?;
                groups = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --groups: {raw}"))?;
            }
            "--alpha" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--alpha requires a percentage".to_owned())?;
                let percentage = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --alpha: {raw}"))?;
                alpha = percentage / 100.0;
            }
            "--power" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--power requires a percentage".to_owned())?;
                let percentage = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --power: {raw}"))?;
                power = percentage / 100.0;
            }
            "--alternative" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--alternative requires a value".to_owned())?;
                alternative = parse_alternative(raw)?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                return Err(format!("unexpected argument: {path}"));
            }
        }
    }
    let procedure = procedure.ok_or_else(|| "--test is required".to_owned())?;
    let effect = effect.ok_or_else(|| "--effect is required".to_owned())?;
    Ok((
        procedure,
        effect,
        null_value,
        n,
        groups,
        alpha,
        power,
        alternative,
    ))
}

fn power(args: &[String]) -> Result<(), String> {
    let (procedure, effect, null_value, n, groups, alpha, power, alternative) =
        parse_power_args(args)?;
    let spec = AnalysisSpec::power(procedure, effect, n)
        .with_null_proportion(null_value)
        .with_power(power)
        .with_power_alpha(alpha)
        .with_power_groups(groups)
        .with_alternative(alternative);
    let df = fp_core::data::DataFrame::new(vec![]).map_err(|e| e.to_string())?;
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn prop1(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut null_p = 0.5;
    let mut success = 1.0;
    let mut method = ProportionMethod::Exact;
    let mut alternative = Alternative::TwoSided;
    let mut confidence = 0.95;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--p0" | "--p" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a proportion"))?;
                null_p = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid {arg}: {raw}"))?;
            }
            "--success" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--success requires 0 or 1".to_owned())?;
                success = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --success: {raw}"))?;
            }
            "--method" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--method requires exact or normal".to_owned())?;
                method = ProportionMethod::parse(raw)
                    .ok_or_else(|| format!("invalid --method: {raw} (use exact or normal)"))?;
            }
            "--alternative" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--alternative requires a value".to_owned())?;
                alternative = parse_alternative(raw)?;
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = parse_confidence(raw)?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::one_proportion(column, null_p)
        .with_success(success)
        .with_proportion_method(method)
        .with_alternative(alternative)
        .with_test_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn prop2(args: &[String]) -> Result<(), String> {
    let (csv, first, second, _, alternative, confidence, success) = parse_two_columns(args, false)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::two_proportions(first, second)
        .with_success(success)
        .with_alternative(alternative)
        .with_test_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn var1(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut null_variance = 1.0;
    let mut alternative = Alternative::TwoSided;
    let mut confidence = 0.95;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--variance" | "--var" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a variance"))?;
                null_variance = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid {arg}: {raw}"))?;
            }
            "--alternative" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--alternative requires a value".to_owned())?;
                alternative = parse_alternative(raw)?;
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = parse_confidence(raw)?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::one_variance(column, null_variance)
        .with_alternative(alternative)
        .with_test_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn var2(args: &[String]) -> Result<(), String> {
    let mut method = VarianceMethod::F;
    let mut rest: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--method" {
            let raw = it
                .next()
                .ok_or_else(|| "--method requires f or bonett".to_owned())?;
            method = VarianceMethod::parse(raw)
                .ok_or_else(|| format!("invalid --method: {raw} (use f or bonett)"))?;
        } else {
            rest.push(arg.clone());
        }
    }
    let (csv, first, second, _, alternative, confidence, _) = parse_two_columns(&rest, false)?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::two_variances(first, second)
        .with_variance_method(method)
        .with_alternative(alternative)
        .with_test_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn histogram(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut bins: Option<usize> = None;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--bins" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--bins requires a positive integer".to_owned())?;
                bins = Some(
                    raw.parse::<usize>()
                        .map_err(|_| format!("invalid --bins: {raw}"))?,
                );
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::histogram(column).with_bins(bins);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn boxplot(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut by: Option<String> = None;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--by" | "--group" => {
                by = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a name"))?
                        .clone(),
                );
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = match by {
        Some(by) => AnalysisSpec::boxplot_by(column, by),
        None => AnalysisSpec::boxplot(column),
    };
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn scatter(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut x: Option<String> = None;
    let mut y: Option<String> = None;
    let mut fit = true;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--x" | "--x-column" => {
                x = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a name"))?
                        .clone(),
                );
            }
            "--y" | "--y-column" => {
                y = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a name"))?
                        .clone(),
                );
            }
            "--no-fit" => fit = false,
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let x = x.ok_or_else(|| "missing --x <column>".to_owned())?;
    let y = y.ok_or_else(|| "missing --y <column>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let mut spec = AnalysisSpec::scatterplot(x, y);
    if let AnalysisSpec::Scatterplot { fit: slot, .. } = &mut spec {
        *slot = fit;
    }
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn rsm(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut factors: Vec<String> = Vec::new();
    let mut svg: Option<PathBuf> = None;
    let mut pdf: Option<PathBuf> = None;
    let mut optimize = false;
    let mut contours = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--response" | "--y" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--factors" | "--factor" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        factors.push(name.to_owned());
                    }
                }
            }
            "--optimize" | "--stationary" => optimize = true,
            "--contours" | "--contour" => contours = true,
            "--pdf" => {
                pdf = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--pdf requires a path".to_owned())?,
                ));
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "missing --response <column>".to_owned())?;
    if factors.is_empty() {
        return Err("missing --factors <A,B>".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::response_surface(response, factors)
        .with_rsm_optimization(optimize)
        .with_rsm_contours(contours);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let Some(path) = &pdf {
        let session: String = out
            .results
            .iter()
            .map(|result| result.to_session_text())
            .collect::<Vec<_>>()
            .join("\n");
        let bytes = fp_core::pdf::session_pdf("FreePlots Session", &session, out.plot.as_ref());
        std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("report written: {}", path.display());
    }
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn ccd(args: &[String]) -> Result<(), String> {
    let mut output: Option<PathBuf> = None;
    let mut factors: Vec<String> = Vec::new();
    let mut center_points = 6usize;
    let mut alpha = fp_core::doe::CcdAlpha::Rotatable;
    let mut randomize = false;
    let mut seed = MC_SEED;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--factors" | "--factor" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        factors.push(name.to_owned());
                    }
                }
            }
            "--center-points" | "--centers" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--center-points requires an integer".to_owned())?;
                center_points = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --center-points: {raw}"))?;
            }
            "--alpha" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--alpha requires rotatable, face or a number".to_owned())?;
                alpha = fp_core::doe::CcdAlpha::parse(raw)
                    .ok_or_else(|| format!("invalid --alpha: {raw}"))?;
            }
            "--randomize" => randomize = true,
            "--seed" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--seed requires an integer".to_owned())?;
                seed = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --seed: {raw}"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if output.is_some() {
                    return Err("only one output file is accepted".to_owned());
                }
                output = Some(PathBuf::from(path));
            }
        }
    }
    let output = output.ok_or_else(|| "missing output CSV file".to_owned())?;
    if factors.is_empty() {
        return Err("missing --factors <A,B>".to_owned());
    }
    let df =
        fp_core::doe::central_composite_design(&factors, center_points, alpha, randomize, seed)
            .map_err(|e| e.to_string())?;
    let csv = io::write_csv_string(&df, false).map_err(|e| e.to_string())?;
    std::fs::write(&output, csv).map_err(|e| format!("{}: {e}", output.display()))?;
    println!(
        "central composite design written: {} rows × {} columns — {}",
        df.n_rows(),
        df.n_columns(),
        output.display()
    );
    Ok(())
}

/// Writes a transformed worksheet.
fn write_frame(df: &fp_core::data::DataFrame, path: &PathBuf) -> Result<(), String> {
    let csv = io::write_csv_string(df, false).map_err(|e| e.to_string())?;
    std::fs::write(path, csv).map_err(|e| format!("{}: {e}", path.display()))?;
    println!(
        "worksheet written: {} rows x {} columns - {}",
        df.n_rows(),
        df.n_columns(),
        path.display()
    );
    Ok(())
}

/// Splits the two positional arguments (input and output) of a reshape command.
fn reshape_paths(args: &[String]) -> Result<(PathBuf, PathBuf, Vec<String>), String> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg.starts_with('-') {
            rest.push(arg.clone());
            if let Some(value) = it.next() {
                rest.push(value.clone());
            }
        } else {
            paths.push(PathBuf::from(arg));
        }
    }
    if paths.len() != 2 {
        return Err("expected an input CSV and an output CSV".to_owned());
    }
    Ok((paths[0].clone(), paths[1].clone(), rest))
}

fn option_string(options: &[String], name: &str) -> Option<String> {
    options
        .iter()
        .position(|value| value == name)
        .and_then(|index| options.get(index + 1))
        .cloned()
}

fn sort_rows(args: &[String]) -> Result<(), String> {
    let (input, output, options) = reshape_paths(args)?;
    let raw = option_string(&options, "--by").ok_or_else(|| "missing --by <col,...>".to_owned())?;
    let mut keys = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, direction) = match part.split_once(':') {
            Some((name, direction)) => (
                name.trim(),
                fp_core::reshape::SortDirection::parse(direction.trim())
                    .ok_or_else(|| format!("invalid direction: {direction}"))?,
            ),
            None => (part, fp_core::reshape::SortDirection::Ascending),
        };
        keys.push((name.to_owned(), direction));
    }
    if keys.is_empty() {
        return Err("missing --by <col,...>".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&input).map_err(|e| e.to_string())?;
    let sorted = fp_core::reshape::sort(&df, &keys).map_err(|e| e.to_string())?;
    write_frame(&sorted, &output)
}

fn transpose(args: &[String]) -> Result<(), String> {
    let (input, output, _options) = reshape_paths(args)?;
    let (df, _encoding) = io::read_csv_file(&input).map_err(|e| e.to_string())?;
    let transposed = fp_core::reshape::transpose(&df).map_err(|e| e.to_string())?;
    write_frame(&transposed, &output)
}

fn stack_columns(args: &[String]) -> Result<(), String> {
    let (input, output, options) = reshape_paths(args)?;
    let raw =
        option_string(&options, "--columns").ok_or_else(|| "missing --columns <a,b>".to_owned())?;
    let columns: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    let (df, _encoding) = io::read_csv_file(&input).map_err(|e| e.to_string())?;
    let stacked = fp_core::reshape::stack(&df, &columns).map_err(|e| e.to_string())?;
    write_frame(&stacked, &output)
}

fn unstack_column(args: &[String]) -> Result<(), String> {
    let (input, output, options) = reshape_paths(args)?;
    let value =
        option_string(&options, "--value").ok_or_else(|| "missing --value <name>".to_owned())?;
    let group =
        option_string(&options, "--group").ok_or_else(|| "missing --group <name>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&input).map_err(|e| e.to_string())?;
    let wide = fp_core::reshape::unstack(&df, &value, &group).map_err(|e| e.to_string())?;
    write_frame(&wide, &output)
}

fn subset_rows(args: &[String]) -> Result<(), String> {
    let (input, output, options) = reshape_paths(args)?;
    let column =
        option_string(&options, "--column").ok_or_else(|| "missing --column <name>".to_owned())?;
    let raw = option_string(&options, "--op").unwrap_or_else(|| "=".to_owned());
    let op =
        fp_core::reshape::SubsetOp::parse(&raw).ok_or_else(|| format!("invalid --op: {raw}"))?;
    let value =
        option_string(&options, "--value").ok_or_else(|| "missing --value <text>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&input).map_err(|e| e.to_string())?;
    let filtered = fp_core::reshape::subset(&df, &column, op, &value).map_err(|e| e.to_string())?;
    write_frame(&filtered, &output)
}

fn plackett_burman(args: &[String]) -> Result<(), String> {
    let mut output: Option<PathBuf> = None;
    let mut runs = 12usize;
    let mut randomize = false;
    let mut seed = MC_SEED;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--runs" | "--size" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--runs requires an integer".to_owned())?;
                runs = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --runs: {raw}"))?;
            }
            "--randomize" => randomize = true,
            "--seed" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--seed requires an integer".to_owned())?;
                seed = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --seed: {raw}"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if output.is_some() {
                    return Err("only one output file is accepted".to_owned());
                }
                output = Some(PathBuf::from(path));
            }
        }
    }
    let output = output.ok_or_else(|| "missing output CSV file".to_owned())?;
    let df =
        fp_core::doe::plackett_burman_design(runs, randomize, seed).map_err(|e| e.to_string())?;
    let csv = io::write_csv_string(&df, false).map_err(|e| e.to_string())?;
    std::fs::write(&output, csv).map_err(|e| format!("{}: {e}", output.display()))?;
    println!(
        "Plackett-Burman design written: {} rows x {} columns - {}",
        df.n_rows(),
        df.n_columns(),
        output.display()
    );
    Ok(())
}

fn box_behnken(args: &[String]) -> Result<(), String> {
    let mut output: Option<PathBuf> = None;
    let mut factors: Vec<String> = Vec::new();
    let mut center_points = 3usize;
    let mut randomize = false;
    let mut seed = MC_SEED;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--factors" | "--factor" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        factors.push(name.to_owned());
                    }
                }
            }
            "--center-points" | "--centers" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--center-points requires an integer".to_owned())?;
                center_points = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --center-points: {raw}"))?;
            }
            "--randomize" => randomize = true,
            "--seed" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--seed requires an integer".to_owned())?;
                seed = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --seed: {raw}"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if output.is_some() {
                    return Err("only one output file is accepted".to_owned());
                }
                output = Some(PathBuf::from(path));
            }
        }
    }
    let output = output.ok_or_else(|| "missing output CSV file".to_owned())?;
    if factors.is_empty() {
        return Err("missing --factors <A,B,C>".to_owned());
    }
    let df = fp_core::doe::box_behnken_design(&factors, center_points, randomize, seed)
        .map_err(|e| e.to_string())?;
    let csv = io::write_csv_string(&df, false).map_err(|e| e.to_string())?;
    std::fs::write(&output, csv).map_err(|e| format!("{}: {e}", output.display()))?;
    println!(
        "Box-Behnken design written: {} rows x {} columns - {}",
        df.n_rows(),
        df.n_columns(),
        output.display()
    );
    Ok(())
}

fn xlsx(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--output" | "-o" => {
                output = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--output requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let output = output.unwrap_or_else(|| {
        let stem = csv.file_stem().map_or_else(
            || "worksheet".to_owned(),
            |value| value.to_string_lossy().into_owned(),
        );
        csv.with_file_name(format!("{stem}.xlsx"))
    });
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let bytes = fp_core::xlsx::workbook(&df);
    std::fs::write(&output, bytes).map_err(|e| format!("{}: {e}", output.display()))?;
    println!("workbook written: {}", output.display());
    Ok(())
}

fn doealias(args: &[String]) -> Result<(), String> {
    let mut base: Vec<String> = Vec::new();
    let mut generators: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--base" | "--factors" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        base.push(name.to_owned());
                    }
                }
            }
            "--generators" | "--generator" => {
                let raw = it.next().ok_or_else(|| format!("{arg} requires D=ABC"))?;
                for generator in raw.split(',') {
                    let generator = generator.trim();
                    if !generator.is_empty() {
                        generators.push(generator.to_owned());
                    }
                }
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    if base.is_empty() || generators.is_empty() {
        return Err("use --base A,B,C --generators D=ABC".to_owned());
    }
    let groups = fp_core::doe::fractional_aliases(&base, &generators).map_err(|e| e.to_string())?;
    println!("Alias groups");
    for group in groups {
        println!("  {}", group.join(" = "));
    }
    Ok(())
}

fn fracdesign(args: &[String]) -> Result<(), String> {
    let mut output: Option<PathBuf> = None;
    let mut base: Vec<String> = Vec::new();
    let mut generators: Vec<String> = Vec::new();
    let mut replicates = 1usize;
    let mut randomize = false;
    let mut seed = MC_SEED;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--base" | "--factors" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        base.push(name.to_owned());
                    }
                }
            }
            "--generators" | "--generator" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires D=ABC,..."))?;
                for generator in raw.split(',') {
                    let generator = generator.trim();
                    if !generator.is_empty() {
                        generators.push(generator.to_owned());
                    }
                }
            }
            "--replicates" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--replicates requires a positive integer".to_owned())?;
                replicates = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --replicates: {raw}"))?;
            }
            "--randomize" => randomize = true,
            "--seed" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--seed requires an integer".to_owned())?;
                seed = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --seed: {raw}"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if output.is_some() {
                    return Err("only one output file is accepted".to_owned());
                }
                output = Some(PathBuf::from(path));
            }
        }
    }
    let output = output.ok_or_else(|| "missing output CSV file".to_owned())?;
    if base.is_empty() || generators.is_empty() {
        return Err("missing --base <A,B,C> or --generators <D=ABC>".to_owned());
    }
    let df =
        fp_core::doe::fractional_factorial_design(&base, &generators, replicates, randomize, seed)
            .map_err(|e| e.to_string())?;
    let csv = io::write_csv_string(&df, false).map_err(|e| e.to_string())?;
    std::fs::write(&output, csv).map_err(|e| format!("{}: {e}", output.display()))?;
    println!(
        "fractional design written: {} rows × {} columns — {}",
        df.n_rows(),
        df.n_columns(),
        output.display()
    );
    Ok(())
}

fn doedesign(args: &[String]) -> Result<(), String> {
    let mut output: Option<PathBuf> = None;
    let mut factors: Vec<String> = Vec::new();
    let mut replicates = 1usize;
    let mut center_points = 0usize;
    let mut block_size: Option<usize> = None;
    let mut generators: Vec<String> = Vec::new();
    let mut fold_over = false;
    let mut randomize = false;
    let mut seed = MC_SEED;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--factors" | "--factor" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        factors.push(name.to_owned());
                    }
                }
            }
            "--replicates" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--replicates requires a positive integer".to_owned())?;
                replicates = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --replicates: {raw}"))?;
            }
            "--center-points" | "--centers" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--center-points requires an integer".to_owned())?;
                center_points = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --center-points: {raw}"))?;
            }
            "--block-size" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--block-size requires a positive integer".to_owned())?;
                let value = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --block-size: {raw}"))?;
                if value == 0 {
                    return Err("--block-size must be greater than zero".to_owned());
                }
                block_size = Some(value);
            }
            "--generators" | "--generator" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        generators.push(name.to_owned());
                    }
                }
            }
            "--fold-over" => fold_over = true,
            "--randomize" => randomize = true,
            "--seed" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--seed requires an integer".to_owned())?;
                seed = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --seed: {raw}"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if output.is_some() {
                    return Err("only one output file is accepted".to_owned());
                }
                output = Some(PathBuf::from(path));
            }
        }
    }
    let output = output.ok_or_else(|| "missing output CSV file".to_owned())?;
    if factors.is_empty() {
        return Err("missing --factors <A,B>".to_owned());
    }
    let df = if fold_over {
        if generators.is_empty() {
            return Err("--fold-over needs --generators (for example D=ABC)".to_owned());
        }
        fp_core::doe::fold_over_design(&factors, &generators, randomize, seed)
            .map_err(|e| e.to_string())?
    } else if generators.is_empty() {
        fp_core::doe::blocked_factorial_design(
            &factors,
            replicates,
            center_points,
            block_size,
            randomize,
            seed,
        )
        .map_err(|e| e.to_string())?
    } else {
        fp_core::doe::fractional_factorial_design(
            &factors,
            &generators,
            replicates,
            randomize,
            seed,
        )
        .map_err(|e| e.to_string())?
    };
    let csv = io::write_csv_string(&df, false).map_err(|e| e.to_string())?;
    std::fs::write(&output, csv).map_err(|e| format!("{}: {e}", output.display()))?;
    println!(
        "design written: {} rows × {} columns — {}",
        df.n_rows(),
        df.n_columns(),
        output.display()
    );
    Ok(())
}

fn doe(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut factors: Vec<String> = Vec::new();
    let mut svg: Option<PathBuf> = None;
    let mut main_effects = false;
    let mut block: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--response" | "--y" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--factors" | "--factor" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        factors.push(name.to_owned());
                    }
                }
            }
            "--main-effects" | "--fractional" => main_effects = true,
            "--block" => {
                block = Some(
                    it.next()
                        .ok_or_else(|| "--block requires a column name".to_owned())?
                        .clone(),
                );
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "missing --response <column>".to_owned())?;
    if factors.is_empty() {
        return Err("missing --factors <A,B>".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let mut spec = AnalysisSpec::factorial(response, factors).with_main_effects(main_effects);
    if let Some(block) = block {
        spec = spec.with_block(block);
    }
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn lognormalrel(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut time: Option<String> = None;
    let mut censor: Option<String> = None;
    let mut censor_value = 0.0;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--time" | "--column" => {
                time = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--censor" | "--status" => {
                censor = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--censor-value" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--censor-value requires a number".to_owned())?;
                censor_value = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --censor-value: {raw}"))?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let time = time.ok_or_else(|| "missing --time <column>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::lognormal_reliability(time).with_censoring(censor, censor_value);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn exporeliability(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut time: Option<String> = None;
    let mut censor: Option<String> = None;
    let mut censor_value = 0.0;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--time" | "--column" => {
                time = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--censor" | "--status" => {
                censor = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--censor-value" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--censor-value requires a number".to_owned())?;
                censor_value = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --censor-value: {raw}"))?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let time = time.ok_or_else(|| "missing --time <column>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::exponential_reliability(time).with_censoring(censor, censor_value);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn weibullrel(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut time: Option<String> = None;
    let mut censor: Option<String> = None;
    let mut censor_value = 0.0;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--time" | "--column" => {
                time = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--censor" | "--status" => {
                censor = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--censor-value" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--censor-value requires a number".to_owned())?;
                censor_value = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --censor-value: {raw}"))?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let time = time.ok_or_else(|| "missing --time <column>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::weibull_reliability(time).with_censoring(censor, censor_value);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn gagetype1(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut measurement: Option<String> = None;
    let mut reference: Option<f64> = None;
    let mut lsl: Option<f64> = None;
    let mut usl: Option<f64> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--measurement" | "--column" => {
                measurement = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--reference" | "--ref" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--reference requires a number".to_owned())?;
                reference = Some(
                    raw.parse::<f64>()
                        .map_err(|_| format!("invalid --reference: {raw}"))?,
                );
            }
            "--lsl" | "--usl" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a number"))?;
                let value = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid {arg}: {raw}"))?;
                if arg == "--lsl" {
                    lsl = Some(value);
                } else {
                    usl = Some(value);
                }
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let measurement = measurement.ok_or_else(|| "missing --measurement <column>".to_owned())?;
    let reference = reference.ok_or_else(|| "missing --reference <value>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::gage_type1(measurement, reference).with_gage_limits(lsl, usl);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn agreement(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut part: Option<String> = None;
    let mut appraiser: Option<String> = None;
    let mut rating: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--part" => {
                part = Some(
                    it.next()
                        .ok_or_else(|| "--part requires a column name".to_owned())?
                        .clone(),
                );
            }
            "--appraiser" | "--operator" => {
                appraiser = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--rating" | "--response" => {
                rating = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let part = part.ok_or_else(|| "missing --part <column>".to_owned())?;
    let appraiser = appraiser.ok_or_else(|| "missing --appraiser <column>".to_owned())?;
    let rating = rating.ok_or_else(|| "missing --rating <column>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::agreement(part, appraiser, rating);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn gage_linearity(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut measurement: Option<String> = None;
    let mut reference: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--measurement" | "--column" => {
                measurement = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--reference" | "--ref" => {
                reference = Some(
                    it.next()
                        .ok_or_else(|| "--reference requires a column name".to_owned())?
                        .clone(),
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let measurement = measurement.ok_or_else(|| "missing --measurement <column>".to_owned())?;
    let reference = reference.ok_or_else(|| "missing --reference <column>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::gage_linearity(measurement, reference);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn gagerr(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut measurement: Option<String> = None;
    let mut part: Option<String> = None;
    let mut operator: Option<String> = None;
    let mut design = fp_core::gage::GageDesign::Crossed;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--measurement" | "--column" => {
                measurement = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--part" => {
                part = Some(
                    it.next()
                        .ok_or_else(|| "--part requires a column name".to_owned())?
                        .clone(),
                );
            }
            "--operator" => {
                operator = Some(
                    it.next()
                        .ok_or_else(|| "--operator requires a column name".to_owned())?
                        .clone(),
                );
            }
            "--type" | "--design" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--type requires crossed or nested".to_owned())?;
                design = fp_core::gage::GageDesign::parse(raw)
                    .ok_or_else(|| format!("invalid --type: {raw} (use crossed or nested)"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let measurement = measurement.ok_or_else(|| "missing --measurement <column>".to_owned())?;
    let part = part.ok_or_else(|| "missing --part <column>".to_owned())?;
    let operator = operator.ok_or_else(|| "missing --operator <column>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::gage_rr(measurement, part, operator).with_gage_design(design);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn reliability(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut time: Option<String> = None;
    let mut censor: Option<String> = None;
    let mut censor_value = 0.0;
    let mut confidence = 0.95;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--time" | "--column" => {
                time = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--censor" | "--status" => {
                censor = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--censor-value" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--censor-value requires a number".to_owned())?;
                censor_value = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --censor-value: {raw}"))?;
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = parse_confidence(raw)?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let time = time.ok_or_else(|| "missing --time <column>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::kaplan_meier(time)
        .with_censoring(censor, censor_value)
        .with_reliability_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn anova(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut factor: Option<String> = None;
    let mut confidence = 0.95;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--response" | "--y" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--factor" | "--group" => {
                factor = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = parse_confidence(raw)?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "missing --response <column>".to_owned())?;
    let factor = factor.ok_or_else(|| "missing --factor <column>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::anova(response, factor).with_anova_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn regress(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut predictors: Vec<String> = Vec::new();
    let mut confidence = 0.95;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--response" | "--y" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--predictors" | "--x" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        predictors.push(name.to_owned());
                    }
                }
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = parse_confidence(raw)?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "missing --response <column>".to_owned())?;
    if predictors.is_empty() {
        return Err("missing --predictors <a,b>".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec =
        AnalysisSpec::regression(response, predictors).with_regression_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn stepwise(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut predictors: Vec<String> = Vec::new();
    let mut method = fp_core::stepwise::StepMethod::Stepwise;
    let mut criterion = fp_core::stepwise::StepCriterion::PValue;
    let mut alpha_enter = 0.15;
    let mut alpha_remove = 0.15;
    let mut max_steps = 50usize;
    let mut confidence = 0.95;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--response" | "--y" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--predictors" | "--candidates" | "--x" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        predictors.push(name.to_owned());
                    }
                }
            }
            "--method" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--method requires forward, backward or stepwise".to_owned())?;
                method = fp_core::stepwise::StepMethod::parse(raw)
                    .ok_or_else(|| format!("unknown stepwise method: {raw}"))?;
            }
            "--criterion" | "--select" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--criterion requires p, aic or bic".to_owned())?;
                criterion = fp_core::stepwise::StepCriterion::parse(raw)
                    .ok_or_else(|| format!("unknown selection criterion: {raw}"))?;
            }
            "--alpha-enter" => {
                alpha_enter = parse_alpha(
                    it.next()
                        .ok_or_else(|| "--alpha-enter requires a probability".to_owned())?,
                )?;
            }
            "--alpha-remove" | "--alpha" => {
                alpha_remove = parse_alpha(
                    it.next()
                        .ok_or_else(|| "--alpha-remove requires a probability".to_owned())?,
                )?;
            }
            "--max-steps" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--max-steps requires a count".to_owned())?;
                max_steps = raw
                    .parse()
                    .map_err(|_| format!("invalid maximum number of steps: {raw}"))?;
                if max_steps == 0 {
                    return Err("--max-steps must be at least 1".to_owned());
                }
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = parse_confidence(raw)?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "missing --response <column>".to_owned())?;
    if predictors.is_empty() {
        return Err("missing --predictors <a,b>".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::stepwise_regression(response, predictors)
        .with_stepwise_method(method)
        .with_stepwise_criterion(criterion)
        .with_stepwise_alphas(alpha_enter, alpha_remove)
        .with_stepwise_max_steps(max_steps)
        .with_regression_confidence(confidence);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn parse_alpha(raw: &str) -> Result<f64, String> {
    let value: f64 = raw.parse().map_err(|_| format!("invalid alpha: {raw}"))?;
    if !(0.0..1.0).contains(&value) {
        return Err(format!("alpha must be between 0 and 1: {raw}"));
    }
    Ok(value)
}

fn logistic(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut predictors: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--response" | "--y" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--predictors" | "--x" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        predictors.push(name.to_owned());
                    }
                }
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "missing --response <column>".to_owned())?;
    if predictors.is_empty() {
        return Err("missing --predictors <a,b>".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::logistic(response, predictors);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

/// Options of a time-series command: the file, the column, the method options
/// and the optional SVG path.
struct TimeSeriesArgs {
    csv: PathBuf,
    column: String,
    options: Vec<(String, String)>,
    svg: Option<PathBuf>,
}

/// Reads `--column`, `--svg` and the extra options into a time-series call.
fn timeseries_options(args: &[String], allowed: &[&str]) -> Result<TimeSeriesArgs, String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut options: Vec<(String, String)> = Vec::new();
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" | "--c" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with("--") => {
                let name = flag.trim_start_matches("--");
                if !allowed.contains(&name) {
                    return Err(format!("unknown option: {flag}"));
                }
                let value = it
                    .next()
                    .ok_or_else(|| format!("{flag} requires a value"))?
                    .clone();
                options.push((name.to_owned(), value));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let column = column.ok_or_else(|| "missing --column <name>".to_owned())?;
    Ok(TimeSeriesArgs {
        csv,
        column,
        options,
        svg,
    })
}

fn option<'a>(options: &'a [(String, String)], name: &str) -> Option<&'a str> {
    options
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn option_usize(options: &[(String, String)], name: &str) -> Result<Option<usize>, String> {
    match option(options, name) {
        Some(raw) => raw
            .parse::<usize>()
            .map(Some)
            .map_err(|_| format!("invalid --{name}: {raw}")),
        None => Ok(None),
    }
}

/// Prints a time-series analysis and optionally writes its plot.
fn run_timeseries(spec: AnalysisSpec, csv: PathBuf, svg: Option<PathBuf>) -> Result<(), String> {
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn moving_average(args: &[String]) -> Result<(), String> {
    let parsed = timeseries_options(args, &["window"])?;
    let window = option_usize(&parsed.options, "window")?.unwrap_or(3);
    run_timeseries(
        AnalysisSpec::moving_average(parsed.column, window),
        parsed.csv,
        parsed.svg,
    )
}

fn decompose(args: &[String]) -> Result<(), String> {
    let parsed = timeseries_options(args, &["period", "model"])?;
    let period = option_usize(&parsed.options, "period")?.unwrap_or(12);
    let model = match option(&parsed.options, "model") {
        Some(raw) => fp_core::timeseries::DecompositionModel::parse(raw)
            .ok_or_else(|| format!("invalid --model: {raw}"))?,
        None => fp_core::timeseries::DecompositionModel::Additive,
    };
    run_timeseries(
        AnalysisSpec::decomposition(parsed.column, period, model),
        parsed.csv,
        parsed.svg,
    )
}

fn exponential_smoothing(args: &[String]) -> Result<(), String> {
    let parsed = timeseries_options(args, &["season", "horizon"])?;
    let season = option_usize(&parsed.options, "season")?.unwrap_or(1);
    let horizon = option_usize(&parsed.options, "horizon")?.unwrap_or(4);
    run_timeseries(
        AnalysisSpec::exponential_smoothing(parsed.column, season, horizon),
        parsed.csv,
        parsed.svg,
    )
}

/// Reads a numeric option with a default.
fn number_option(args: &[String], name: &str, default: f64) -> Result<f64, String> {
    match args.iter().position(|value| value == name) {
        Some(index) => args
            .get(index + 1)
            .ok_or_else(|| format!("{name} requires a value"))?
            .parse::<f64>()
            .map_err(|_| format!("invalid {name}")),
        None => Ok(default),
    }
}

/// Reads an integer option with a default.
fn integer_option(args: &[String], name: &str, default: usize) -> Result<usize, String> {
    match args.iter().position(|value| value == name) {
        Some(index) => args
            .get(index + 1)
            .ok_or_else(|| format!("{name} requires a value"))?
            .parse::<usize>()
            .map_err(|_| format!("invalid {name}")),
        None => Ok(default),
    }
}

fn svg_option(args: &[String]) -> Option<PathBuf> {
    args.iter()
        .position(|value| value == "--svg")
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
}

fn print_with_plot(
    out: &fp_core::workflow::AnalysisOutput,
    svg: Option<PathBuf>,
) -> Result<(), String> {
    print_analysis(out);
    if let (Some(path), Some(plot)) = (svg, out.plot.clone()) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn accelerated_life(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut time: Option<String> = None;
    let mut stress: Option<String> = None;
    let mut censor: Option<String> = None;
    let mut censor_value = 1.0;
    let mut use_stress = 300.0;
    let mut life = fp_core::alt::LifeModel::Weibull;
    let mut acceleration = fp_core::alt::AccelerationModel::Arrhenius;
    let mut mode: Option<String> = None;
    let mut horizon: Option<f64> = None;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--time" => {
                time = Some(
                    it.next()
                        .ok_or_else(|| "--time requires a column name".to_owned())?
                        .clone(),
                );
            }
            "--stress" | "--temperature" => {
                stress = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--censor" => {
                censor = Some(
                    it.next()
                        .ok_or_else(|| "--censor requires a column name".to_owned())?
                        .clone(),
                );
            }
            "--censor-value" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--censor-value requires a number".to_owned())?;
                censor_value = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --censor-value: {raw}"))?;
            }
            "--use-stress" | "--use" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--use-stress requires a number".to_owned())?;
                use_stress = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --use-stress: {raw}"))?;
            }
            "--distribution" | "--life" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--distribution requires a name".to_owned())?;
                life = fp_core::alt::LifeModel::parse(raw)
                    .ok_or_else(|| format!("invalid --distribution: {raw}"))?;
            }
            "--model" | "--acceleration" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--model requires a name".to_owned())?;
                acceleration = fp_core::alt::AccelerationModel::parse(raw)
                    .ok_or_else(|| format!("invalid --model: {raw}"))?;
            }
            "--mode" | "--failure-mode" => {
                mode = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--horizon" | "--at" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a number"))?;
                horizon = Some(
                    raw.parse::<f64>()
                        .map_err(|_| format!("invalid {arg}: {raw}"))?,
                );
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let time = time.ok_or_else(|| "missing --time <column>".to_owned())?;
    let stress = stress.ok_or_else(|| "missing --stress <column>".to_owned())?;
    if horizon.is_some() && mode.is_none() {
        return Err("--horizon needs --mode <column> (competing risks)".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::accelerated_life(time, stress)
        .with_accelerated_life_options(censor, censor_value, use_stress)
        .with_accelerated_life_model(life, acceleration)
        .with_competing_mode(mode)
        .with_competing_horizon(horizon);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn step_stress(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut duration: Option<String> = None;
    let mut stress: Option<String> = None;
    let mut unit: Option<String> = None;
    let mut event: Option<String> = None;
    let mut use_stress = 300.0;
    let mut life = fp_core::alt::LifeModel::Weibull;
    let mut acceleration = fp_core::alt::AccelerationModel::Arrhenius;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--duration" | "--time" => {
                duration = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--stress" | "--temperature" => {
                stress = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--unit" | "--item" => {
                unit = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--event" | "--failed" => {
                event = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--use-stress" | "--use" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--use-stress requires a number".to_owned())?;
                use_stress = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --use-stress: {raw}"))?;
            }
            "--distribution" | "--life" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--distribution requires a name".to_owned())?;
                life = fp_core::alt::LifeModel::parse(raw)
                    .ok_or_else(|| format!("invalid --distribution: {raw}"))?;
            }
            "--model" | "--acceleration" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--model requires a name".to_owned())?;
                acceleration = fp_core::alt::AccelerationModel::parse(raw)
                    .ok_or_else(|| format!("invalid --model: {raw}"))?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let duration = duration.ok_or_else(|| "missing --duration <column>".to_owned())?;
    let stress = stress.ok_or_else(|| "missing --stress <column>".to_owned())?;
    let unit = unit.ok_or_else(|| "missing --unit <column>".to_owned())?;
    let event = event.ok_or_else(|| "missing --event <column>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::step_stress_alt(
        duration,
        stress,
        unit,
        event,
        use_stress,
        life,
        acceleration,
    );
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

/// One `--response` of the desirability command.
///
/// Format: `column:goal:low:high[:target=<t>][:weight=<w>][:importance=<i>]`.
fn parse_desirability_response(text: &str) -> Result<fp_core::desirability::Response, String> {
    let parts: Vec<&str> = text.split(':').map(str::trim).collect();
    if parts.len() < 4 {
        return Err(format!(
            "invalid --response: {text} (use column:goal:low:high[:target=<t>][:weight=<w>][:importance=<i>])"
        ));
    }
    let column = parts[0];
    if column.is_empty() {
        return Err(format!("invalid --response: {text} (empty column name)"));
    }
    let goal = fp_core::desirability::Goal::parse(parts[1]).ok_or_else(|| {
        format!(
            "invalid goal in --response: {} (use maximize, minimize or target)",
            parts[1]
        )
    })?;
    let number = |what: &str, raw: &str| {
        raw.parse::<f64>()
            .map_err(|_| format!("invalid {what} in --response: {raw}"))
    };
    let low = number("low limit", parts[2])?;
    let high = number("high limit", parts[3])?;
    let mut target = 0.5 * (low + high);
    let mut weight = 1.0;
    let mut importance = 1.0;
    for extra in &parts[4..] {
        let (key, value) = extra.split_once('=').ok_or_else(|| {
            format!("invalid --response field: {extra} (use key=value, e.g. target=80)")
        })?;
        match key.trim().to_ascii_lowercase().as_str() {
            "target" => target = number("target", value.trim())?,
            "weight" => weight = number("weight", value.trim())?,
            "importance" => importance = number("importance", value.trim())?,
            other => {
                return Err(format!(
                    "unknown --response field: {other} (use target, weight or importance)"
                ));
            }
        }
    }
    if goal == fp_core::desirability::Goal::Target && target <= low {
        return Err(format!(
            "--response {column}: a target goal needs target=<t> inside (low, high)"
        ));
    }
    Ok(fp_core::desirability::Response {
        column: column.to_owned(),
        goal,
        low,
        target,
        high,
        weight,
        importance,
    })
}

fn desirability(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut factors: Vec<String> = Vec::new();
    let mut responses: Vec<fp_core::desirability::Response> = Vec::new();
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--factors" | "--factor" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        factors.push(name.to_owned());
                    }
                }
            }
            "--response" | "--goal" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires column:goal:low:high"))?;
                responses.push(parse_desirability_response(raw)?);
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    if factors.len() < 2 {
        return Err("missing --factors <A,B> (at least two factors)".to_owned());
    }
    if responses.is_empty() {
        return Err("missing --response <column:goal:low:high>".to_owned());
    }
    for response in &responses {
        response.validate().map_err(|e| e.to_string())?;
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::desirability(factors, responses);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn calc(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut name: Option<String> = None;
    let mut expression: Option<String> = None;
    let mut output: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--name" | "--column" => {
                name = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--expression" | "--formula" => {
                expression = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires an expression"))?
                        .clone(),
                );
            }
            "--output" => {
                output = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--output requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let name = name.ok_or_else(|| "missing --name <new column>".to_owned())?;
    let expression = expression.ok_or_else(|| "missing --expression <formula>".to_owned())?;
    let (mut df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    fp_core::calc::add_column(&mut df, &name, &expression).map_err(|e| e.to_string())?;
    match output {
        Some(path) => {
            let text = io::write_csv_string(&df, false).map_err(|e| e.to_string())?;
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            println!(
                "column {name} added: {} rows x {} columns - {}",
                df.n_rows(),
                df.n_columns(),
                path.display()
            );
        }
        None => {
            println!(
                "column {name} added: {} rows x {} columns",
                df.n_rows(),
                df.n_columns()
            );
            let values = df
                .column(&name)
                .map_err(|e| e.to_string())?
                .numeric(fp_core::data::MissingPolicy::Drop)
                .map_err(|e| e.to_string())?;
            for value in values.iter().take(5) {
                println!("  {value:.6}");
            }
        }
    }
    Ok(())
}

fn date_difference(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut first: Option<String> = None;
    let mut second: Option<String> = None;
    let mut name = "difference".to_owned();
    let mut output: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--first" | "--from" => {
                first = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--second" | "--to" => {
                second = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--name" => {
                name = it
                    .next()
                    .ok_or_else(|| "--name requires a column name".to_owned())?
                    .clone();
            }
            "--output" => {
                output = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--output requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let first = first.ok_or_else(|| "missing --first <date column>".to_owned())?;
    let second = second.ok_or_else(|| "missing --second <date column>".to_owned())?;
    let (mut df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    fp_core::dates::add_difference_column(&mut df, &first, &second, &name)
        .map_err(|e| e.to_string())?;
    let values: Vec<f64> = df
        .column(&name)
        .map_err(|e| e.to_string())?
        .numeric(fp_core::data::MissingPolicy::Drop)
        .map_err(|e| e.to_string())?
        .to_vec();
    match output {
        Some(path) => {
            let text = io::write_csv_string(&df, false).map_err(|e| e.to_string())?;
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            println!(
                "column {name} added: {} rows x {} columns - {}",
                df.n_rows(),
                df.n_columns(),
                path.display()
            );
        }
        None => {
            println!(
                "column {name} added: {} rows x {} columns",
                df.n_rows(),
                df.n_columns()
            );
        }
    }
    if !values.is_empty() {
        let count = values.len();
        let mean = values.iter().sum::<f64>() / count as f64;
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        println!(
            "days between {first} and {second}: n={count} mean={mean:.4} min={min:.0} max={max:.0}"
        );
    }
    Ok(())
}

fn date_add_days(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut days: Option<i64> = None;
    let mut name: Option<String> = None;
    let mut output: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" | "--date" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--days" | "--shift" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a whole number of days"))?;
                days = Some(
                    raw.parse::<i64>()
                        .map_err(|_| format!("invalid {arg}: {raw}"))?,
                );
            }
            "--name" => {
                name = Some(
                    it.next()
                        .ok_or_else(|| "--name requires a column name".to_owned())?
                        .clone(),
                );
            }
            "--output" => {
                output = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--output requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let column = column.ok_or_else(|| "missing --column <date column>".to_owned())?;
    let days = days.ok_or_else(|| "missing --days <n>".to_owned())?;
    let name = name.unwrap_or_else(|| format!("{column}_shifted"));
    let (mut df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    fp_core::dates::add_shift_column(&mut df, &column, days, &name).map_err(|e| e.to_string())?;
    match output {
        Some(path) => {
            let text = io::write_csv_string(&df, false).map_err(|e| e.to_string())?;
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            println!(
                "column {name} added: {} rows x {} columns - {}",
                df.n_rows(),
                df.n_columns(),
                path.display()
            );
        }
        None => {
            println!(
                "column {name} added: {} rows x {} columns",
                df.n_rows(),
                df.n_columns()
            );
        }
    }
    Ok(())
}

fn sign_test(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut median: Option<f64> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" | "--c" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--median" | "--mu" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--median requires a number".to_owned())?;
                median = Some(
                    raw.parse::<f64>()
                        .map_err(|_| format!("invalid --median: {raw}"))?,
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let column = column.ok_or_else(|| "missing --column <name>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    // The documented behaviour tests the median against 0 by default.
    let median = median.unwrap_or(0.0);
    let spec = AnalysisSpec::sign_test(column, median);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn series_plot(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut time: Option<String> = None;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" | "--c" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--time" | "--x" => {
                time = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::series_plot(column).with_series_time(time);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn stem_leaf(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" | "--c" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::stem_leaf(column);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn ecdf(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut confidence = 95.0;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" | "--c" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --confidence: {raw}"))?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec =
        AnalysisSpec::ecdf(column).with_ecdf_confidence((confidence / 100.0).clamp(0.5, 0.999));
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn dotplot(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut by: Option<String> = None;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" | "--c" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--by" | "--factor" => {
                by = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::dotplot(column).with_dotplot_factor(by);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn matrix_plot(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut columns: Vec<String> = Vec::new();
    let mut bins = 8usize;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--columns" | "--variables" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        columns.push(name.to_owned());
                    }
                }
            }
            "--bins" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--bins requires an integer".to_owned())?;
                bins = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --bins: {raw}"))?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    if columns.len() < 2 {
        return Err("missing --columns <a,b,...> (at least two)".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::matrix_plot(columns).with_matrix_bins(bins.clamp(3, 30));
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn interval_plot(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut factor: Option<String> = None;
    let mut confidence = 95.0;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--response" | "--y" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--factor" | "--by" => {
                factor = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --confidence: {raw}"))?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "missing --response <column>".to_owned())?;
    let factor = factor.ok_or_else(|| "missing --factor <column>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::interval_plot(response, factor)
        .with_interval_confidence((confidence / 100.0).clamp(0.5, 0.999));
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn pareto(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" | "--c" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let column = column.ok_or_else(|| "missing --column <name>".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::pareto(column);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn taguchi_design(args: &[String]) -> Result<(), String> {
    let mut output: Option<PathBuf> = None;
    let mut runs = 8usize;
    let mut factors: Option<usize> = None;
    let mut randomize = false;
    let mut seed = MC_SEED;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--runs" | "--array" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--runs requires an integer".to_owned())?;
                runs = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --runs: {raw}"))?;
            }
            "--factors" | "--columns" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--factors requires an integer".to_owned())?;
                factors = Some(
                    raw.parse::<usize>()
                        .map_err(|_| format!("invalid --factors: {raw}"))?,
                );
            }
            "--randomize" => randomize = true,
            "--seed" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--seed requires an integer".to_owned())?;
                seed = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --seed: {raw}"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if output.is_some() {
                    return Err("only one output file is accepted".to_owned());
                }
                output = Some(PathBuf::from(path));
            }
        }
    }
    let output = output.ok_or_else(|| "missing output CSV file".to_owned())?;
    let available = fp_core::taguchi::array(runs).map_err(|e| e.to_string())?;
    let factors = factors.unwrap_or(available.factors());
    let df = fp_core::taguchi::design(runs, factors, randomize, seed).map_err(|e| e.to_string())?;
    let csv = io::write_csv_string(&df, false).map_err(|e| e.to_string())?;
    std::fs::write(&output, csv).map_err(|e| format!("{}: {e}", output.display()))?;
    println!(
        "Taguchi design written: {} runs x {} columns - {}",
        df.n_rows(),
        df.n_columns(),
        output.display()
    );
    Ok(())
}

fn taguchi(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut factors: Vec<String> = Vec::new();
    let mut quality = fp_core::taguchi::Quality::Smaller;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--response" | "--y" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--factors" | "--x" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        factors.push(name.to_owned());
                    }
                }
            }
            "--quality" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--quality requires smaller, larger or nominal".to_owned())?;
                quality = fp_core::taguchi::Quality::parse(raw)
                    .ok_or_else(|| format!("invalid --quality: {raw}"))?;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "missing --response <column>".to_owned())?;
    if factors.is_empty() {
        return Err("missing --factors <A,B>".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::taguchi(response, factors, quality);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn acceptance_curve(args: &[String]) -> Result<(), String> {
    let sample = integer_option(args, "--sample", 20)?;
    let acceptance = integer_option(args, "--accept", 1)?;
    let lot = match args.iter().position(|value| value == "--lot") {
        Some(index) => Some(
            args.get(index + 1)
                .ok_or_else(|| "--lot requires a value".to_owned())?
                .parse::<usize>()
                .map_err(|_| "invalid --lot".to_owned())?,
        ),
        None => None,
    };
    let mut out = fp_core::sampling::analyze(lot, sample, acceptance).map_err(|e| e.to_string())?;
    let lang = language();
    for result in &mut out.results {
        result.set_lang(lang);
    }
    print_with_plot(&out, svg_option(args))
}

fn sampling_plan(args: &[String]) -> Result<(), String> {
    let aql = number_option(args, "--aql", 1.0)? / 100.0;
    let ltpd = number_option(args, "--ltpd", 5.0)? / 100.0;
    let alpha = number_option(args, "--alpha", 5.0)? / 100.0;
    let beta = number_option(args, "--beta", 10.0)? / 100.0;
    let lot = match option_string(args, "--lot") {
        Some(raw) => Some(
            raw.parse::<usize>()
                .map_err(|_| format!("invalid --lot: {raw}"))?,
        ),
        None => None,
    };
    let mut out =
        fp_core::sampling::analyze_plan(aql, ltpd, alpha, beta, lot).map_err(|e| e.to_string())?;
    let lang = language();
    for result in &mut out.results {
        result.set_lang(lang);
    }
    print_with_plot(&out, svg_option(args))
}

fn t2_chart(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut columns: Vec<String> = Vec::new();
    let mut subgroup = 3usize;
    let mut confidence = 95.0;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--columns" | "--variables" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        columns.push(name.to_owned());
                    }
                }
            }
            "--subgroup" | "--size" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires an integer"))?;
                subgroup = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid {arg}: {raw}"))?;
            }
            "--confidence" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--confidence requires a percentage".to_owned())?;
                confidence = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --confidence: {raw}"))?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    if columns.len() < 2 {
        return Err("missing --columns <a,b> (at least two)".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::hotelling_t2(columns, subgroup)
        .with_t2_confidence((confidence / 100.0).clamp(0.5, 0.999));
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn pca(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut columns: Vec<String> = Vec::new();
    let mut keep = 2usize;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--columns" | "--variables" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        columns.push(name.to_owned());
                    }
                }
            }
            "--keep" | "--components" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires an integer"))?;
                keep = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid {arg}: {raw}"))?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    if columns.len() < 2 {
        return Err("missing --columns <a,b,...> (at least two)".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::pca(columns, keep);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn kmeans(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut columns: Vec<String> = Vec::new();
    let mut clusters = 2usize;
    let mut standardise = true;
    let mut seed = MC_SEED;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--columns" | "--variables" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        columns.push(name.to_owned());
                    }
                }
            }
            "--clusters" | "--k" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires an integer"))?;
                clusters = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid {arg}: {raw}"))?;
            }
            "--raw" | "--no-standardise" => standardise = false,
            "--seed" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--seed requires an integer".to_owned())?;
                seed = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --seed: {raw}"))?;
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    if columns.len() < 2 {
        return Err("missing --columns <a,b,...> (at least two)".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::kmeans(columns, clusters).with_kmeans_options(standardise, seed);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn ordinal(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut predictors: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--response" | "--y" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--predictors" | "--x" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        predictors.push(name.to_owned());
                    }
                }
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "missing --response <column>".to_owned())?;
    if predictors.is_empty() {
        return Err("missing --predictors <a,b>".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::ordinal(response, predictors);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn poisson(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut response: Option<String> = None;
    let mut predictors: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--response" | "--y" => {
                response = Some(
                    it.next()
                        .ok_or_else(|| format!("{arg} requires a column name"))?
                        .clone(),
                );
            }
            "--predictors" | "--x" => {
                let raw = it
                    .next()
                    .ok_or_else(|| format!("{arg} requires a comma-separated list"))?;
                for name in raw.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        predictors.push(name.to_owned());
                    }
                }
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let response = response.ok_or_else(|| "missing --response <column>".to_owned())?;
    if predictors.is_empty() {
        return Err("missing --predictors <a,b>".to_owned());
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let spec = AnalysisSpec::poisson(response, predictors);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    Ok(())
}

fn attrchart(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut column: Option<String> = None;
    let mut kind: Option<AttributeChartKind> = None;
    let mut sizes: Option<String> = None;
    let mut size: Option<usize> = None;
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--column" => {
                column = Some(
                    it.next()
                        .ok_or_else(|| "--column requires a name".to_owned())?
                        .clone(),
                );
            }
            "--chart" => {
                let raw = it.next().ok_or_else(|| {
                    "--chart requires p, np, c or u (add a prime for the Laney charts)".to_owned()
                })?;
                kind = Some(AttributeChartKind::parse(raw).ok_or_else(|| {
                    format!(
                        "invalid --chart: {raw} (use p, np, c or u; add a prime for the Laney \
                                 charts)"
                    )
                })?);
            }
            "--sizes" => {
                sizes = Some(
                    it.next()
                        .ok_or_else(|| "--sizes requires a column name".to_owned())?
                        .clone(),
                );
            }
            "--size" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--size requires a positive integer".to_owned())?;
                size = Some(
                    raw.parse::<usize>()
                        .map_err(|_| format!("invalid --size: {raw}"))?,
                );
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if csv.is_some() {
                    return Err("only one CSV file is accepted".to_owned());
                }
                csv = Some(PathBuf::from(path));
            }
        }
    }
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let kind = kind.ok_or_else(|| "missing --chart (p, np, c or u)".to_owned())?;
    if kind.needs_size() && sizes.is_none() && size.is_none() {
        return Err(format!(
            "{} chart requires --sizes <column> or --size <n>",
            kind.name()
        ));
    }
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let column = match column {
        Some(name) => name,
        None => fp_core::workflow::first_numeric_column(&df)
            .ok_or_else(|| "no numeric column: specify one with --column".to_owned())?
            .to_owned(),
    };
    let spec = AnalysisSpec::attribute_chart(kind, column).with_attribute_size(sizes, size);
    let out = run_local(&df, &spec).map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

fn doctor() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = exe.parent().unwrap_or_else(|| std::path::Path::new("."));
    let data = platform::resolve_data_dir(exe_dir).map_err(|e| e.to_string())?;

    println!("FreePlots {}", fp_core::VERSION);
    println!("Executable      {}", exe.display());
    println!(
        "Data mode       {}",
        if data.portable {
            "portable"
        } else {
            "fallback"
        }
    );
    println!("Data directory  {}", data.path.display());
    println!("Project schema  {}", fp_core::PROJECT_SCHEMA_VERSION);
    Ok(())
}

/// Lists the registered plugins.
fn plugins(_args: &[String]) -> Result<(), String> {
    let registry = fp_core::plugin::registry()
        .lock()
        .map_err(|_| "the plugin registry is poisoned".to_owned())?;
    if registry.plugin_ids().is_empty() {
        println!("no plugins are registered");
        return Ok(());
    }
    println!("registered plugins");
    println!("-----------------");
    for (id, name, description) in registry.list() {
        println!("{id:<16} {name} — {description}");
    }
    Ok(())
}

/// Runs a registered plugin on a worksheet.
fn plugin(args: &[String]) -> Result<(), String> {
    let mut csv: Option<PathBuf> = None;
    let mut id: Option<String> = None;
    let mut options: Vec<(String, String)> = Vec::new();
    let mut svg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--id" => {
                id = Some(
                    it.next()
                        .ok_or_else(|| "--id requires a plugin identifier".to_owned())?
                        .clone(),
                );
            }
            "--option" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--option requires a name=value pair".to_owned())?;
                let (name, value) = raw
                    .split_once('=')
                    .ok_or_else(|| format!("invalid --option: {raw} (use name=value)"))?;
                options.push((name.trim().to_owned(), value.trim().to_owned()));
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--svg requires a path".to_owned())?,
                ));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            value => {
                if id.is_none() && csv.is_none() {
                    id = Some(value.to_owned());
                } else if csv.is_none() {
                    csv = Some(PathBuf::from(value));
                } else {
                    return Err("only one CSV file is accepted".to_owned());
                }
            }
        }
    }
    let id = id.ok_or_else(|| "missing the plugin identifier".to_owned())?;
    let csv = csv.ok_or_else(|| "missing CSV file".to_owned())?;
    let (df, _encoding) = io::read_csv_file(&csv).map_err(|e| e.to_string())?;
    let registry = fp_core::plugin::registry()
        .lock()
        .map_err(|_| "the plugin registry is poisoned".to_owned())?;
    let out = registry
        .run(&id, &df, &fp_core::plugin::PluginOptions::new(options))
        .map_err(|e| e.to_string())?;
    print_analysis(&out);
    if let (Some(path), Some(plot)) = (svg, out.plot) {
        std::fs::write(&path, render_svg(&plot)).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("graph written: {}", path.display());
    }
    Ok(())
}

/// Imports an Excel workbook and prints (or writes) the worksheet.
fn xlsx_import(args: &[String]) -> Result<(), String> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut sheet: Option<String> = None;
    let mut list = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--output" => {
                output = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| "--output requires a path".to_owned())?,
                ));
            }
            "--sheet" => {
                sheet = Some(
                    it.next()
                        .ok_or_else(|| "--sheet requires a name".to_owned())?
                        .clone(),
                );
            }
            "--list-sheets" | "--sheets" => list = true,
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}"));
            }
            path => {
                if input.is_some() {
                    return Err("only one workbook is accepted".to_owned());
                }
                input = Some(PathBuf::from(path));
            }
        }
    }
    let input = input.ok_or_else(|| "missing the workbook file".to_owned())?;
    let bytes = std::fs::read(&input).map_err(|e| format!("{}: {e}", input.display()))?;
    let names = fp_core::xlsx_read::sheet_names(&bytes).map_err(|e| e.to_string())?;
    if list {
        for name in &names {
            println!("{name}");
        }
        return Ok(());
    }
    if let Some(name) = &sheet
        && !names.iter().any(|existing| existing == name)
    {
        return Err(format!(
            "the workbook has no sheet named {name:?}; available: {}",
            names.join(", ")
        ));
    }
    let df = fp_core::xlsx_read::read_workbook_sheet(&bytes, sheet.as_deref())
        .map_err(|e| e.to_string())?;
    println!(
        "workbook read: {} rows × {} columns (sheet {})",
        df.n_rows(),
        df.n_columns(),
        sheet
            .as_deref()
            .unwrap_or_else(|| names.first().map_or("?", String::as_str))
    );
    for column in df.columns() {
        println!("  {} ({})", column.name(), column.dtype().as_str());
    }
    if let Some(path) = output {
        let csv = io::write_csv_string(&df, false).map_err(|e| e.to_string())?;
        std::fs::write(&path, csv).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("worksheet written: {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_shows_help() {
        assert!(run(&[]).is_ok());
    }

    #[test]
    fn control_chart_options_validate_the_test_selection() {
        let error = run(&[
            "controlchart".to_owned(),
            "x.csv".to_owned(),
            "--chart".to_owned(),
            "imr".to_owned(),
            "--tests".to_owned(),
            "9".to_owned(),
        ])
        .unwrap_err();
        assert!(error.contains("outside the supported range 1–8"), "{error}");

        let error = run(&[
            "controlchart".to_owned(),
            "x.csv".to_owned(),
            "--chart".to_owned(),
            "imr".to_owned(),
            "--sensitivity".to_owned(),
            "2=6".to_owned(),
        ])
        .unwrap_err();
        assert!(error.contains("documented range 7–11"), "{error}");

        let error = run(&[
            "controlchart".to_owned(),
            "x.csv".to_owned(),
            "--chart".to_owned(),
            "imr".to_owned(),
            "--sensitivity".to_owned(),
            "1=2".to_owned(),
        ])
        .unwrap_err();
        assert!(error.contains("fixed"), "{error}");

        // A valid list parses and only the missing file stops the command.
        let error = run(&[
            "controlchart".to_owned(),
            "x.csv".to_owned(),
            "--chart".to_owned(),
            "imr".to_owned(),
            "--tests".to_owned(),
            "1:8".to_owned(),
            "--sensitivity".to_owned(),
            "2=8,5=3".to_owned(),
        ])
        .unwrap_err();
        assert!(!error.contains("--tests"), "{error}");
        assert!(!error.contains("--sensitivity"), "{error}");
    }

    #[test]
    fn the_new_commands_parse_their_arguments() {
        let args = vec![
            "equiv1".to_owned(),
            "x.csv".to_owned(),
            "--column".to_owned(),
            "x".to_owned(),
            "--margin".to_owned(),
            "0.5".to_owned(),
            "--mean".to_owned(),
            "10".to_owned(),
        ];
        let error = run(&args).unwrap_err();
        assert!(error.contains("CSV"), "{error}");

        let args = vec![
            "x.csv".to_owned(),
            "--columns".to_owned(),
            "a,b".to_owned(),
            "--margin".to_owned(),
            "0.5".to_owned(),
        ];
        assert!(parse_equivalence_columns(&args).is_ok());

        let args = vec![
            "x.csv".to_owned(),
            "--response".to_owned(),
            "y".to_owned(),
            "--factor".to_owned(),
            "g".to_owned(),
            "--center".to_owned(),
            "median".to_owned(),
        ];
        let (csv, response, factor, center) = parse_variance_test_args(&args).expect("arguments");
        assert_eq!(response, "y");
        assert_eq!(factor, "g");
        assert_eq!(center, VarianceCenter::Median);
        assert!(csv.ends_with("x.csv"));

        let args = vec![
            "x.csv".to_owned(),
            "--expected".to_owned(),
            "25,25,50".to_owned(),
            "--parameters".to_owned(),
            "1".to_owned(),
        ];
        let (_, _, expected, parameters) = parse_goodness_of_fit_args(&args).expect("arguments");
        assert_eq!(expected, vec![25.0, 25.0, 50.0]);
        assert_eq!(parameters, 1);

        let args = vec!["x.csv".to_owned(), "--alpha".to_owned(), "1".to_owned()];
        let (_, _, alpha) = parse_grubbs_args(&args).expect("arguments");
        assert!((alpha - 0.01).abs() < 1e-12, "alpha {alpha}");

        let args = vec![
            "--test".to_owned(),
            "t1".to_owned(),
            "--effect".to_owned(),
            "0.5".to_owned(),
            "--n".to_owned(),
            "40".to_owned(),
        ];
        let (procedure, effect, _, n, _, _, _, _) = parse_power_args(&args).expect("arguments");
        assert_eq!(procedure, PowerProcedure::OneSampleT);
        assert!((effect - 0.5).abs() < 1e-12);
        assert_eq!(n, 40);
    }

    #[test]
    fn the_new_commands_reject_bad_arguments() {
        let args = vec!["x.csv".to_owned(), "--expected".to_owned(), "25".to_owned()];
        assert!(parse_goodness_of_fit_args(&args).is_err());

        let args = vec![
            "--test".to_owned(),
            "unknown".to_owned(),
            "--effect".to_owned(),
            "0.5".to_owned(),
        ];
        assert!(parse_power_args(&args).is_err());

        let args = vec!["--effect".to_owned(), "0.5".to_owned()];
        assert!(parse_power_args(&args).is_err());

        let args = vec![
            "x.csv".to_owned(),
            "--response".to_owned(),
            "y".to_owned(),
            "--factor".to_owned(),
            "g".to_owned(),
            "--center".to_owned(),
            "mode".to_owned(),
        ];
        assert!(parse_variance_test_args(&args).is_err());
    }

    #[test]
    fn the_desirability_response_parses_its_fields() {
        let response = parse_desirability_response("Strength:maximize:10:30").expect("maximize");
        assert_eq!(response.column, "Strength");
        assert_eq!(response.goal, fp_core::desirability::Goal::Maximize);
        assert!((response.low - 10.0).abs() < 1e-12);
        assert!((response.high - 30.0).abs() < 1e-12);
        assert!((response.target - 20.0).abs() < 1e-12);
        assert!((response.weight - 1.0).abs() < 1e-12);
        assert!((response.importance - 1.0).abs() < 1e-12);

        let response =
            parse_desirability_response("Yield:target:60:100:target=80:weight=2:importance=3")
                .expect("target");
        assert_eq!(response.goal, fp_core::desirability::Goal::Target);
        assert!((response.target - 80.0).abs() < 1e-12);
        assert!((response.weight - 2.0).abs() < 1e-12);
        assert!((response.importance - 3.0).abs() < 1e-12);

        // Boundary: the limits themselves and a weight of one.
        let response = parse_desirability_response("Cost:min:0:1:weight=1").expect("boundary");
        assert_eq!(response.goal, fp_core::desirability::Goal::Minimize);
        assert!((response.high - 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_desirability_response_rejects_bad_fields() {
        for text in [
            "Strength:maximize:10",
            ":maximize:10:30",
            "Strength:sideways:10:30",
            "Strength:maximize:ten:30",
            "Strength:maximize:10:30:colour=red",
            "Strength:maximize:10:30:weight",
            "Yield:target:60:100:target=50",
        ] {
            assert!(
                parse_desirability_response(text).is_err(),
                "accepted {text}"
            );
        }
    }

    #[test]
    fn the_new_analysis_commands_validate_their_arguments() {
        let error = run(&["stepstress".to_owned(), "x.csv".to_owned()]).unwrap_err();
        assert!(error.contains("--duration"), "{error}");

        let error = run(&[
            "alt".to_owned(),
            "x.csv".to_owned(),
            "--time".to_owned(),
            "t".to_owned(),
            "--stress".to_owned(),
            "s".to_owned(),
            "--horizon".to_owned(),
            "100".to_owned(),
        ])
        .unwrap_err();
        assert!(error.contains("--mode"), "{error}");

        let error = run(&[
            "desirability".to_owned(),
            "x.csv".to_owned(),
            "--factors".to_owned(),
            "A".to_owned(),
            "--response".to_owned(),
            "y:maximize:0:1".to_owned(),
        ])
        .unwrap_err();
        assert!(error.contains("at least two factors"), "{error}");

        let error = run(&[
            "desirability".to_owned(),
            "x.csv".to_owned(),
            "--factors".to_owned(),
            "A,B".to_owned(),
        ])
        .unwrap_err();
        assert!(error.contains("--response"), "{error}");

        let error = run(&[
            "controlchart".to_owned(),
            "x.csv".to_owned(),
            "--chart".to_owned(),
            "imr".to_owned(),
            "--subgroup-column".to_owned(),
            "lot".to_owned(),
        ])
        .unwrap_err();
        assert!(error.contains("xbar-r"), "{error}");

        let error = run(&["datediff".to_owned(), "x.csv".to_owned()]).unwrap_err();
        assert!(error.contains("--first"), "{error}");

        let error = run(&["dateadd".to_owned(), "x.csv".to_owned()]).unwrap_err();
        assert!(error.contains("--column"), "{error}");

        let error = run(&[
            "dateadd".to_owned(),
            "x.csv".to_owned(),
            "--column".to_owned(),
            "start".to_owned(),
            "--days".to_owned(),
            "1.5".to_owned(),
        ])
        .unwrap_err();
        assert!(error.contains("--days"), "{error}");
    }

    #[test]
    fn unknown_command_is_an_error() {
        let e = run(&["make-coffee".to_owned()]).unwrap_err();
        assert!(e.contains("unknown"));
    }

    #[test]
    fn the_language_option_is_removed_from_the_arguments() {
        let args: Vec<String> = ["describe", "--lang", "pt", "data.csv"]
            .iter()
            .map(|value| (*value).to_owned())
            .collect();
        let (rest, lang) = split_language(&args).expect("language");
        assert_eq!(rest, vec!["describe".to_owned(), "data.csv".to_owned()]);
        assert_eq!(lang, fp_core::i18n::Lang::Portuguese);
        let args: Vec<String> = ["describe".to_owned(), "--lang=fr".to_owned()].to_vec();
        assert!(split_language(&args).is_err());
    }

    #[test]
    fn analyze_without_file_is_an_error() {
        let e = run(&["analyze".to_owned()]).unwrap_err();
        assert!(e.contains("CSV"));
    }

    #[test]
    fn invalid_reps_is_an_error() {
        let args = vec![
            "analyze".to_owned(),
            "x.csv".to_owned(),
            "--reps".to_owned(),
            "many".to_owned(),
        ];
        assert!(run(&args).unwrap_err().contains("--reps"));
    }

    #[test]
    fn invalid_p_value_is_an_error() {
        let args = vec![
            "analyze".to_owned(),
            "x.csv".to_owned(),
            "--p-value".to_owned(),
            "bootstrap".to_owned(),
        ];
        assert!(run(&args).unwrap_err().contains("--p-value"));
    }

    #[test]
    fn invalid_distribution_is_an_error() {
        let args = vec![
            "analyze".to_owned(),
            "x.csv".to_owned(),
            "--distribution".to_owned(),
            "cauchy".to_owned(),
        ];
        assert!(run(&args).unwrap_err().contains("--distribution"));
    }

    #[test]
    fn unknown_option_is_an_error() {
        let args = vec![
            "analyze".to_owned(),
            "x.csv".to_owned(),
            "--turbo".to_owned(),
        ];
        assert!(run(&args).unwrap_err().contains("--turbo"));
    }
}
