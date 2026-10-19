//! Static side: parse Python source with tree-sitter and describe its functions and `except` blocks.

use indexmap::IndexMap;
use serde::Serialize;
use std::collections::{BTreeSet, HashSet};
use tree_sitter::{Node, Parser, Tree};

const BROAD: [&str; 2] = ["Exception", "BaseException"];
const TRACEBACK_CALLS: [&str; 6] =
    ["exception", "print_exc", "format_exc", "print_exception", "format_exception", "capture_exception"];
const LOG_CALLS: [&str; 8] = ["error", "warning", "warn", "info", "debug", "critical", "fatal", "log"];
const SCOPES: [&str; 4] = ["function_definition", "class_definition", "decorated_definition", "lambda"];
const CONSTANTS: [&str; 8] =
    ["string", "concatenated_string", "integer", "float", "true", "false", "none", "ellipsis"];
const DECISIONS: [&str; 11] = [
    "if_statement", "elif_clause", "conditional_expression", "for_statement", "while_statement",
    "except_clause", "assert_statement", "case_clause", "for_in_clause", "if_clause", "boolean_operator",
];

/// The words the report uses for each verdict.
pub fn verdict_text(verdict: &str) -> &'static str {
    match verdict {
        "ignored" => "silently ignored",
        "printed" => "only printed the message; traceback lost",
        "logged-message" => "logged the message only; traceback lost",
        "logged" => "logged with traceback",
        "fallback" => "took a fallback path",
        _ => "re-raised",
    }
}

/// One `except` clause.
#[derive(Clone, Debug, Serialize)]
pub struct Handler {
    pub line: usize,
    pub scope: String,      // qualified name of the enclosing function, or "<module>"
    pub catches: String,    // source text of the caught types; "" for a bare `except:`
    pub broad: bool,        // bare, `Exception` or `BaseException`
    pub verdict: &'static str,
    pub clause: String,
    pub hides_errors: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Function {
    pub qualname: String,
    pub line: usize,        // the `def` line
    pub first_line: usize,  // first decorator line, which is what code.co_firstlineno reports
    pub is_async: bool,
    pub args: Vec<String>,
    pub complexity: usize,  // McCabe cyclomatic complexity of the function's own body
    pub returns: &'static str,
    pub docstring: bool,
    pub decorators: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct FileReport {
    pub path: String,
    pub functions: Vec<Function>,
    pub handlers: Vec<Handler>,
    pub prints: Vec<(usize, String)>,                    // (line, scope)
    pub variables: IndexMap<String, BTreeSet<String>>,  // scope -> assigned names
    pub error: Option<String>,
}

pub struct Parsed {
    source: Vec<u8>,
    tree: Tree,
    pub report: FileReport,
}

/// Where a runtime exception was caught, read back from the source.
pub enum Site<'p> {
    Except(&'p Handler),
    With,
    Finally,
    Unknown,
}

pub fn analyze_file(path: &str) -> FileReport {
    parse(path).map(|p| p.report).unwrap_or_else(|error| FileReport {
        path: path.to_owned(),
        error: Some(error),
        ..Default::default()
    })
}

pub fn parse(path: &str) -> Result<Parsed, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let body = if bytes.starts_with(b"\xef\xbb\xbf") { bytes[3..].to_vec() } else { bytes };
    let source = String::from_utf8(body)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
        .into_bytes();
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_python::LANGUAGE.into()).map_err(|e| e.to_string())?;
    let tree = parser.parse(&source, None).ok_or("could not parse the file")?;
    let mut report = FileReport { path: path.to_owned(), ..Default::default() };
    let root = tree.root_node();
    if root.has_error() {
        report.error = Some(format!("syntax error at line {}", first_error_line(root)));
    } else {
        let mut c = Collector { src: &source, stack: vec![("module", "<module>".into())], ..Collector::default() };
        c.visit(root);
        (report.functions, report.handlers, report.prints, report.variables) =
            (c.functions, c.handlers, c.prints, c.variables);
    }
    Ok(Parsed { source, tree, report })
}

