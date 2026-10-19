//! Turn results into text for people or JSON for tools. Colours are plain ANSI codes, used only when
//! writing to a terminal. Text goes through Python's own sys.stdout/sys.stderr, so it stays in
//! order with the script's output.

use crate::source::{verdict_text, FileReport, Function};
use crate::tracer::{RunResult, Swallowed};
use indexmap::IndexMap;
use pyo3::prelude::*;
use pyo3::types::PyString;
use serde_json::{json, Value};

const VERSION: &str = env!("CARGO_PKG_VERSION");

type Segment = (String, &'static str); // (text, style)

fn seg(text: impl Into<String>, style: &'static str) -> Segment {
    (text.into(), style)
}

fn plain(text: impl Into<String>) -> Segment {
    (text.into(), "")
}

/// Collects lines of styled segments, then writes them in one go.
struct Out<'py> {
    stream: Bound<'py, PyAny>,
    encoding: String,
    unicode: bool,
    lines: Vec<Vec<Segment>>,
}

impl<'py> Out<'py> {
    fn new(py: Python<'py>, stream: &str) -> PyResult<Self> {
        let stream = py.import("sys")?.getattr(stream)?;
        let encoding = stream.getattr("encoding").ok().and_then(|e| e.extract::<String>().ok());
        let encoding = encoding.unwrap_or_else(|| "utf-8".into());
        let unicode = PyString::new(py, "─✗✓←×·").call_method1("encode", (encoding.as_str(),)).is_ok();
        Ok(Out { stream, encoding, unicode, lines: Vec::new() })
    }

    fn sym(&self, fancy: &'static str, ascii: &'static str) -> &'static str {
        if self.unicode { fancy } else { ascii }
    }

    fn line(&mut self, segments: Vec<Segment>) {
        self.lines.push(segments);
    }

    fn blank(&mut self) {
        self.lines.push(Vec::new());
    }

    fn rule(&mut self, title: &str) {
        let bar = self.sym("─", "-");
        let fill = 64usize.saturating_sub(title.chars().count()).max(3);
        self.line(vec![seg(format!("{} {title} {}", bar.repeat(2), bar.repeat(fill)), "bold")]);
    }

    fn flush(self) -> PyResult<()> {
        let color = supports_color(&self.stream);
        let mut text = String::new();
        for segments in &self.lines {
            for (t, style) in segments {
                if color && !style.is_empty() && !t.is_empty() {
                    text.push_str(&format!("\x1b[{}m{t}\x1b[0m", ansi(style)));
                } else {
                    text.push_str(t);
                }
            }
            text.push('\n');
        }
        // never crash on a character the stream cannot encode (e.g. cp1252 on Windows)
        let s = PyString::new(self.stream.py(), &text);
        let enc = self.encoding.as_str();
        let safe = match s.call_method1("encode", (enc, "backslashreplace")).and_then(|b| b.call_method1("decode", (enc, "replace"))) {
            Ok(safe) => safe,
            Err(_) => s.call_method1("encode", ("ascii", "backslashreplace"))?.call_method1("decode", ("ascii",))?,
        };
        self.stream.call_method1("write", (safe,))?;
        self.stream.call_method0("flush")?;
        Ok(())
    }
}

fn ansi(style: &str) -> &'static str {
    match style {
        "bold" => "1",
        "dim" => "2",
        "red" => "31",
        "green" => "32",
        "yellow" => "33",
        "cyan" => "36",
        "bold red" => "1;31",
        "bold green" => "1;32",
        _ => "0",
    }
}

fn supports_color(stream: &Bound<'_, PyAny>) -> bool {
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) || std::env::var("TERM").is_ok_and(|t| t == "dumb") {
        return false;
    }
    if !stream.call_method0("isatty").and_then(|r| r.is_truthy()).unwrap_or(false) {
        return false;
    }
    enable_ansi(stream.call_method0("fileno").and_then(|f| f.extract()).unwrap_or(-1))
}

/// Windows consoles need virtual-terminal processing switched on before they show ANSI colours.
#[cfg(windows)]
fn enable_ansi(fd: i64) -> bool {
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, CONSOLE_MODE, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };
    let which = match fd {
        1 => STD_OUTPUT_HANDLE,
        2 => STD_ERROR_HANDLE,
        _ => return false,
    };
    // SAFETY: plain Win32 console calls on this process's own standard handle.
    unsafe {
        let handle = GetStdHandle(which);
        let mut mode: CONSOLE_MODE = 0;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return false;
        }
        mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0 || SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
    }
}

#[cfg(not(windows))]
fn enable_ansi(_fd: i64) -> bool {
    true
}

// ---------------------------------------------------------------- run report

