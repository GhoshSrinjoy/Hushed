//! Run a script under sys.monitoring, then turn what the monitor recorded into a report.

use crate::monitor::{self, Monitor, Recorded, Settled};
use crate::paths::{self, Roots};
use crate::source::{self, Handler, Parsed, Site};
use pyo3::exceptions::{
    PyGeneratorExit, PyKeyboardInterrupt, PyRuntimeError, PyStopAsyncIteration, PyStopIteration, PySystemExit,
};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use pyo3::PyTypeInfo;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

const HOOKS: [(&str, &str); 3] = [
    ("RAISE", "on_raise"),
    ("EXCEPTION_HANDLED", "on_handled"),
    ("RERAISE", "on_reraise"),
];
const TIMING_HOOKS: [(&str, &str); 6] = [
    ("PY_START", "on_start"),
    ("PY_RESUME", "on_resume"),
    ("PY_THROW", "on_throw"),
    ("PY_RETURN", "on_leave"),
    ("PY_YIELD", "on_leave"),
    ("PY_UNWIND", "on_unwind"),
];

#[derive(Clone, Debug, Serialize)]
pub struct Location {
    pub file: String,
    pub line: usize,
    pub function: String,
}

/// Exceptions with the same type, raise point and catch point, after which the run went on.
#[derive(Debug, Serialize)]
pub struct Swallowed {
    #[serde(rename = "type")]
    pub exc_type: String,
    pub message: String, // of the first occurrence
    pub count: u64,
    pub raised: Location,
    pub caught: Location,
    pub kind: &'static str, // "except", "with", "finally", "thread", "library" or "unknown"
    pub hidden: bool,       // true: the error vanished; false: caught on purpose and handled visibly
    pub handler: Option<Handler>,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct FunctionStats {
    pub function: String,
    pub file: String,
    pub line: usize,
    pub calls: u64,
    pub seconds: f64, // inclusive; recursive calls are not double counted
}

#[derive(Debug, Serialize)]
pub struct RunResult {
    pub target: String,
    pub status: &'static str, // "ok", "exit", "crash" or "interrupted"
    pub exit_code: i32,
    pub seconds: f64,
    pub timing: bool,
    pub error: Option<String>,
    pub error_at: Option<Location>,
    pub swallowed: Vec<Swallowed>,
    pub functions: Vec<FunctionStats>,
    pub never_ran: Vec<Location>,
    pub files: Vec<String>,
}

pub struct Options {
    pub target: String,
    pub args: Vec<String>,
    pub module: bool,
    pub roots: Vec<String>,
    pub timing: bool,
}

/// `sys.monitoring` switched on for one `Monitor`, until `stop`.
pub struct Watch {
    tool: u8,
    events: Vec<i64>,
    pub timing: bool,
    pub monitor: Py<Monitor>,
}

impl Watch {
    /// Start watching the code under `roots`, except the standard library and installed packages.
    pub fn start(py: Python<'_>, roots: &[String], timing: bool) -> PyResult<Watch> {
        let mon = py.import("sys")?.getattr("monitoring")?;
        let tool = (3..=4u8) // the tool ids with no assigned role
            .find(|i| mon.call_method1("get_tool", (*i,)).is_ok_and(|t| t.is_none()))
            .ok_or_else(|| PyRuntimeError::new_err("no free sys.monitoring tool id (another tool is using 3 and 4)"))?;
        let control_flow = PyTuple::new(
            py,
            [
                PyStopIteration::type_object(py),
                PyStopAsyncIteration::type_object(py),
                PyGeneratorExit::type_object(py),
            ],
        )?;
        let monitor = Bound::new(
            py,
            Monitor::new(
                Roots::new(roots, &skip_dirs(py)?),
                mon.getattr("DISABLE")?.unbind(),
                control_flow.into_any().unbind(),
            ),
        )?;

        let events = mon.getattr("events")?;
        let timing_hooks: &[(&str, &str)] = if timing { &TIMING_HOOKS } else { &[] };
        let mut hooks: Vec<(i64, Bound<PyAny>)> = Vec::new();
        for (event, method) in HOOKS.iter().chain(timing_hooks) {
            hooks.push((events.getattr(*event)?.extract()?, monitor.getattr(*method)?));
        }
        mon.call_method1("use_tool_id", (tool, "hushed"))?;
        let watch = Watch {
            tool,
            events: hooks.iter().map(|(id, _)| *id).collect(),
            timing,
            monitor: monitor.unbind(),
        };
        let started = hooks
            .iter()
            .try_for_each(|(id, hook)| mon.call_method1("register_callback", (tool, *id, hook)).map(drop))
            .and_then(|_| mon.call_method1("set_events", (tool, watch.events.iter().sum::<i64>())));
        if let Err(e) = started {
            watch.stop(py);
            return Err(e);
        }
        Ok(watch)
    }

