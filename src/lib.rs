//! Rust engine for whathappened: the sys.monitoring callbacks that run on every call, return
//! and exception of the watched script. It mirrors `Tracer._callbacks()` in tracer.py, which
//! stays as the pure-Python fallback, and the tests run both engines against each other.
//!
//! No reference to any exception is kept (only its address while it is in flight). Python code
//! (classifying a file path, `str(exc)`) is only ever called without holding the state lock,
//! because running Python code may switch threads.

use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
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
    codes: FxHashMap<Key, (Py<PyAny>, bool)>, // classified code objects -> is it the user's code
    kept: FxHashMap<Key, Py<PyAny>>,          // other objects referred to by address
    calls: FxHashMap<Key, u64>,
    spent: FxHashMap<Key, f64>, // seconds in outermost activations only
    threads: FxHashMap<u64, Frames>,
    pending: FxHashMap<usize, Pending>,
    settled: FxHashMap<SwallowKey, (u64, String)>, // -> (count, first message)
    seq: u64,
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
        let Some(started) = frames.starts.pop() else {
            return;
        };
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
        self.pending.insert(key, Pending { exc_type, origin, raised, caught: None, message: None, seq });
        if self.pending.len() > MAX_PENDING {
            let mut oldest: Vec<(u64, usize)> = self.pending.iter().map(|(k, p)| (p.seq, *k)).collect();
            oldest.sort_unstable();
            for (_, k) in oldest.into_iter().take(MAX_PENDING / 2) {
                self.settle(k);
            }
        }
    }

    fn settle(&mut self, key: usize) {
        let Some(p) = self.pending.remove(&key) else {
            return;
        };
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
        match self.codes.get(&key) {
            Some((obj, _)) => obj.clone_ref(py),
            None => match self.kept.get(&key) {
                Some(obj) => obj.clone_ref(py),
                None => py.None(),
            },
        }
    }
}

fn traceback<'py>(exc: &Bound<'py, PyAny>) -> Option<Bound<'py, PyAny>> {
    exc.getattr(intern!(exc.py(), "__traceback__")).ok().filter(|t| !t.is_none())
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
    tb.getattr(intern!(py, "tb_frame")).ok()?.getattr(intern!(py, "f_code")).ok()
}

#[pyclass(frozen, module = "whathappened._native")]
struct Monitor {
    classify: Py<PyAny>,
    message: Py<PyAny>,
    disable: Py<PyAny>,
    control_flow: Py<PyAny>,
    own_code: Key,
    state: Mutex<State>,
}

impl Monitor {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn is_mine(&self, code: &Bound<'_, PyAny>) -> bool {
        let key = code.as_ptr() as Key;
        if let Some(&(_, mine)) = self.state().codes.get(&key) {
            return mine;
        }
        let mine = self
            .classify
            .bind(code.py())
            .call1((code,))
            .and_then(|r| r.is_truthy())
            .unwrap_or(false);
        self.state().codes.entry(key).or_insert_with(|| (code.clone().unbind(), mine));
        mine
    }

    fn skip(&self, py: Python<'_>, code: &Bound<'_, PyAny>, exc: &Bound<'_, PyAny>) -> bool {
        exc.is_instance(self.control_flow.bind(py)).unwrap_or(true) || !self.is_mine(code)
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
}

#[pymethods]
impl Monitor {
    #[new]
    fn new(
        classify: Py<PyAny>,
        message: Py<PyAny>,
        disable: Py<PyAny>,
        own_code: &Bound<'_, PyAny>,
        control_flow: Py<PyAny>,
    ) -> Self {
        Monitor {
            classify,
            message,
            disable,
            control_flow,
            own_code: own_code.as_ptr() as Key,
            state: Mutex::new(State::default()),
        }
    }

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
    fn on_throw(&self, py: Python<'_>, code: &Bound<'_, PyAny>, _offset: &Bound<'_, PyAny>, _exc: &Bound<'_, PyAny>) -> Py<PyAny> {
        if self.is_mine(code) {
            self.state().enter(code.as_ptr() as Key);
        }
        py.None()
    }

    /// PY_UNWIND (cannot be disabled)
    fn on_unwind(&self, py: Python<'_>, code: &Bound<'_, PyAny>, _offset: &Bound<'_, PyAny>, _exc: &Bound<'_, PyAny>) -> Py<PyAny> {
        if self.is_mine(code) {
            self.state().leave(code.as_ptr() as Key);
        }
        py.None()
    }

    // ---------------------------------------------------------------- exceptions

    fn on_raise(&self, py: Python<'_>, code: &Bound<'_, PyAny>, offset: u32, exc: &Bound<'_, PyAny>) -> Py<PyAny> {
        let key = exc.as_ptr() as usize;
        let origin = self.state().pending.get(&key).map(|p| p.origin);
        if let Some(origin) = origin {
            let same = traceback(exc).is_some_and(|tb| {
                has_next(&tb) && innermost_code(&tb).is_some_and(|c| c.as_ptr() as Key == origin)
            });
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
        if code.as_ptr() as Key == self.own_code {
            return py.None(); // whathappened's own `except` around the run
        }
        let key = exc.as_ptr() as usize;
        let known = self.state().pending.contains_key(&key);
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
            let message = self
                .message
                .bind(py)
                .call1((exc,))
                .and_then(|m| m.extract::<String>())
                .unwrap_or_else(|_| "<unprintable>".to_owned());
            if let Some(p) = self.state().pending.get_mut(&key) {
                p.message.get_or_insert(message);
            }
        }
        py.None()
    }

    fn on_reraise(&self, py: Python<'_>, _code: &Bound<'_, PyAny>, _offset: &Bound<'_, PyAny>, exc: &Bound<'_, PyAny>) -> Py<PyAny> {
        if let Some(p) = self.state().pending.get_mut(&(exc.as_ptr() as usize)) {
            p.caught = None;
        }
        py.None()
    }

    // ---------------------------------------------------------------- results

    /// Settle every exception still being followed (call once the run is over).
    fn settle_all(&self) {
        let mut st = self.state();
        let keys: Vec<usize> = st.pending.keys().copied().collect();
        for key in keys {
            st.settle(key);
        }
    }

    /// Everything recorded, as plain Python objects (built after releasing the lock).
    fn results<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        type Swallow = (Py<PyAny>, Py<PyAny>, u32, Py<PyAny>, u32, u64, String);
        let (mine, calls, spent, settled) = {
            let st = self.state();
            let mine: Vec<(Py<PyAny>, bool)> = st.codes.values().map(|(c, m)| (c.clone_ref(py), *m)).collect();
            let calls: Vec<(Py<PyAny>, u64)> = st.calls.iter().map(|(k, n)| (st.object(py, *k), *n)).collect();
            let spent: Vec<(Py<PyAny>, f64)> = st.spent.iter().map(|(k, s)| (st.object(py, *k), *s)).collect();
            let settled: Vec<Swallow> = st
                .settled
                .iter()
                .map(|(&(t, rc, ro, cc, co), (n, msg))| {
                    (st.object(py, t), st.object(py, rc), ro, st.object(py, cc), co, *n, msg.clone())
                })
                .collect();
            (mine, calls, spent, settled)
        };
        let out = PyDict::new(py);
        out.set_item("mine", PyList::new(py, mine)?)?;
        out.set_item("calls", PyList::new(py, calls)?)?;
        out.set_item("spent", PyList::new(py, spent)?)?;
        out.set_item("settled", PyList::new(py, settled)?)?;
        Ok(out)
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Monitor>()
}
