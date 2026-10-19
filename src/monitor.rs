//! The sys.monitoring callbacks that run on every call, return and exception of the watched script.
//!
//! No reference to any exception is kept (only its address while it is in flight), so frames,
//! open files and locals are freed exactly when they would be without the tool. Python code
//! (`str(exc)`) is only ever called without holding the state lock, because running Python
//! code may switch threads.

use crate::paths::Roots;
use pyo3::exceptions::PySystemExit;
use pyo3::intern;
use pyo3::prelude::*;
use rustc_hash::FxHashMap;
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

const MAX_PENDING: usize = 10_000;

/// Address of a Python object that `State` keeps alive, so the address stays unique.
type Key = usize;
/// (exception class, raising code, raise offset, catching code, catch offset)
type SwallowKey = (Key, Key, u32, Key, u32);

static NEXT_THREAD: AtomicU64 = AtomicU64::new(1);
thread_local! {
    static THREAD: Cell<u64> = const { Cell::new(0) };
}

/// A small id for the current OS thread (Python threads are OS threads).
fn thread_key() -> u64 {
    THREAD.with(|t| {
        if t.get() == 0 {
            t.set(NEXT_THREAD.fetch_add(1, Ordering::Relaxed));
        }
        t.get()
    })
}

/// What a run recorded, handed to the tracer once monitoring has stopped.
pub struct Recorded {
    pub codes: Vec<(Py<PyAny>, bool)>, // every classified code object -> is it the user's
    pub calls: Vec<(Py<PyAny>, u64, f64)>, // code, calls, seconds in outermost activations
    pub settled: Vec<Settled>,
}

/// Exceptions with the same class, raise point and catch point, after which the run went on.
pub struct Settled {
    pub exc_type: Py<PyAny>,
    pub raised: (Py<PyAny>, u32),
    pub caught: (Py<PyAny>, u32),
    pub count: u64,
    pub message: String,
}

#[derive(Default)]
struct Frames {
    starts: Vec<Instant>,
    depth: FxHashMap<Key, u32>,
}

/// An exception being followed: its address, class and code objects, never the exception.
struct Pending {
    exc_type: Key,
    origin: Key, // code of the innermost traceback frame: tells reused addresses apart
    raised: (Key, u32),
    caught: Option<(Key, u32)>,
    message: Option<String>,
    seq: u64,
}

#[derive(Default)]
struct State {
    codes: FxHashMap<Key, (Py<PyAny>, bool)>,
    kept: FxHashMap<Key, Py<PyAny>>, // other objects referred to by address
    calls: FxHashMap<Key, u64>,
    spent: FxHashMap<Key, f64>,
    threads: FxHashMap<u64, Frames>,
    pending: FxHashMap<usize, Pending>,
    settled: FxHashMap<SwallowKey, (u64, String)>, // -> (count, first message)
    seq: u64,
    exit: Option<(usize, Py<PyAny>)>, // a SystemExit on its way out: its address and its code
}

impl State {
    fn enter(&mut self, code: Key) {
        let frames = self.threads.entry(thread_key()).or_default();
        *frames.depth.entry(code).or_insert(0) += 1;
        frames.starts.push(Instant::now());
    }

    fn leave(&mut self, code: Key) {
        let now = Instant::now();
        let frames = self.threads.entry(thread_key()).or_default();
        let Some(started) = frames.starts.pop() else { return };
        let depth = frames.depth.entry(code).or_insert(1);
        *depth = depth.saturating_sub(1);
        if *depth == 0 {
            *self.spent.entry(code).or_insert(0.0) += now.duration_since(started).as_secs_f64();
        }
    }

    fn keep(&mut self, obj: &Bound<'_, PyAny>) -> Key {
        let key = obj.as_ptr() as Key;
        if !self.codes.contains_key(&key) {
            self.kept.entry(key).or_insert_with(|| obj.clone().unbind());
        }
        key
    }

    fn follow(&mut self, key: usize, exc_type: Key, origin: Key, raised: (Key, u32)) {
        self.seq += 1;
        let seq = self.seq;
        self.pending.insert(
            key,
            Pending {
                exc_type,
                origin,
                raised,
                caught: None,
                message: None,
                seq,
            },
        );
        if self.pending.len() > MAX_PENDING {
            let mut oldest: Vec<(u64, usize)> = self.pending.iter().map(|(k, p)| (p.seq, *k)).collect();
            oldest.sort_unstable();
            for (_, k) in oldest.into_iter().take(MAX_PENDING / 2) {
                self.settle(k);
            }
        }
    }

    fn settle(&mut self, key: usize) {
        let Some(p) = self.pending.remove(&key) else { return };
        if let Some((code, offset)) = p.caught {
            let message = p.message;
            let entry = self
                .settled
                .entry((p.exc_type, p.raised.0, p.raised.1, code, offset))
                .or_insert_with(|| (0, message.unwrap_or_default()));
            entry.0 += 1;
        }
    }