    pub fn stop(&self, py: Python<'_>) {
        let Ok(mon) = py.import("sys").and_then(|sys| sys.getattr("monitoring")) else {
            return;
        };
        let _ = mon.call_method1("set_events", (self.tool, 0));
        for id in &self.events {
            let _ = mon.call_method1("register_callback", (self.tool, *id, py.None()));
        }
        let _ = mon.call_method1("free_tool_id", (self.tool,));
    }

    pub fn roots(&self) -> &Roots {
        self.monitor.get().roots()
    }

    /// Fill `result` with what was recorded. Call it after `stop`.
    pub fn fill(&self, py: Python<'_>, result: &mut RunResult) -> PyResult<()> {
        let recorded = self.monitor.get().take(py);
        Builder {
            py,
            roots: self.roots().clone(),
            lines: HashMap::new(),
            parsed: HashMap::new(),
        }
        .fill(result, recorded)
    }
}

impl RunResult {
    pub fn new(target: &str, seconds: f64, timing: bool) -> Self {
        RunResult {
            target: target.to_owned(),
            status: "ok",
            exit_code: 0,
            seconds,
            timing,
            error: None,
            error_at: None,
            swallowed: Vec::new(),
            functions: Vec::new(),
            never_ran: Vec::new(),
            files: Vec::new(),
        }
    }

    /// The run ended with an uncaught exception, whose traceback Python has printed.
    pub fn crashed(&mut self, exc: &Bound<'_, PyAny>, tb: &Bound<'_, PyAny>, roots: &Roots) -> PyResult<()> {
        let interrupted = exc.is_instance_of::<PyKeyboardInterrupt>();
        (self.status, self.exit_code) = if interrupted {
            ("interrupted", 130)
        } else {
            ("crash", 1)
        };
        let name: String = exc.get_type().getattr("__name__")?.extract()?;
        let text = monitor::message(exc);
        self.error = Some(if text.is_empty() {
            name
        } else {
            format!("{name}: {text}")
        });
        self.error_at = crash_location(tb, roots)?;
        Ok(())
    }