impl Parsed {
    /// Find the construct that caught an exception at `line`. Python reports the first clause of a
    /// try statement as the catch point even when a later clause matched, so the clause is picked
    /// by matching the exception's class names (`type_names`: every class in its MRO).
    pub fn catch_site(&self, line: usize, type_names: &HashSet<String>) -> Site<'_> {
        let mut innermost: Option<Node> = None;
        let mut stack = vec![self.tree.root_node()];
        while let Some(n) = stack.pop() {
            let (start, end) = (line_of(n), n.end_position().row + 1);
            if line < start || line > end {
                continue;
            }
            match n.kind() {
                "with_statement" if start == line => return Site::With,
                "try_statement" if innermost.is_none_or(|t| start > line_of(t)) => innermost = Some(n),
                _ => {}
            }
            stack.extend(children(n));
        }
        let Some(try_node) = innermost else { return Site::Unknown };
        let parts = named(try_node);
        let finally_start = parts
            .iter()
            .find(|p| p.kind() == "finally_clause")
            .and_then(|f| named(*f).into_iter().find(|b| b.kind() == "block"))
            .and_then(|b| named(b).first().map(|s| line_of(*s)));
        if finally_start.is_some_and(|f| line >= f) {
            return Site::Finally; // return/break/continue inside `finally` dropped it
        }
        let clauses: Vec<Node> = parts.into_iter().filter(|p| p.kind() == "except_clause").collect();
        let Some(first) = clauses.first() else { return Site::Unknown };
        let clause = clauses.iter().find(|c| self.matches(**c, type_names)).unwrap_or(first);
        match self.report.handlers.iter().find(|h| h.line == line_of(*clause)) {
            Some(h) => Site::Except(h),
            None => Site::Unknown,
        }
    }

    fn matches(&self, clause: Node, type_names: &HashSet<String>) -> bool {
        match caught_types(clause) {
            None => true,
            Some(expr) => identifiers(expr, &self.source).iter().any(|n| type_names.contains(n)),
        }
    }
}

// ---------------------------------------------------------------- collecting

#[derive(Default)]
struct Collector<'s> {
    src: &'s [u8],
    stack: Vec<(&'static str, String)>, // (kind, qualified name)
    functions: Vec<Function>,
    handlers: Vec<Handler>,
    prints: Vec<(usize, String)>,
    variables: IndexMap<String, BTreeSet<String>>,
}

impl Collector<'_> {
    fn scope(&self) -> String {
        self.stack.last().map(|(_, q)| q.clone()).unwrap_or_default()
    }

    fn qualname(&self, name: &str) -> String {
        match self.stack.last() {
            Some(("class", qual)) => format!("{qual}.{name}"),
            Some(("function", qual)) => format!("{qual}.<locals>.{name}"),
            _ => name.to_owned(),
        }
    }

    fn assign(&mut self, name: &str) {
        let scope = self.scope();
        self.variables.entry(scope).or_default().insert(name.to_owned());
    }

    fn field(&mut self, node: Node, name: &str) {
        if let Some(child) = node.child_by_field_name(name) {
            self.visit(child);
        }
    }

    fn visit_children(&mut self, node: Node) {
        for child in children(node) {
            self.visit(child);
        }
    }

    fn visit(&mut self, node: Node) {
        let src = self.src;
        match node.kind() {
            "decorated_definition" => {
                let decorators: Vec<Node> = named(node).into_iter().filter(|n| n.kind() == "decorator").collect();
                for d in &decorators {
                    self.visit_children(*d);
                }
                let texts = decorators.iter().filter_map(|d| named(*d).first().map(|e| text(*e, src).to_owned()));
                match node.child_by_field_name("definition") {
                    Some(def) if def.kind() == "function_definition" => self.function(def, texts.collect(), line_of(node)),
                    Some(def) => self.visit(def),
                    None => {}
                }
            }
            "function_definition" => self.function(node, Vec::new(), line_of(node)),
            "class_definition" => {
                self.field(node, "superclasses");
                let qual = self.qualname(node.child_by_field_name("name").map_or("", |n| text(n, src)));
                self.stack.push(("class", qual));
                self.field(node, "body");
                self.stack.pop();
            }
            "lambda" => self.field(node, "body"),
            "call" => {
                if node.child_by_field_name("function").is_some_and(|f| f.kind() == "identifier" && text(f, src) == "print") {
                    self.prints.push((line_of(node), self.scope()));
                }
                self.visit_children(node);
            }
            "except_clause" => {
                self.handlers.push(handler(node, &self.scope(), src));
                if let Some(alias) = node.child_by_field_name("value").and_then(|v| v.child_by_field_name("alias")) {
                    self.targets(alias);
                }
                self.visit_children(node);
            }
            "assignment" | "augmented_assignment" => {
                if let Some(left) = node.child_by_field_name("left") {
                    self.targets(left);
                }
                self.field(node, "right");
            }
            "for_statement" => {
                if let Some(left) = node.child_by_field_name("left") {
                    self.targets(left);
                }
                for f in ["right", "body", "alternative"] {
                    self.field(node, f);
                }
            }
            "with_item" => match node.child_by_field_name("value") {
                Some(v) if v.kind() == "as_pattern" => {
                    if let Some(alias) = v.child_by_field_name("alias") {
                        self.targets(alias);
                    }
                    if let Some(expr) = named(v).first() {
                        self.visit(*expr);
                    }
                }
                Some(v) => self.visit(v),
                None => {}
            },
            "named_expression" => {
                if let Some(n) = node.child_by_field_name("name") {
                    self.assign(text(n, src));
                }
                self.field(node, "value");
            }
            // a comprehension's loop variable lives in the comprehension's own scope
            "for_in_clause" => self.field(node, "right"),
            _ => self.visit_children(node),
        }
    }

    fn targets(&mut self, node: Node) {
        match node.kind() {
            "identifier" => self.assign(text(node, self.src)),
            "attribute" | "subscript" => self.visit(node),
            _ => {
                for child in named(node) {
                    self.targets(child);
                }
            }
        }
    }

    fn function(&mut self, node: Node, decorators: Vec<String>, first_line: usize) {
        let src = self.src;
        let qual = self.qualname(node.child_by_field_name("name").map_or("", |n| text(n, src)));
        let body = node.child_by_field_name("body");
        self.functions.push(Function {
            qualname: qual.clone(),
            line: line_of(node),
            first_line,
            is_async: children(node).iter().any(|c| c.kind() == "async"),
            args: node.child_by_field_name("parameters").map_or_else(Vec::new, |p| arg_names(p, src)),
            complexity: body.map_or(1, |b| 1 + walk_own(b).iter().filter(|n| DECISIONS.contains(&n.kind())).count()),
            returns: body.map_or("none", returns),
            docstring: body.is_some_and(has_docstring),
            decorators,
        });
        self.stack.push(("function", qual));
        if let Some(b) = body {
            self.visit(b);
        }
        self.stack.pop();
    }
}