pub fn print_run(py: Python<'_>, result: &RunResult, show_all: bool, top: usize) -> PyResult<()> {
    let mut out = Out::new(py, "stderr")?;
    out.blank();
    out.rule(&format!("whathappened: {}", result.target));
    out.line(status(result));

    let (hidden, handled): (Vec<&Swallowed>, Vec<&Swallowed>) = result.swallowed.iter().partition(|s| s.hidden);
    let looked_fine = result.status == "ok" || (result.status == "exit" && result.exit_code == 0);
    out.blank();
    if hidden.is_empty() {
        let tick = out.sym("✓", "+");
        out.line(vec![seg(format!("{tick} no swallowed errors"), "bold green")]);
    } else {
        let n: u64 = hidden.iter().map(|s| s.count).sum();
        let headline = format!("{} {n} error{} swallowed", out.sym("✗", "x"), if n > 1 { "s were" } else { " was" });
        out.line(vec![seg(headline, "bold red"), seg(if looked_fine { " (the run looked fine)" } else { "" }, "red")]);
        for s in &hidden {
            swallowed(&mut out, s);
        }
    }
    if !handled.is_empty() {
        let n: u64 = handled.iter().map(|s| s.count).sum();
        if show_all {
            out.blank();
            out.line(vec![seg(format!("Caught on purpose ({n})"), "bold")]);
            for s in &handled {
                swallowed(&mut out, s);
            }
        } else {
            let text = format!("  + {n} more caught on purpose (fallback, logged or suppressed); show them with --all");
            out.line(vec![seg(text, "dim")]);
        }
    }

    if result.timing && !result.functions.is_empty() {
        out.blank();
        out.line(vec![seg("Time per function", "bold"), seg("  (includes the calls it makes)", "dim")]);
        let shown = &result.functions[..top.min(result.functions.len())];
        let width = shown.iter().map(|f| f.function.chars().count()).max().unwrap_or(0).min(48);
        for f in shown {
            let calls = format!("{} call{}", f.calls, if f.calls == 1 { "" } else { "s" });
            out.line(vec![
                plain(format!("  {:9.3}s  ", f.seconds)),
                seg(format!("{:<width$}", f.function), "cyan"),
                plain(format!("  {calls}")),
            ]);
        }
        if result.functions.len() > shown.len() {
            out.line(vec![seg(format!("  ... {} more", result.functions.len() - shown.len()), "dim")]);
        }
    }
    if result.timing && !result.never_ran.is_empty() {
        let names: Vec<String> =
            result.never_ran.iter().take(10).map(|l| format!("{} ({}:{})", l.function, l.file, l.line)).collect();
        let more = result.never_ran.len() - names.len();
        let tail = if more > 0 { format!(" and {more} more") } else { String::new() };
        out.blank();
        out.line(vec![seg(format!("Never ran ({}): ", result.never_ran.len()), "bold"), plain(names.join(", ") + &tail)]);
    }
    out.blank();
    out.flush()
}

fn status(r: &RunResult) -> Vec<Segment> {
    let took = format!(" in {:.2}s", r.seconds);
    match r.status {
        "ok" => vec![seg("finished normally", "green"), plain(took)],
        "exit" => {
            let style = if r.exit_code == 0 { "green" } else { "yellow" };
            vec![seg(format!("exited with code {}", r.exit_code), style), plain(took)]
        }
        "interrupted" => vec![seg("interrupted", "yellow"), plain(format!(" after {:.2}s", r.seconds))],
        _ => {
            let at = r.error_at.as_ref().map(|a| format!(" at {} ({}:{})", a.function, a.file, a.line));
            let error = r.error.as_deref().unwrap_or("");
            vec![seg("crashed", "bold red"), plain(took), plain(format!(": {error}{}", at.unwrap_or_default()))]
        }
    }
}

fn swallowed(out: &mut Out, s: &Swallowed) {
    let title = if s.message.is_empty() { s.exc_type.clone() } else { format!("{}: {}", s.exc_type, s.message) };
    let times = if s.count > 1 { format!("  {}{}", out.sym("×", "x"), s.count) } else { String::new() };
    let arrow = out.sym("←", "<-");
    let width = s.raised.function.chars().count().max(s.caught.function.chars().count());
    out.line(vec![plain("  "), seg(title, "bold"), seg(times, "yellow")]);
    out.line(vec![
        seg("      raised  ", "dim"),
        plain(format!("{:<width$}  {}:{}", s.raised.function, s.raised.file, s.raised.line)),
    ]);
    out.line(vec![
        seg("      caught  ", "dim"),
        plain(format!("{:<width$}  {}:{}", s.caught.function, s.caught.file, s.caught.line)),
        seg(format!("  {arrow} {}", s.reason), if s.hidden { "yellow" } else { "dim" }),
    ]);
}

// ---------------------------------------------------------------- static report