    /// The run ended with `sys.exit(code)`; Python's rules turn the code into an exit status.
    pub fn exited(&mut self, code: &Bound<'_, PyAny>) {
        self.status = "exit";
        self.exit_code = if code.is_none() {
            0
        } else {
            code.extract::<i64>().map_or(1, |n| n as i32)
        };
    }
}

pub fn run(py: Python<'_>, opts: &Options) -> PyResult<RunResult> {
    // like `python script.py` (or `python folder/`, or `python -m module`)
    let script = paths::absolute(&opts.target);
    let path0 = if opts.module {
        paths::absolute(".")
    } else if Path::new(&script).is_dir() {
        script.clone()
    } else {
        Path::new(&script)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let sys = py.import("sys")?;
    let runpy = py.import("runpy")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("run_name", "__main__")?;
    if opts.module {
        kwargs.set_item("alter_sys", true)?;
    }

    let watch = Watch::start(py, &opts.roots, opts.timing)?;
    let (sys_path, saved_argv) = (sys.getattr("path")?, sys.getattr("argv")?);
    let saved_path0 = sys_path.get_item(0)?;
    sys.setattr(
        "argv",
        PyList::new(py, std::iter::once(&opts.target).chain(&opts.args))?,
    )?;
    sys_path.set_item(0, path0)?;
    let clock = Instant::now();
    let outcome = if opts.module {
        runpy.call_method("run_module", (&opts.target,), Some(&kwargs))
    } else {
        runpy.call_method("run_path", (&script,), Some(&kwargs))
    };
    let seconds = clock.elapsed().as_secs_f64();
    watch.stop(py);
    sys.setattr("argv", saved_argv)?;
    sys_path.set_item(0, saved_path0)?;

    let mut result = RunResult::new(&opts.target, seconds, opts.timing);
    if let Err(err) = outcome {
        let value = err.value(py).clone().into_any();
        if err.is_instance_of::<PySystemExit>(py) {
            let code = value.getattr("code")?;
            if !code.is_none() && code.extract::<i64>().is_err() {
                // like Python, print a code that isn't a number (usually a message) to stderr
                let kwargs = PyDict::new(py);
                kwargs.set_item("file", sys.getattr("stderr")?)?;
                py.import("builtins")?.getattr("print")?.call((&code,), Some(&kwargs))?;
            }
            result.exited(&code);
        } else {
            let tb = user_traceback(py, &value)?;
            value.setattr("__traceback__", &tb)?; // 3.12's hook ignores its tb argument when one is attached
            sys.getattr("excepthook")?.call1((value.get_type(), &value, &tb))?; // print it the way Python would
            result.crashed(&value, &tb, watch.roots())?;
        }
    }
    watch.fill(py, &mut result)?;
    Ok(result)
}

struct Builder<'py> {
    py: Python<'py>,
    roots: Roots,
    lines: HashMap<usize, Vec<(usize, usize, Option<usize>)>>, // code address -> co_lines()
    parsed: HashMap<String, Option<Parsed>>,
}

impl Builder<'_> {
    fn fill(&mut self, result: &mut RunResult, rec: Recorded) -> PyResult<()> {
        let py = self.py;
        for s in &rec.settled {
            let swallowed = self.swallowed(s)?;
            result.swallowed.push(swallowed);
        }
        result.swallowed.sort_by(|a, b| {
            let key = |s: &Swallowed| {
                (
                    !s.hidden,
                    Reverse(s.count),
                    s.raised.file.clone(),
                    s.raised.line,
                    s.caught.file.clone(),
                    s.caught.line,
                )
            };
            key(a).cmp(&key(b))
        });
        let mut files = BTreeSet::new();
        for (code, mine) in &rec.codes {
            if *mine {
                files.insert(info(code.bind(py))?.0);
            }
        }
        result.files = files.iter().map(|f| self.roots.display(f)).collect();
        if !result.timing {
            return Ok(());
        }

        let mut executed = HashSet::new();
        for (code, calls, seconds) in &rec.calls {
            let code = code.bind(py);
            let (file, function, line) = info(code)?;
            executed.insert((paths::normcase(&file), function.clone(), line));
            // named functions only: not module bodies, lambdas or generator expressions
            if !code.getattr("co_name")?.extract::<String>()?.starts_with('<') {
                let file = self.roots.display(&file);
                result.functions.push(FunctionStats {
                    function,
                    file,
                    line,
                    calls: *calls,
                    seconds: *seconds,
                });
            }
        }
        result.functions.sort_by(|a, b| {
            b.seconds
                .total_cmp(&a.seconds)
                .then_with(|| (&a.file, a.line).cmp(&(&b.file, b.line)))
        });
        for file in &files {
            let key = paths::normcase(file);
            let shown = self.roots.display(file);
            let Some(parsed) = self.parsed(file) else { continue };
            let missing = parsed.report.functions.iter().filter(|f| {
                !f.decorators
                    .iter()
                    .any(|d| d.ends_with("overload") || d.ends_with("abstractmethod"))
                    && ![f.line, f.first_line]
                        .iter()
                        .any(|l| executed.contains(&(key.clone(), f.qualname.clone(), *l)))
            });
            let missing: Vec<Location> = missing
                .map(|f| Location {
                    file: shown.clone(),
                    line: f.line,
                    function: f.qualname.clone(),
                })
                .collect();
            result.never_ran.extend(missing);
        }
        Ok(())
    }

    fn swallowed(&mut self, s: &Settled) -> PyResult<Swallowed> {
        let py = self.py;
        let (raised_code, caught_code, exc_type) = (s.raised.0.bind(py), s.caught.0.bind(py), s.exc_type.bind(py));
        let raised = self.location(raised_code, s.raised.1)?;
        let mut caught = self.location(caught_code, s.caught.1)?;
        let file = info(caught_code)?.0;
        let (kind, hidden, handler) = if !self.roots.is_mine(&file) {
            let thread = Path::new(&file).file_name().is_some_and(|n| n == "threading.py");
            (if thread { "thread" } else { "library" }, true, None)
        } else {
            let names: HashSet<String> = exc_type
                .getattr("__mro__")?
                .try_iter()?
                .filter_map(|c| c.ok()?.getattr("__name__").ok()?.extract().ok())
                .collect();
            let line = caught.line;
            match self.parsed(&file).map(|p| p.catch_site(line, &names)) {
                Some(Site::Finally) => ("finally", true, None),
                Some(Site::With) => ("with", false, None),
                Some(Site::Except(h)) => (
                    "except",
                    h.hides_errors || (h.verdict == "reraised" && h.broad),
                    Some(h.clone()),
                ),
                _ => ("unknown", false, None),
            }
        };
        if let Some(h) = &handler {
            caught.line = h.line; // the clause that matched, not the first clause of the try
        }
        Ok(Swallowed {
            exc_type: exc_type.getattr("__name__")?.extract()?,
            message: s.message.clone(),
            count: s.count,
            reason: reason(kind, handler.as_ref()),
            raised,
            caught,
            kind,
            hidden,
            handler,
        })
    }