    fn object(&self, py: Python<'_>, key: Key) -> Py<PyAny> {
        match (self.codes.get(&key), self.kept.get(&key)) {
            (Some((obj, _)), _) | (None, Some(obj)) => obj.clone_ref(py),
            (None, None) => py.None(),
        }
    }
}

fn traceback<'py>(exc: &Bound<'py, PyAny>) -> Option<Bound<'py, PyAny>> {
    exc.getattr(intern!(exc.py(), "__traceback__"))
        .ok()
        .filter(|t| !t.is_none())
}

fn has_next(tb: &Bound<'_, PyAny>) -> bool {
    tb.getattr(intern!(tb.py(), "tb_next")).is_ok_and(|n| !n.is_none())
}

fn innermost_code<'py>(tb: &Bound<'py, PyAny>) -> Option<Bound<'py, PyAny>> {
    let py = tb.py();
    let mut tb = tb.clone();
    while let Ok(next) = tb.getattr(intern!(py, "tb_next")) {
        if next.is_none() {
            break;
        }
        tb = next;
    }
    tb.getattr(intern!(py, "tb_frame"))
        .ok()?
        .getattr(intern!(py, "f_code"))
        .ok()
}

/// `str(exc)` on one line, at most 160 characters.
pub fn message(exc: &Bound<'_, PyAny>) -> String {
    let Ok(text) = exc.str() else {
        return "<unprintable>".into();
    };
    let text = text.to_string_lossy().split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= 160 {
        text
    } else {
        text.chars().take(157).chain("...".chars()).collect()
    }
}

#[pyclass(frozen, module = "hushed._native")]
pub struct Monitor {
    roots: Roots,
    disable: Py<PyAny>,
    control_flow: Py<PyAny>,
    state: Mutex<State>,
}