// ---------------------------------------------------------------- except blocks

fn handler(node: Node, scope: &str, src: &[u8]) -> Handler {
    let types = caught_types(node);
    let catches = types.map(|t| text(t, src).to_owned()).unwrap_or_default();
    let broad = types.is_none_or(|t| identifiers(t, src).iter().any(|n| BROAD.contains(&n.as_str())));
    let verdict = named(node).into_iter().find(|n| n.kind() == "block").map_or("ignored", |b| classify(b, src));
    let star = if children(node).iter().any(|c| c.kind() == "*") { "*" } else { "" };
    Handler {
        line: line_of(node),
        scope: scope.to_owned(),
        clause: if catches.is_empty() { "except:".into() } else { format!("except{star} {catches}") },
        catches,
        broad,
        verdict,
        hides_errors: matches!(verdict, "ignored" | "printed" | "logged-message") || (verdict == "fallback" && broad),
    }
}

/// The type expression of an `except` clause (without its `as name`), or None for a bare `except:`.
fn caught_types(clause: Node) -> Option<Node> {
    let value = clause.child_by_field_name("value")?;
    if value.kind() == "as_pattern" { named(value).first().copied() } else { Some(value) }
}

fn classify(block: Node, src: &[u8]) -> &'static str {
    let own = walk_own(block);
    if own.iter().any(|n| n.kind() == "raise_statement") {
        return "reraised";
    }
    if named(block).iter().all(|s| is_trivial(*s)) {
        return "ignored";
    }
    let calls: Vec<Node> = own.into_iter().filter(|n| n.kind() == "call").collect();
    let names: HashSet<&str> = calls.iter().map(|c| call_name(*c, src)).collect();
    let with_traceback = calls.iter().any(|c| {
        c.child_by_field_name("arguments").is_some_and(|args| {
            named(args).iter().any(|k| {
                k.kind() == "keyword_argument"
                    && k.child_by_field_name("name").is_some_and(|n| text(n, src) == "exc_info")
                    && !k.child_by_field_name("value").is_some_and(|v| is_falsy(v, src))
            })
        })
    });
    if with_traceback || TRACEBACK_CALLS.iter().any(|n| names.contains(n)) {
        "logged"
    } else if LOG_CALLS.iter().any(|n| names.contains(n)) {
        "logged-message"
    } else if names.contains("print") || calls.iter().any(|c| writes_to_stream(*c, src)) {
        "printed"
    } else {
        "fallback"
    }
}