    fn parsed(&mut self, file: &str) -> Option<&Parsed> {
        self.parsed
            .entry(file.to_owned())
            .or_insert_with(|| source::parse(file).ok())
            .as_ref()
    }

    fn location(&mut self, code: &Bound<'_, PyAny>, offset: u32) -> PyResult<Location> {
        let (file, function, first) = info(code)?;
        let table = match self.lines.entry(code.as_ptr() as usize) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                let mut table = Vec::new();
                for item in code.call_method0("co_lines")?.try_iter()? {
                    table.push(item?.extract()?);
                }
                e.insert(table)
            }
        };
        let line = table
            .iter()
            .find(|(_, end, line)| *end > offset as usize && line.is_some())
            .and_then(|(_, _, line)| *line)
            .unwrap_or(first);
        Ok(Location {
            file: self.roots.display(&file),
            line,
            function,
        })
    }
}

fn reason(kind: &str, handler: Option<&Handler>) -> String {
    match (kind, handler) {
        ("except", Some(h)) if h.verdict == "reraised" => format!("`{}` did not reach its `raise` this time", h.clause),
        ("except", Some(h)) => format!("`{}` {}", h.clause, source::verdict_text(h.verdict)),
        ("with", _) => "suppressed by a `with` block".into(),
        ("finally", _) => "dropped by a `finally` block (return/break/continue inside it)".into(),
        ("thread", _) => "killed its thread; the rest of the program kept going".into(),
        ("library", _) => "caught inside library code".into(),
        _ => "caught (source not available)".into(),
    }
}

/// (co_filename, co_qualname, co_firstlineno)
fn info(code: &Bound<'_, PyAny>) -> PyResult<(String, String, usize)> {
    Ok((
        code.getattr("co_filename")?.extract()?,
        code.getattr("co_qualname")?.extract()?,
        code.getattr("co_firstlineno")?.extract()?,
    ))
}

/// The standard library, installed packages and hushed itself: never watched.
fn skip_dirs(py: Python<'_>) -> PyResult<Vec<String>> {
    let paths = py.import("sysconfig")?.call_method0("get_paths")?;
    let mut dirs = Vec::new();
    for key in ["stdlib", "platstdlib", "purelib", "platlib"] {
        if let Ok(dir) = paths.get_item(key) {
            dirs.push(dir.extract()?);
        }
    }
    dirs.extend(package_dir(py));
    Ok(dirs)
}

fn package_dir(py: Python<'_>) -> Option<String> {
    let file: String = py.import("hushed").ok()?.getattr("__file__").ok()?.extract().ok()?;
    Path::new(&file).parent().map(|p| p.to_string_lossy().into_owned())
}

/// The exception's traceback without the frames of runpy and hushed at the top.
fn user_traceback<'py>(py: Python<'py>, exc: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let runpy_file = py
        .import("runpy")?
        .getattr("__file__")
        .ok()
        .and_then(|f| f.extract::<String>().ok());
    let runpy_file = runpy_file.map(|f| paths::normcase(&f));
    let package = package_dir(py).map(|d| paths::normcase(&d));
    let mut tb = exc.getattr("__traceback__")?;
    while !tb.is_none() {
        let file = paths::normcase(
            &tb.getattr("tb_frame")?
                .getattr("f_code")?
                .getattr("co_filename")?
                .extract::<String>()?,
        );
        let ours = runpy_file.as_deref() == Some(file.as_str())
            || file.starts_with("<frozen")
            || package.as_ref().is_some_and(|p| Path::new(&file).starts_with(p));
        if !ours {
            break;
        }
        tb = tb.getattr("tb_next")?;
    }
    Ok(tb)
}

fn crash_location(tb: &Bound<'_, PyAny>, roots: &Roots) -> PyResult<Option<Location>> {
    let mut found = None;
    let mut tb = tb.clone();
    while !tb.is_none() {
        let code = tb.getattr("tb_frame")?.getattr("f_code")?;
        let (file, function, _) = info(&code)?;
        if roots.is_mine(&file) {
            found = Some(Location {
                file: roots.display(&file),
                line: tb.getattr("tb_lineno")?.extract()?,
                function,
            });
        }
        tb = tb.getattr("tb_next")?;
    }
    Ok(found)
}
