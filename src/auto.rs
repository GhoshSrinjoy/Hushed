//! `import hushed`: watch the program that imported it, from that point on, and print the report
//! when it exits.
//!
//! The report is printed by an `atexit` handler. By then Python has printed the traceback of an
//! uncaught exception (and left it in `sys.last_exc`); a `sys.exit()` code is remembered by the
//! monitor. Only the main process reports: forked and `multiprocessing` children stay quiet.

use crate::cli;
use crate::paths;
use crate::report;
use crate::tracer::{RunResult, Watch};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// The session `import hushed` started, until the report is printed or it is cancelled.
static ACTIVE: Mutex<Option<Py<Session>>> = Mutex::new(None);

#[pyclass(frozen, module = "hushed._native")]
pub struct Session {
    watch: Watch,
    target: String,
    started: Instant,
    show_all: bool,
    top: usize,
    json: Option<String>,
    done: AtomicBool,
}

#[pymethods]
impl Session {
    /// Registered with `atexit`: stop watching and print the report.
    fn report(&self, py: Python<'_>) -> PyResult<()> {
        if self.done.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.watch.stop(py);
        let mut result = RunResult::new(&self.target, self.started.elapsed().as_secs_f64(), self.watch.timing);
        let crash = py.import("sys")?.getattr("last_exc").ok().filter(|e| !e.is_none());
        if let Some(exc) = crash {
            result.crashed(&exc, &exc.getattr("__traceback__")?, self.watch.roots())?;
        } else if let Some(code) = self.watch.monitor.get().exit_code(py) {
            result.exited(code.bind(py));
        }
        self.watch.fill(py, &mut result)?;
        let _ = py.import("sys")?.getattr("stdout")?.call_method0("flush"); // the program's output first
        report::print_run(py, &result, self.show_all, self.top)?;
        if let Some(file) = &self.json {
            if let Err(e) = cli::write_json(file, &report::run_json(&result)) {
                cli::complain(py, &e)?;
            }
        }
        Ok(())
    }

    /// Stop watching without a report.
    fn cancel(&self, py: Python<'_>) {
        if !self.done.swap(true, Ordering::SeqCst) {
            self.watch.stop(py);
        }
    }
}

/// Called by `import hushed`. It never fails: when it can't watch, it says why and lets the
/// program run.
#[pyfunction]
pub fn watch(py: Python<'_>) -> PyResult<()> {
    if lock().is_some() || switched_off() || in_child(py).unwrap_or(false) {
        return Ok(());
    }
    if let Err(e) = start(py) {
        cli::complain(py, &format!("not watching this program: {}", e.value(py)))?;
    }
    Ok(())
}

/// Stop the session `import hushed` started, without a report. `hushed run` and `hushed analyze`
/// call it first, because `python -m hushed` imports the package before the command line runs.
pub fn cancel(py: Python<'_>) {
    let session = lock().take();
    if let Some(session) = session {
        session.get().cancel(py);
    }
}

fn start(py: Python<'_>) -> PyResult<()> {
    let mut roots = vec![main_folder(py)?];
    if let Some(dirs) = setting("HUSHED_INCLUDE") {
        roots.extend(std::env::split_paths(&dirs).map(|d| d.to_string_lossy().into_owned()));
    }
    let watch = Watch::start(py, &roots, !flag("HUSHED_EXCEPTIONS_ONLY"))?;
    let session = Session {
        watch,
        target: target(py)?,
        started: Instant::now(),
        show_all: flag("HUSHED_ALL"),
        top: setting("HUSHED_TOP").and_then(|n| n.parse().ok()).unwrap_or(10),
        json: setting("HUSHED_JSON").map(|f| paths::absolute(&f)),
        done: AtomicBool::new(false),
    };
    let session = Bound::new(py, session)?;
    let registered = py
        .import("atexit")?
        .call_method1("register", (session.getattr("report")?,))
        .and_then(|_| {
            let os = py.import("os")?;
            if os.hasattr("register_at_fork")? {
                let kwargs = PyDict::new(py);
                kwargs.set_item("after_in_child", session.getattr("cancel")?)?;
                os.call_method("register_at_fork", (), Some(&kwargs))?;
            }
            Ok(())
        });
    if let Err(e) = registered {
        session.get().cancel(py);
        return Err(e);
    }
    *lock() = Some(session.unbind());
    Ok(())
}

fn lock() -> std::sync::MutexGuard<'static, Option<Py<Session>>> {
    ACTIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn setting(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn flag(name: &str) -> bool {
    setting(name).is_some_and(|v| !["0", "false", "no", "off"].contains(&v.trim().to_lowercase().as_str()))
}

/// `HUSHED=0` (or `false`, `no`, `off`) makes `import hushed` do nothing.
fn switched_off() -> bool {
    setting("HUSHED").is_some_and(|_| !flag("HUSHED"))
}

/// A `multiprocessing` child: either re-importing the main script before it starts, or running.
fn in_child(py: Python<'_>) -> PyResult<bool> {
    let Ok(mp) = py.import("sys")?.getattr("modules")?.get_item("multiprocessing") else {
        return Ok(false);
    };
    if !mp.call_method0("parent_process")?.is_none() {
        return Ok(true);
    }
    let current = mp.call_method0("current_process")?;
    Ok(current
        .getattr("_inheriting")
        .is_ok_and(|v| v.is_truthy().unwrap_or(false)))
}

/// The folder whose code is watched, chosen the way `hushed run` chooses it: the script's folder
/// (or the folder given to `python folder/`), else the current directory (`python -m`, `-c`, a REPL).
fn main_folder(py: Python<'_>) -> PyResult<String> {
    let main = py.import("sys")?.getattr("modules")?.get_item("__main__").ok();
    let file = main
        .as_ref()
        .and_then(|m| m.getattr("__file__").ok()?.extract::<String>().ok());
    let module = main
        .and_then(|m| m.getattr("__spec__").ok())
        .filter(|spec| !spec.is_none())
        .and_then(|spec| spec.getattr("name").ok()?.extract::<String>().ok());
    let folder = match (file, module.as_deref()) {
        (Some(file), None | Some("__main__")) => Path::new(&paths::absolute(&file))
            .parent()
            .map(|p| p.to_string_lossy().into_owned()),
        _ => None,
    };
    Ok(folder.unwrap_or_else(|| paths::absolute(".")))
}

/// What the report calls the program: the script as it was given to Python.
fn target(py: Python<'_>) -> PyResult<String> {
    let argv0: String = py
        .import("sys")?
        .getattr("argv")?
        .get_item(0)
        .and_then(|a| a.extract())
        .unwrap_or_default();
    Ok(match argv0.as_str() {
        "" => "python".into(),
        "-c" => "python -c".into(),
        _ => argv0,
    })
}
