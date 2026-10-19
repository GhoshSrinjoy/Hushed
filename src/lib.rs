//! hushed: run a Python script unchanged and see what actually happened, including the
//! errors it swallowed. Everything is here in Rust; the Python package only checks the
//! interpreter version and calls `main`.

mod cli;
mod monitor;
mod paths;
mod report;
mod source;
mod tracer;

use pyo3::prelude::*;

/// The command line, called by `python -m hushed` and the `hushed` command.
#[pyfunction]
fn main(py: Python<'_>, argv: Vec<String>) -> PyResult<i32> {
    cli::main(py, argv)
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(main, m)?)?;
    m.add_class::<monitor::Monitor>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