pub fn print_analysis(py: Python<'_>, reports: &[FileReport], verbose: bool) -> PyResult<()> {
    let mut out = Out::new(py, "stdout")?;
    for r in reports {
        out.blank();
        if let Some(error) = &r.error {
            out.rule(&r.path);
            out.line(vec![seg("  could not analyze: ", "red"), plain(error.clone())]);
            continue;
        }
        let hiding: Vec<_> = r.handlers.iter().filter(|h| h.hides_errors).collect();
        let dot = out.sym("·", "-");
        out.rule(&format!(
            "{} {dot} {} functions {dot} {} except blocks ({} can hide errors) {dot} {} print() calls",
            r.path, r.functions.len(), r.handlers.len(), hiding.len(), r.prints.len()
        ));
        if !r.functions.is_empty() {
            functions_table(&mut out, &r.functions);
        }
        if !hiding.is_empty() {
            out.blank();
            out.line(vec![seg("  except blocks that can hide errors", "bold")]);
            let width = hiding.iter().map(|h| h.scope.chars().count()).max().unwrap_or(0);
            for h in hiding {
                out.line(vec![
                    plain(format!("  {:>5}  ", h.line)),
                    seg(format!("{:<width$}", h.scope), "cyan"),
                    plain(format!("  `{}` ", h.clause)),
                    seg(verdict_text(h.verdict), "yellow"),
                ]);
            }
        }
        if verbose {
            let mut prints: IndexMap<&str, Vec<String>> = IndexMap::new();
            for (line, scope) in &r.prints {
                prints.entry(scope).or_default().push(line.to_string());
            }
            by_scope(&mut out, "print() calls", prints.into_iter().map(|(k, v)| (k.to_owned(), v)).collect());
            let variables = r.variables.iter().map(|(k, v)| (k.clone(), v.iter().cloned().collect())).collect();
            by_scope(&mut out, "assigned variables", variables);
        }
    }
    out.blank();
    out.flush()
}

fn functions_table(out: &mut Out, functions: &[Function]) {
    let head = ["line", "function", "args", "complexity", "returns", "docstring"].map(String::from);
    let rows: Vec<[String; 6]> = functions
        .iter()
        .map(|f| {
            let args = if f.args.is_empty() { "-".to_owned() } else { f.args.join(", ") };
            [
                f.line.to_string(),
                format!("{}{}", if f.is_async { "async " } else { "" }, f.qualname),
                clip(&args, 32),
                f.complexity.to_string(),
                f.returns.to_owned(),
                (if f.docstring { "yes" } else { "no" }).to_owned(),
            ]
        })
        .collect();
    let widths: Vec<usize> = (0..6)
        .map(|i| rows.iter().map(|r| r[i].chars().count()).chain([head[i].len()]).max().unwrap_or(0))
        .collect();
    out.blank();
    out.line(vec![seg(format!("  {}", cells(&head, &widths)), "dim")]);
    for (row, f) in rows.iter().zip(functions) {
        out.line(vec![plain("  "), seg(cells(row, &widths), if f.complexity > 10 { "yellow" } else { "" })]);
    }
}

fn cells(row: &[String; 6], widths: &[usize]) -> String {
    let cells: Vec<String> = row
        .iter()
        .zip(widths)
        .enumerate()
        .map(|(i, (cell, &w))| if i == 0 || i == 3 { format!("{cell:>w$}") } else { format!("{cell:<w$}") })
        .collect();
    cells.join("  ").trim_end().to_owned()
}

fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_owned()
    } else {
        text.chars().take(width - 3).collect::<String>() + "..."
    }
}

fn by_scope(out: &mut Out, title: &str, items: Vec<(String, Vec<String>)>) {
    if items.is_empty() {
        return;
    }
    out.blank();
    out.line(vec![seg(format!("  {title}"), "bold")]);
    for (scope, values) in items {
        out.line(vec![plain("    "), seg(scope, "cyan"), plain(format!(": {}", values.join(", ")))]);
    }
}

// ---------------------------------------------------------------- JSON

pub fn run_json(result: &RunResult) -> Value {
    let mut run = serde_json::to_value(result).unwrap_or(Value::Null);
    let hidden: u64 = result.swallowed.iter().filter(|s| s.hidden).map(|s| s.count).sum();
    let on_purpose: u64 = result.swallowed.iter().filter(|s| !s.hidden).map(|s| s.count).sum();
    if let Value::Object(map) = &mut run {
        map.insert("summary".into(), json!({"hidden_errors": hidden, "caught_on_purpose": on_purpose}));
    }
    json!({"tool": "whathappened", "version": VERSION, "run": run})
}

pub fn analysis_json(reports: &[FileReport]) -> Value {
    json!({"tool": "whathappened", "version": VERSION, "files": reports})
}