fn is_trivial(stmt: Node) -> bool {
    match stmt.kind() {
        "pass_statement" | "continue_statement" | "break_statement" => true,
        "expression_statement" => matches!(named(stmt).as_slice(), [only] if CONSTANTS.contains(&only.kind())),
        "return_statement" => matches!(named(stmt).as_slice(), [] | [_]) && named(stmt).iter().all(|v| v.kind() == "none"),
        _ => false,
    }
}

fn is_falsy(value: Node, src: &[u8]) -> bool {
    match value.kind() {
        "false" | "none" => true,
        "integer" | "float" => text(value, src).trim_start_matches(['0', '.']).is_empty(),
        "string" => matches!(text(value, src), "\"\"" | "''"),
        _ => false,
    }
}

fn call_name<'s>(call: Node, src: &'s [u8]) -> &'s str {
    match call.child_by_field_name("function") {
        Some(f) if f.kind() == "identifier" => text(f, src),
        Some(f) if f.kind() == "attribute" => f.child_by_field_name("attribute").map_or("", |a| text(a, src)),
        _ => "",
    }
}

fn writes_to_stream(call: Node, src: &[u8]) -> bool {
    let Some(f) = call.child_by_field_name("function").filter(|f| f.kind() == "attribute") else { return false };
    f.child_by_field_name("attribute").is_some_and(|a| text(a, src) == "write")
        && f.child_by_field_name("object").is_some_and(|o| {
            o.kind() == "attribute"
                && o.child_by_field_name("attribute").is_some_and(|a| matches!(text(a, src), "stdout" | "stderr"))
        })
}

// ---------------------------------------------------------------- small helpers

fn returns(body: Node) -> &'static str {
    let own = walk_own(body);
    if own.iter().any(|n| n.kind() == "yield") {
        "yields"
    } else if own.iter().any(|n| n.kind() == "return_statement" && !named(*n).is_empty()) {
        "value"
    } else {
        "none"
    }
}

fn has_docstring(body: Node) -> bool {
    named(body).first().is_some_and(|s| {
        s.kind() == "expression_statement"
            && matches!(named(*s).as_slice(), [only] if matches!(only.kind(), "string" | "concatenated_string"))
    })
}

fn arg_names(params: Node, src: &[u8]) -> Vec<String> {
    let splat = |n: Node| -> String {
        let inner = named(n).first().map_or("", |i| text(*i, src));
        match n.kind() {
            "list_splat_pattern" => format!("*{inner}"),
            "dictionary_splat_pattern" => format!("**{inner}"),
            _ => text(n, src).to_owned(),
        }
    };
    let mut names = Vec::new();
    for p in named(params) {
        match p.kind() {
            "identifier" => names.push(text(p, src).to_owned()),
            "typed_parameter" => names.extend(named(p).first().map(|first| splat(*first))),
            "default_parameter" | "typed_default_parameter" => {
                names.extend(p.child_by_field_name("name").map(|n| text(n, src).to_owned()))
            }
            "list_splat_pattern" | "dictionary_splat_pattern" => names.push(splat(p)),
            "keyword_separator" => names.push("*".into()),
            _ => {} // the positional-only marker `/`
        }
    }
    names
}

fn identifiers(expr: Node, src: &[u8]) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![expr];
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier" {
            found.push(text(n, src).to_owned());
        }
        stack.extend(children(n));
    }
    found
}

/// Every node below `node`, without entering nested functions, classes or lambdas.
fn walk_own(node: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut stack = children(node);
    while let Some(n) = stack.pop() {
        if !SCOPES.contains(&n.kind()) {
            stack.extend(children(n));
        }
        out.push(n);
    }
    out
}

fn first_error_line(root: Node) -> usize {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.is_error() || n.is_missing() {
            return line_of(n);
        }
        stack.extend(children(n).into_iter().rev());
    }
    1
}

fn children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

fn named(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).filter(|n| n.kind() != "comment").collect()
}

fn text<'s>(node: Node, src: &'s [u8]) -> &'s str {
    node.utf8_text(src).unwrap_or("")
}

fn line_of(node: Node) -> usize {
    node.start_position().row + 1
}