impl Monitor {
    pub fn new(roots: Roots, disable: Py<PyAny>, control_flow: Py<PyAny>) -> Self {
        Monitor {
            roots,
            disable,
            control_flow,
            state: Mutex::new(State::default()),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn is_mine(&self, code: &Bound<'_, PyAny>) -> bool {
        let key = code.as_ptr() as Key;
        if let Some(&(_, mine)) = self.state().codes.get(&key) {
            return mine;
        }
        let mine = code
            .getattr(intern!(code.py(), "co_filename"))
            .and_then(|f| f.extract::<String>())
            .is_ok_and(|f| self.roots.is_mine(&f));
        self.state()
            .codes
            .entry(key)
            .or_insert_with(|| (code.clone().unbind(), mine));
        mine
    }

    fn skip(&self, py: Python<'_>, code: &Bound<'_, PyAny>, exc: &Bound<'_, PyAny>) -> bool {
        exc.is_instance(self.control_flow.bind(py)).unwrap_or(true) || !self.is_mine(code)
    }

    /// Remember the code of a SystemExit being raised; `exit_code` returns it if nothing catches it.
    fn exiting(&self, exc: &Bound<'_, PyAny>) {
        if exc.is_instance_of::<PySystemExit>() {
            let code = exc
                .getattr(intern!(exc.py(), "code"))
                .map_or_else(|_| exc.py().None(), Bound::unbind);
            self.state().exit = Some((exc.as_ptr() as usize, code));
        }
    }

    pub fn roots(&self) -> &Roots {
        &self.roots
    }

    /// The code of the SystemExit that ended the program, if one did.
    pub fn exit_code(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.state().exit.as_ref().map(|(_, code)| code.clone_ref(py))
    }

    fn start_following(&self, code: &Bound<'_, PyAny>, offset: u32, exc: &Bound<'_, PyAny>, key: usize) {
        let origin = traceback(exc).as_ref().and_then(innermost_code);
        let exc_type = exc.get_type().into_any();
        let mut st = self.state();
        let raised = (st.keep(code), offset);
        let origin = match &origin {
            Some(c) => st.keep(c),
            None => raised.0,
        };
        let exc_type = st.keep(&exc_type);
        st.follow(key, exc_type, origin, raised);
    }

    /// Settle every exception still being followed and copy out what was recorded.
    pub fn take(&self, py: Python<'_>) -> Recorded {
        let mut st = self.state();
        let keys: Vec<usize> = st.pending.keys().copied().collect();
        for key in keys {
            st.settle(key);
        }
        Recorded {
            codes: st.codes.values().map(|(c, mine)| (c.clone_ref(py), *mine)).collect(),
            calls: st
                .calls
                .iter()
                .map(|(k, n)| (st.object(py, *k), *n, st.spent.get(k).copied().unwrap_or(0.0)))
                .collect(),
            settled: st
                .settled
                .iter()
                .map(|(&(t, rc, ro, cc, co), (count, msg))| Settled {
                    exc_type: st.object(py, t),
                    raised: (st.object(py, rc), ro),
                    caught: (st.object(py, cc), co),
                    count: *count,
                    message: msg.clone(),
                })
                .collect(),
        }
    }
}

#[pymethods]
impl Monitor {
    // ---------------------------------------------------------------- timing

    fn on_start(&self, py: Python<'_>, code: &Bound<'_, PyAny>, _offset: &Bound<'_, PyAny>) -> Py<PyAny> {
        if !self.is_mine(code) {
            return self.disable.clone_ref(py);
        }
        let key = code.as_ptr() as Key;
        let mut st = self.state();
        *st.calls.entry(key).or_insert(0) += 1;
        st.enter(key);
        py.None()
    }

    fn on_resume(&self, py: Python<'_>, code: &Bound<'_, PyAny>, _offset: &Bound<'_, PyAny>) -> Py<PyAny> {
        if !self.is_mine(code) {
            return self.disable.clone_ref(py);
        }
        self.state().enter(code.as_ptr() as Key);
        py.None()
    }

    /// PY_RETURN and PY_YIELD
    fn on_leave(
        &self,
        py: Python<'_>,
        code: &Bound<'_, PyAny>,
        _offset: &Bound<'_, PyAny>,
        _value: &Bound<'_, PyAny>,
    ) -> Py<PyAny> {
        if !self.is_mine(code) {
            return self.disable.clone_ref(py);
        }
        self.state().leave(code.as_ptr() as Key);
        py.None()
    }

    /// PY_THROW (cannot be disabled)
    fn on_throw(
        &self,
        py: Python<'_>,
        code: &Bound<'_, PyAny>,
        _offset: &Bound<'_, PyAny>,
        _exc: &Bound<'_, PyAny>,
    ) -> Py<PyAny> {
        if self.is_mine(code) {
            self.state().enter(code.as_ptr() as Key);
        }
        py.None()
    }

    /// PY_UNWIND (cannot be disabled)
    fn on_unwind(
        &self,
        py: Python<'_>,
        code: &Bound<'_, PyAny>,
        _offset: &Bound<'_, PyAny>,
        _exc: &Bound<'_, PyAny>,
    ) -> Py<PyAny> {
        if self.is_mine(code) {
            self.state().leave(code.as_ptr() as Key);
        }
        py.None()
    }

    // ---------------------------------------------------------------- exceptions

    fn on_raise(&self, py: Python<'_>, code: &Bound<'_, PyAny>, offset: u32, exc: &Bound<'_, PyAny>) -> Py<PyAny> {
        self.exiting(exc);
        let key = exc.as_ptr() as usize;
        let origin = self.state().pending.get(&key).map(|p| p.origin);
        if let Some(origin) = origin {
            let same = traceback(exc)
                .is_some_and(|tb| has_next(&tb) && innermost_code(&tb).is_some_and(|c| c.as_ptr() as Key == origin));
            let mut st = self.state();
            if same {
                if let Some(p) = st.pending.get_mut(&key) {
                    p.caught = None; // same exception, propagating or raised again
                }
                return py.None();
            }
            st.settle(key); // an earlier exception that had the same address
        }
        if self.skip(py, code, exc) {
            return py.None();
        }
        self.start_following(code, offset, exc, key);
        let context = exc.getattr(intern!(py, "__context__")).ok().filter(|c| !c.is_none());
        if let Some(ctx) = &context {
            if let Some(p) = self.state().pending.get_mut(&(ctx.as_ptr() as usize)) {
                p.caught = None; // chained into the new exception, so not lost
            }
        }
        py.None()
    }

    fn on_handled(&self, py: Python<'_>, code: &Bound<'_, PyAny>, offset: u32, exc: &Bound<'_, PyAny>) -> Py<PyAny> {
        let key = exc.as_ptr() as usize;
        let known = {
            let mut st = self.state();
            if st.exit.as_ref().is_some_and(|(k, _)| *k == key) {
                st.exit = None; // caught: the program goes on
            }
            st.pending.contains_key(&key)
        };
        if !known {
            if self.skip(py, code, exc) {
                return py.None();
            }
            self.start_following(code, offset, exc, key);
        }
        let needs_message = {
            let mut st = self.state();
            let caught = (st.keep(code), offset);
            match st.pending.get_mut(&key) {
                Some(p) => {
                    p.caught = Some(caught);
                    p.message.is_none()
                }
                None => false,
            }
        };
        if needs_message {
            let text = message(exc);
            if let Some(p) = self.state().pending.get_mut(&key) {
                p.message.get_or_insert(text);
            }
        }
        py.None()
    }

    fn on_reraise(
        &self,
        py: Python<'_>,
        _code: &Bound<'_, PyAny>,
        _offset: &Bound<'_, PyAny>,
        exc: &Bound<'_, PyAny>,
    ) -> Py<PyAny> {
        self.exiting(exc);
        if let Some(p) = self.state().pending.get_mut(&(exc.as_ptr() as usize)) {
            p.caught = None;
        }
        py.None()
    }
}
