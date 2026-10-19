//! The command line: `hushed run ...` and `hushed analyze ...`.

use crate::{paths, report, source, tracer};
use clap::{Args, Parser, Subcommand};
use pyo3::prelude::*;
use std::path::Path;

const SKIP_DIRS: [&str; 7] = [
    "__pycache__",
    "venv",
    "env",
    "node_modules",
    "build",
    "dist",
    "site-packages",
];

#[derive(Parser)]
#[command(
    name = "hushed",
    version,
    about = "Run a Python script unchanged and see what actually happened, including the errors it swallowed."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a script (or module) unchanged, then report swallowed errors, time per function and
    /// functions that never ran. The report goes to stderr; the script's own output is untouched.
    Run(RunArgs),
    /// Read Python files without running them: functions, complexity, and the except blocks that
    /// can hide errors.
    Analyze(AnalyzeArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Run a module, like `python -m`
    #[arg(short = 'm')]
    module: bool,
    /// Also list errors that were caught on purpose
    #[arg(long)]
    all: bool,
    /// Skip timing, so the script runs at full speed
    #[arg(long)]
    exceptions_only: bool,
    /// Exit with code 3 when errors were swallowed but the script itself succeeded
    #[arg(long)]
    fail_on_swallowed: bool,
    /// Also watch code under DIR (default: the script's folder)
    #[arg(long, value_name = "DIR")]
    include: Vec<String>,
    /// Also write the report as JSON to FILE
    #[arg(long, value_name = "FILE")]
    json: Option<String>,
    /// How many of the slowest functions to show
    #[arg(long, value_name = "N", default_value_t = 10)]
    top: usize,
    /// The script (or module, with -m), then the arguments passed on to it
    #[arg(required = true, num_args = 1.., trailing_var_arg = true, allow_hyphen_values = true, value_name = "SCRIPT [ARGS]")]
    command: Vec<String>,
}

#[derive(Args)]
struct AnalyzeArgs {
    /// Python files or folders
    #[arg(required = true, value_name = "PATH")]
    paths: Vec<String>,
    /// Also list print() calls and assigned variables
    #[arg(short, long)]
    verbose: bool,
    /// Also write the report as JSON to FILE
    #[arg(long, value_name = "FILE")]
    json: Option<String>,
}

pub fn main(py: Python<'_>, argv: Vec<String>) -> PyResult<i32> {
    match Cli::try_parse_from(std::iter::once("hushed".to_owned()).chain(argv)) {
        Ok(Cli {
            command: Command::Run(args),
        }) => run(py, args),
        Ok(Cli {
            command: Command::Analyze(args),
        }) => analyze(py, args),
        Err(e) => {
            let _ = e.print(); // --help, --version or a usage error
            Ok(e.exit_code())
        }
    }
}

fn run(py: Python<'_>, args: RunArgs) -> PyResult<i32> {
    let target = args.command[0].clone();
    let path = Path::new(&target);
    let mut roots = if args.module {
        vec![paths::absolute(".")]
    } else if path.is_dir() {
        vec![paths::absolute(&target)]
    } else if path.is_file() {
        let script = paths::absolute(&target);
        vec![Path::new(&script)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or(script)]
    } else {
        return complain(py, &format!("can't open file '{target}': no such file"));
    };
    roots.extend(args.include);
    let options = tracer::Options {
        target,
        args: args.command[1..].to_vec(),
        module: args.module,
        roots,
        timing: !args.exceptions_only,
    };
    let result = tracer::run(py, &options)?;
    let _ = py.import("sys")?.getattr("stdout")?.call_method0("flush"); // the script's output first
    report::print_run(py, &result, args.all, args.top)?;
    if let Some(file) = &args.json {
        if let Err(e) = write_json(file, &report::run_json(&result)) {
            return complain(py, &e);
        }
    }
    let hidden = result.swallowed.iter().any(|s| s.hidden);
    Ok(if args.fail_on_swallowed && result.exit_code == 0 && hidden {
        3
    } else {
        result.exit_code
    })
}

fn analyze(py: Python<'_>, args: AnalyzeArgs) -> PyResult<i32> {
    let files: Vec<String> = args.paths.iter().flat_map(|p| python_files(p)).collect();
    if files.is_empty() {
        return complain(py, "no Python files found");
    }
    let reports: Vec<source::FileReport> = files.iter().map(|f| source::analyze_file(f)).collect();
    report::print_analysis(py, &reports, args.verbose)?;
    if let Some(file) = &args.json {
        if let Err(e) = write_json(file, &report::analysis_json(&reports)) {
            return complain(py, &e);
        }
    }
    Ok(if reports.iter().any(|r| r.error.is_some()) {
        1
    } else {
        0
    })
}

fn python_files(path: &str) -> Vec<String> {
    if !Path::new(path).is_dir() {
        return vec![path.to_owned()];
    }
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(path) else {
        return found;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    let (dirs, files): (Vec<_>, Vec<_>) = entries.into_iter().partition(|p| p.is_dir());
    for file in files.iter().filter(|f| f.extension().is_some_and(|x| x == "py")) {
        found.push(file.to_string_lossy().into_owned());
    }
    for dir in dirs {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str()) {
            found.extend(python_files(&dir.to_string_lossy()));
        }
    }
    found
}

fn write_json(file: &str, data: &serde_json::Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    std::fs::write(file, text + "\n").map_err(|e| format!("can't write {file}: {e}"))
}

fn complain(py: Python<'_>, message: &str) -> PyResult<i32> {
    let stderr = py.import("sys")?.getattr("stderr")?;
    stderr.call_method1("write", (format!("hushed: {message}\n"),))?;
    Ok(2)
}
