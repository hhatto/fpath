extern crate memchr;

use std::{env, str};
use std::borrow::Cow;
use std::collections::HashMap;
use std::env::current_dir;
use std::path::{Path, MAIN_SEPARATOR};
use pyo3::prelude::*;
use pyo3::call::PyCallArgs;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyBytes, PyDict, PyString, PyTuple};
use pyo3::exceptions;
use uzers::os::unix::UserExt;
use uzers::{get_user_by_uid, get_user_by_name, get_current_uid};

#[macro_use]
mod utils;

const SEP: u8 = MAIN_SEPARATOR as u8;

static OS_FSPATH: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static OS_PATH: PyOnceLock<Py<PyModule>> = PyOnceLock::new();

fn os_path_module(py: Python<'_>) -> PyResult<&Bound<'_, PyModule>> {
    let ospath = OS_PATH.get_or_try_init(py, || py.import("os.path").map(Bound::unbind))?;
    Ok(ospath.bind(py))
}

/// Calls `os.path.<name>(*args)`.
///
/// fpath handles a path only if it is an exact str without lone surrogates or
/// an exact bytes that is valid UTF-8. Everything else, e.g. a str with lone
/// surrogates, non-UTF-8 bytes, subclasses of str and bytes, and invalid
/// arguments, is delegated to os.path so that the result or the error is the
/// same as os.path.
fn os_path<'py>(py: Python<'py>, name: &str, args: impl PyCallArgs<'py>) -> PyResult<Py<PyAny>> {
    Ok(os_path_module(py)?.call_method1(name, args)?.unbind())
}

/// `os.fspath(obj)`, skipping the call for exact str and bytes.
fn fspath<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if obj.is_exact_instance_of::<PyString>() || obj.is_exact_instance_of::<PyBytes>() {
        return Ok(obj.clone());
    }
    OS_FSPATH.import(obj.py(), "os", "fspath")?.call1((obj,))
}

/// Returns the contents of `path` and whether it is bytes, if fpath handles
/// it (see `os_path()`).
fn utf8_path<'a>(path: &'a Bound<'_, PyAny>) -> Option<(&'a str, bool)> {
    if let Ok(s) = path.cast_exact::<PyString>() {
        // to_str() fails for a str with lone surrogates
        s.to_str().ok().map(|s| (s, false))
    } else if let Ok(b) = path.cast_exact::<PyBytes>() {
        str::from_utf8(b.as_bytes()).ok().map(|s| (s, true))
    } else {
        None
    }
}

/// Calls `f` with `os.fspath(path)`, its contents and whether it is bytes, if
/// fpath handles the path. Returns None if fpath does not handle it, if
/// os.fspath() raises (os.path then raises its own error), or if `f` returns
/// None. The caller then delegates to os.path.
fn with_path<'py, R>(
    path: &Bound<'py, PyAny>,
    f: impl FnOnce(&Bound<'py, PyAny>, &str, bool) -> Option<R>,
) -> Option<R> {
    let path = fspath(path).ok()?;
    let (path_str, is_bytes) = utf8_path(&path)?;
    f(&path, path_str, is_bytes)
}


#[inline(always)]
fn _islink(path_str: &str) -> bool {
    std::fs::read_link(path_str).is_ok()
}

/// Mode selected by the `strict` argument of `realpath()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RealpathMode {
    /// `strict=False`: every `OSError` is ignored.
    NonStrict,
    /// `strict=True`: no error is ignored.
    Strict,
    /// `strict=os.path.ALLOW_MISSING`: only `FileNotFoundError` is ignored.
    AllowMissing,
    /// `strict=os.path.ALL_BUT_LAST`: `FileNotFoundError` is ignored only for
    /// the last path component.
    AllButLast,
}

impl RealpathMode {
    /// Whether an `OSError` with `errno` is ignored. The "last component only"
    /// rule of `ALL_BUT_LAST` is checked by the caller.
    #[inline]
    fn ignores(self, errno: i32) -> bool {
        match self {
            RealpathMode::NonStrict => true,
            RealpathMode::Strict => false,
            RealpathMode::AllowMissing | RealpathMode::AllButLast => errno == libc::ENOENT,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RealpathError {
    /// Raised as `OSError(errno, os.strerror(errno), filename)`.
    Os { errno: i32, filename: Option<String> },
    /// The caller falls back to `os.path.realpath()`, which raises
    /// `ValueError` with the message of `os.lstat()`.
    EmbeddedNul,
    /// A path that can not be handled as UTF-8; the caller falls back to
    /// `os.path.realpath()`.
    NonUtf8,
}

impl RealpathError {
    fn os(errno: i32, filename: &str) -> Self {
        RealpathError::Os { errno, filename: Some(filename.to_string()) }
    }

    fn from_io(e: std::io::Error, filename: Option<&str>) -> Self {
        RealpathError::Os {
            errno: e.raw_os_error().unwrap_or(libc::EIO),
            filename: filename.map(str::to_string),
        }
    }
}

/// An entry of the stack of unresolved path parts in `_realpath()`.
enum RealpathPart<'a> {
    /// A path component that is not processed yet.
    Name(Cow<'a, str>),
    /// Marks that the target of the symlink at this path has been resolved.
    ResolvedLink(String),
}

/// What the path component being processed turned out to be.
enum RealpathStep {
    /// Not a symlink, or a symlink loop in non-strict mode.
    Plain,
    /// A symlink whose fully resolved path is already known.
    Cached(String),
    /// A symlink to resolve, with its target.
    Link(String),
}

/// Port of `posixpath.realpath()` in CPython 3.15.
fn _realpath(filename: &str, mode: RealpathMode) -> Result<String, RealpathError> {
    let strict = mode != RealpathMode::NonStrict;

    // The stack of unresolved path parts, in reverse order.
    let mut rest: Vec<RealpathPart> = filename
        .trim_end_matches(MAIN_SEPARATOR)
        .rsplit(MAIN_SEPARATOR)
        .map(|name| RealpathPart::Name(Cow::Borrowed(name)))
        .collect();

    // Number of unprocessed parts in `rest`. This can differ from rest.len()
    // later, because `rest` might contain markers for unresolved symlinks.
    let mut part_count = rest.len();

    // The resolved path, which is absolute throughout this function.
    // Note: getcwd() returns a normalized and symlink-free path.
    let mut path = if _isabs(filename) {
        MAIN_SEPARATOR.to_string()
    } else {
        current_dir()
            .map_err(|e| RealpathError::from_io(e, None))?
            .into_os_string()
            .into_string()
            .map_err(|_| RealpathError::NonUtf8)?
    };
    let trailing_sep = filename.ends_with(MAIN_SEPARATOR);

    // Mapping from symlink paths to *fully resolved* symlink targets. If a
    // symlink is encountered but not yet resolved, the value is None. This is
    // used both to detect symlink loops and to speed up repeated traversals of
    // the same links.
    let mut seen: HashMap<String, Option<String>> = HashMap::new();

    while part_count > 0 {
        let name = match rest.pop() {
            Some(RealpathPart::Name(name)) => name,
            Some(RealpathPart::ResolvedLink(link)) => {
                seen.insert(link, Some(path.clone()));
                continue;
            }
            None => break,
        };
        part_count -= 1;
        if name.is_empty() || name == "." {
            // current dir
            continue;
        }
        if name == ".." {
            // parent dir; `path` is absolute, so it always contains a separator.
            let i = path.rfind(MAIN_SEPARATOR).unwrap_or(0);
            path.truncate(i);
            if path.is_empty() {
                path.push(MAIN_SEPARATOR);
            }
            continue;
        }
        if memchr::memchr(0, name.as_bytes()).is_some() {
            return Err(RealpathError::EmbeddedNul);
        }
        let mut newpath = String::with_capacity(path.len() + 1 + name.len());
        newpath.push_str(&path);
        if path.len() != 1 {
            // `path` is not the root directory
            newpath.push(MAIN_SEPARATOR);
        }
        newpath.push_str(&name);

        let step = match std::fs::symlink_metadata(&newpath) {
            Err(e) => Err(RealpathError::from_io(e, Some(&newpath))),
            Ok(meta) if !meta.file_type().is_symlink() => {
                if strict && (part_count > 0 || trailing_sep) && !meta.is_dir() {
                    Err(RealpathError::os(libc::ENOTDIR, &newpath))
                } else {
                    Ok(RealpathStep::Plain)
                }
            }
            Ok(_) => match seen.get(&newpath) {
                // use cached value
                Some(Some(resolved)) => Ok(RealpathStep::Cached(resolved.clone())),
                // The symlink is not resolved, so we must have a symlink loop.
                Some(None) if strict => Err(RealpathError::os(libc::ELOOP, &newpath)),
                Some(None) => Ok(RealpathStep::Plain),
                None => match std::fs::read_link(&newpath) {
                    Ok(target) => Ok(RealpathStep::Link(
                        target.into_os_string().into_string().map_err(|_| RealpathError::NonUtf8)?,
                    )),
                    Err(e) => Err(RealpathError::from_io(e, Some(&newpath))),
                },
            },
        };

        match step {
            Ok(RealpathStep::Plain) => path = newpath,
            Ok(RealpathStep::Cached(resolved)) => path = resolved,
            Ok(RealpathStep::Link(target)) => {
                if _isabs(&target) {
                    // Symlink target is absolute; reset resolved path.
                    path = MAIN_SEPARATOR.to_string();
                }
                // Mark this symlink as seen but not fully resolved, and push a
                // marker to record its resolved path once the target parts
                // below it have been processed.
                seen.insert(newpath.clone(), None);
                rest.push(RealpathPart::ResolvedLink(newpath));
                // Push the unresolved symlink target parts onto the stack.
                let len = rest.len();
                rest.extend(
                    target
                        .rsplit(MAIN_SEPARATOR)
                        .map(|name| RealpathPart::Name(Cow::Owned(name.to_string()))),
                );
                part_count += rest.len() - len;
            }
            Err(RealpathError::Os { errno, filename }) => {
                if !mode.ignores(errno) || (mode == RealpathMode::AllButLast && part_count > 0) {
                    return Err(RealpathError::Os { errno, filename });
                }
                // An error occurred and was ignored.
                path = newpath;
            }
            Err(e) => return Err(e),
        }
    }

    Ok(path)
}

static ALLOW_MISSING: PyOnceLock<Option<Py<PyAny>>> = PyOnceLock::new();
static ALL_BUT_LAST: PyOnceLock<Option<Py<PyAny>>> = PyOnceLock::new();

/// Returns `os.path.<name>`, or None if the running Python does not have it.
fn os_path_sentinel<'a>(
    py: Python<'_>,
    cell: &'a PyOnceLock<Option<Py<PyAny>>>,
    name: &str,
) -> PyResult<Option<&'a Py<PyAny>>> {
    let value = cell.get_or_try_init(py, || -> PyResult<Option<Py<PyAny>>> {
        let ospath = py.import("os")?.getattr("path")?;
        Ok(ospath.getattr(name).ok().map(Bound::unbind))
    })?;
    Ok(value.as_ref())
}

fn realpath_mode(py: Python<'_>, strict: Option<&Bound<'_, PyAny>>) -> PyResult<RealpathMode> {
    let strict = match strict {
        Some(strict) => strict,
        None => return Ok(RealpathMode::NonStrict),
    };
    if let Some(sentinel) = os_path_sentinel(py, &ALLOW_MISSING, "ALLOW_MISSING")?
        && strict.is(sentinel)
    {
        return Ok(RealpathMode::AllowMissing);
    }
    if let Some(sentinel) = os_path_sentinel(py, &ALL_BUT_LAST, "ALL_BUT_LAST")?
        && strict.is(sentinel)
    {
        return Ok(RealpathMode::AllButLast);
    }
    if strict.is_truthy()? {
        Ok(RealpathMode::Strict)
    } else {
        Ok(RealpathMode::NonStrict)
    }
}

/// Delegates to `os.path.realpath()`.
fn realpath_fallback(
    py: Python<'_>,
    filename: &Bound<'_, PyAny>,
    strict: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let kwargs = PyDict::new(py);
    if let Some(strict) = strict {
        kwargs.set_item("strict", strict)?;
    }
    Ok(os_path_module(py)?.call_method("realpath", (filename,), Some(&kwargs))?.unbind())
}

/// Creates `OSError(errno, os.strerror(errno)[, filename])`.
fn realpath_error(py: Python<'_>, errno: i32, filename: Option<String>, is_bytes: bool) -> PyErr {
    let new_oserror = || -> PyResult<PyErr> {
        // OSError(errno, ...) creates the subclass for the errno,
        // e.g. FileNotFoundError for ENOENT.
        let oserror = py.get_type::<exceptions::PyOSError>();
        let strerror = py.import("os")?.call_method1("strerror", (errno,))?;
        let exc = match filename {
            Some(f) if is_bytes => oserror.call1((errno, strerror, PyBytes::new(py, f.as_bytes())))?,
            Some(f) => oserror.call1((errno, strerror, PyString::new(py, &f)))?,
            None => oserror.call1((errno, strerror))?,
        };
        Ok(PyErr::from_value(exc))
    };
    new_oserror().unwrap_or_else(|e| e)
}

/// Appends `b` to `ret_path` as `posixpath.join()` does.
fn join_push(ret_path: &mut String, b: &str) {
    if b.starts_with(MAIN_SEPARATOR) {
        ret_path.clear();
        ret_path.push_str(b);
    } else if ret_path.is_empty() || ret_path.ends_with(MAIN_SEPARATOR) {
        ret_path.push_str(b);
    } else {
        ret_path.push(MAIN_SEPARATOR);
        ret_path.push_str(b);
    }
}

pub fn _inner_join(path_str: &str, path_list: &[&str]) -> String {
    let mut ret_path = String::from(path_str);
    for b in path_list {
        join_push(&mut ret_path, b);
    }
    ret_path
}

/// Returns None when `os.path.abspath()` has to be used instead: when the
/// current directory can not be got (os.getcwd() then raises the error) or is
/// not UTF-8.
fn _abspath(path_str: &str) -> Option<String> {
    if _isabs(path_str) {
        return Some(_normpath(path_str));
    }
    let path = current_dir().ok()?.join(path_str).into_os_string().into_string().ok()?;
    Some(_normpath(&path))
}

fn _basename(py: Python<'_>, path_str: &str, is_bytes: bool) -> Py<PyAny> {
    let i = match memchr::memrchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => v + 1,
        None => 0,
    };
    if is_bytes {
        PyBytes::new(py, path_str.split_at(i).1.as_bytes()).into_any().unbind()
    } else {
        PyString::new(py, path_str.split_at(i).1).into_any().unbind()
    }
}

fn _dirname<'a>(path_str: &'a str) -> &'a str {
    let i = match memchr::memrchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => v + 1,
        None => 0,
    };
    let (head, _) = path_str.split_at(i);
    let head_sep = numsep!(head.len());
    if !head.is_empty() && head != head_sep {
        head.trim_end_matches(MAIN_SEPARATOR)
    } else {
        head
    }
}

fn _exists(path_str: &str) -> bool {
    Path::new(path_str).exists()
}

/// Expands a path that starts with `~`. Returns None when
/// `os.path.expanduser()` has to be used instead: when the user is not found
/// (it then returns the path unchanged) or the home directory is not UTF-8.
fn _expanduser(path_str: &str) -> Option<String> {
    let i = memchr::memchr(SEP, path_str.as_bytes()).unwrap_or(path_str.len());
    let userhome = if i == 1 {
        match env::var_os("HOME") {
            Some(home) => home.into_string().ok()?,
            None => get_user_by_uid(get_current_uid())?.home_dir().to_str()?.to_string(),
        }
    } else {
        get_user_by_name(&path_str[1..i])?.home_dir().to_str()?.to_string()
    };

    let mut ret = userhome.trim_end_matches(MAIN_SEPARATOR).to_string();
    ret.push_str(&path_str[i..]);
    if ret.is_empty() {
        ret.push(MAIN_SEPARATOR);
    }
    Some(ret)
}

/// Returns the value of the environment variable `name`, or Some(None) if it
/// is not set. Returns None if the value is not UTF-8.
fn getenv(name: &str) -> Option<Option<String>> {
    // os.environ never has such names, but getenv() might find a variable for
    // a name with "=".
    if name.is_empty() || name.contains(['=', '\0']) {
        return Some(None);
    }
    match env::var_os(name) {
        Some(value) => value.into_string().ok().map(Some),
        None => Some(None),
    }
}

/// Expands `$name` and `${name}` like `posixpath.expandvars()`: `name` of
/// `$name` consists of ASCII letters, digits and `_`, and unknown variables are
/// left unchanged. Returns None when `os.path.expandvars()` has to be used
/// instead: for an unclosed `${`, which Python 3.14 handles differently from
/// older versions, and for a value that is not UTF-8.
fn _expandvars(path_str: &str) -> Option<String> {
    let mut ret = String::with_capacity(path_str.len());
    // `path_str[copied..]` is not copied to `ret` yet.
    let mut copied = 0;
    let mut i = 0;
    while let Some(n) = memchr::memchr(b'$', &path_str.as_bytes()[i..]) {
        let start = i + n;
        let rest = &path_str[start + 1..];
        let (name, end) = match rest.strip_prefix('{') {
            Some(braced) => {
                let len = braced.find('}')?;
                // "${" + name + "}"
                (&braced[..len], start + len + 3)
            }
            None => {
                let len = rest.bytes().take_while(|&c| c.is_ascii_alphanumeric() || c == b'_').count();
                if len == 0 {
                    i = start + 1;
                    continue;
                }
                // "$" + name
                (&rest[..len], start + len + 1)
            }
        };
        if let Some(value) = getenv(name)? {
            ret.push_str(&path_str[copied..start]);
            ret.push_str(&value);
            copied = end;
        }
        i = end;
    }
    ret.push_str(&path_str[copied..]);
    Some(ret)
}

#[inline(always)]
fn _isabs(path_str: &str) -> bool {
    path_str.starts_with(MAIN_SEPARATOR)
}

/// `posixpath.join(a, *p)` for the paths that fpath handles. Returns None
/// otherwise.
fn join_fast(py: Python<'_>, a: &Bound<'_, PyAny>, p: &Bound<'_, PyTuple>) -> Option<PyResult<Py<PyAny>>> {
    with_path(a, |a, a_str, is_bytes| {
        if p.is_empty() {
            return Some(Ok(a.clone().unbind()));
        }
        // Append each part directly; collecting the parts into Vecs first made
        // join() noticeably slower.
        let mut ret = String::with_capacity(a_str.len() + 32);
        ret.push_str(a_str);
        for b in p.iter() {
            let b = fspath(&b).ok()?;
            let (b, b_is_bytes) = utf8_path(&b)?;
            if b_is_bytes != is_bytes {
                return None;
            }
            join_push(&mut ret, b);
        }
        Some(str2pyobj!(py, ret.as_str(), is_bytes))
    })
}

fn _normpath(path_str: &str) -> String {
    if path_str.is_empty() {
        return ".".to_string();
    }
    let initial_slashes = path_str.starts_with(MAIN_SEPARATOR);
    let initial_slashes_str = if initial_slashes && path_str.starts_with("//") && !path_str.starts_with("///") {
        "//"
    } else {
        "/"
    };
    let mut new_comps: Vec<&str> = vec![];
    for comp in path_str.split('/').into_iter() {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp != ".." || (!initial_slashes && new_comps.len() == 0)
            || (!new_comps.is_empty() && *new_comps.last().unwrap() == "..")
        {
            new_comps.push(comp);
        } else if !new_comps.is_empty() {
            new_comps.pop();
        }
    }

    let new_comps_path = new_comps.join("/");
    if initial_slashes {
        let mut head_sep = initial_slashes_str.to_string();
        head_sep.push_str(new_comps_path.as_str());
        head_sep
    } else if new_comps_path.is_empty() {
        ".".to_string()
    } else {
        new_comps_path
    }
}

fn _commonprefix(m: &Vec<&[String]>) -> Result<Vec<String>, String> {
    let s1 = m.into_iter().min().unwrap();
    let s2 = m.into_iter().max().unwrap();
    for (i, c) in s1.iter().enumerate() {
        if c != &s2[i] {
            return Ok(s1[..i].iter().map(|x| x.to_string()).collect());
        }
    }
    Ok(s1.iter().map(|x| x.to_string()).collect())
}

/// Path components used by `commonpath()`: empty and "." components are dropped.
fn commonpath_components(path: &[u8]) -> impl Iterator<Item = &[u8]> {
    path.split(|&c| c == b'/').filter(|c| !c.is_empty() && *c != b".")
}

/// Returns the longest common sub-path of `paths`. Returns None if `paths` is
/// empty or mixes absolute and relative paths.
///
/// This works on bytes, so it handles both bytes and (UTF-8 encoded) str paths.
fn _commonpath(paths: &[&[u8]]) -> Option<Vec<u8>> {
    let (first, rest) = paths.split_first()?;
    let isabs = first.starts_with(b"/");
    if rest.iter().any(|path| path.starts_with(b"/") != isabs) {
        return None;
    }

    let mut common: Vec<&[u8]> = commonpath_components(first).collect();
    for path in rest {
        if common.is_empty() {
            break;
        }
        let n = commonpath_components(path)
            .zip(&common)
            .take_while(|(c, common_c)| c == *common_c)
            .count();
        common.truncate(n);
    }

    let mut ret = Vec::new();
    if isabs {
        ret.push(b'/');
    }
    for (i, c) in common.iter().enumerate() {
        if i > 0 {
            ret.push(b'/');
        }
        ret.extend_from_slice(c);
    }
    Some(ret)
}

/// Delegates to `os.path.commonpath()`, e.g. to raise the same errors.
fn commonpath_fallback(py: Python<'_>, paths: &[Bound<'_, PyAny>]) -> PyResult<Py<PyAny>> {
    os_path(py, "commonpath", (PyTuple::new(py, paths)?,))
}

/// Returns None when `os.path.relpath()` has to be used instead (see
/// `_abspath()`).
fn _relpath(path_str: &str, start: &str) -> Option<String> {
    let start_abs = _abspath(start)?;
    let path_abs = _abspath(path_str)?;
    let start_list: Vec<String> = start_abs
        .split(MAIN_SEPARATOR)
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect();
    let path_list: Vec<String> = path_abs
        .split(MAIN_SEPARATOR)
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect();
    let cprefix = _commonprefix(&vec![start_list.as_slice(), path_list.as_slice()]).unwrap();
    let i = cprefix.len();
    let num = start_list.len() - i;
    let plist_list: Vec<&str> = path_list[i..].iter().map(|x| x.as_str()).collect();
    let rel_list: Vec<&str> = (0..num).map(|_| "..").chain(plist_list).collect();
    if rel_list.len() == 0 {
        return Some(".".to_string());
    }
    Some(_inner_join(rel_list[0], &rel_list[1..]))
}

fn _split<'a>(path_str: &'a str) -> (&'a str, &'a str) {
    let (mut head, tail) = match memchr::memrchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => path_str.split_at(v + 1),
        None => ("", path_str),
    };
    let head_sep = numsep!(head.len());
    if !head.is_empty() && head != head_sep {
        head = head.trim_end_matches(MAIN_SEPARATOR);
    }
    (head, tail)
}

fn _splitext<'a>(path_str: &'a str) -> (&'a str, &'a str) {
    let bytes = path_str.as_bytes();
    let name_start = memchr::memrchr(SEP, bytes).map_or(0, |i| i + 1);
    let name = &bytes[name_start..];
    match memchr::memrchr(b'.', name) {
        // Leading dots of the file name do not start an extension.
        Some(dot) if name[..dot].iter().any(|&c| c != b'.') => path_str.split_at(name_start + dot),
        _ => (path_str, ""),
    }
}

#[pymodule]
#[pyo3(name = "_fpath")]
fn init_mod(m: &Bound<'_, PyModule>) -> PyResult<()> {

    #[pyfunction]
    #[pyo3(name = "abspath")]
    pub fn abspath(py: Python<'_>, path: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        with_path(path, |_, path_str, is_bytes| {
            let ret = _abspath(path_str)?;
            Some(str2pyobj!(py, ret.as_str(), is_bytes))
        })
        .unwrap_or_else(|| os_path(py, "abspath", (path,)))
    }

    #[pyfunction]
    #[pyo3(name = "basename")]
    pub fn basename(py: Python<'_>, p: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        with_path(p, |_, path_str, is_bytes| Some(Ok(_basename(py, path_str, is_bytes))))
            .unwrap_or_else(|| os_path(py, "basename", (p,)))
    }

    #[pyfunction]
    #[pyo3(name = "commonpath")]
    pub fn commonpath(py: Python<'_>, paths: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        // paths = tuple(map(os.fspath, paths))
        // A plain loop: collect::<PyResult<Vec<_>>>() was measurably slower.
        let mut items = Vec::new();
        for item in paths.try_iter()? {
            items.push(fspath(&item?)?);
        }

        // Only handle paths that are all exact str (encodable to UTF-8) or
        // all exact bytes. Everything else, including the error cases, is
        // delegated to os.path.
        let is_bytes = match items.first() {
            Some(first) => first.is_exact_instance_of::<PyBytes>(),
            None => return commonpath_fallback(py, &items),
        };
        let parts: Option<Vec<&[u8]>> = items
            .iter()
            .map(|item| {
                if is_bytes {
                    item.cast_exact::<PyBytes>().ok().map(|b| b.as_bytes())
                } else {
                    // to_str() fails for a str with lone surrogates
                    item.cast_exact::<PyString>().ok().and_then(|s| s.to_str().ok()).map(str::as_bytes)
                }
            })
            .collect();
        let ret = match parts.as_deref().and_then(_commonpath) {
            Some(ret) => ret,
            None => return commonpath_fallback(py, &items),
        };
        if is_bytes {
            Ok(PyBytes::new(py, &ret).into_any().unbind())
        } else {
            Ok(PyString::new(py, &String::from_utf8(ret)?).into_any().unbind())
        }
    }

    #[pyfunction]
    #[pyo3(name = "dirname")]
    pub fn dirname(py: Python<'_>, p: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        with_path(p, |_, path_str, is_bytes| Some(str2pyobj!(py, _dirname(path_str), is_bytes)))
            .unwrap_or_else(|| os_path(py, "dirname", (p,)))
    }

    #[pyfunction]
    #[pyo3(name = "exists")]
    pub fn exists(py: Python<'_>, path: &Bound<'_, PyAny>) -> PyResult<bool> {
        match with_path(path, |_, path_str, _| Some(_exists(path_str))) {
            Some(ret) => Ok(ret),
            // e.g. a file descriptor
            None => os_path(py, "exists", (path,))?.bind(py).is_truthy(),
        }
    }

    #[pyfunction]
    #[pyo3(name = "expanduser")]
    pub fn expanduser(py: Python<'_>, path: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        with_path(path, |path, path_str, is_bytes| {
            if !path_str.starts_with('~') {
                return Some(Ok(path.clone().unbind()));
            }
            let ret = _expanduser(path_str)?;
            Some(str2pyobj!(py, ret.as_str(), is_bytes))
        })
        .unwrap_or_else(|| os_path(py, "expanduser", (path,)))
    }

    #[pyfunction]
    #[pyo3(name = "expandvars")]
    pub fn expandvars(py: Python<'_>, path: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        with_path(path, |path, path_str, is_bytes| {
            if memchr::memchr(b'$', path_str.as_bytes()).is_none() {
                return Some(Ok(path.clone().unbind()));
            }
            let ret = _expandvars(path_str)?;
            Some(str2pyobj!(py, ret.as_str(), is_bytes))
        })
        .unwrap_or_else(|| os_path(py, "expandvars", (path,)))
    }

    #[pyfunction]
    #[pyo3(name = "isabs")]
    pub fn isabs(py: Python<'_>, s: &Bound<'_, PyAny>) -> PyResult<bool> {
        match with_path(s, |_, path_str, _| Some(_isabs(path_str))) {
            Some(ret) => Ok(ret),
            None => os_path(py, "isabs", (s,))?.bind(py).is_truthy(),
        }
    }

    #[pyfunction]
    #[pyo3(name = "islink")]
    pub fn islink(py: Python<'_>, path: &Bound<'_, PyAny>) -> PyResult<bool> {
        match with_path(path, |_, path_str, _| Some(_islink(path_str))) {
            Some(ret) => Ok(ret),
            None => os_path(py, "islink", (path,))?.bind(py).is_truthy(),
        }
    }

    /// Called as `join(a, p)` by `fpath.join(a, *p)`.
    #[pyfunction]
    #[pyo3(name = "join")]
    pub fn join(py: Python<'_>, a: &Bound<'_, PyAny>, p: &Bound<'_, PyTuple>) -> PyResult<Py<PyAny>> {
        if let Some(ret) = join_fast(py, a, p) {
            return ret;
        }
        let args: Vec<_> = std::iter::once(a.clone()).chain(p.iter()).collect();
        let args = PyTuple::new(py, args)?;
        os_path(py, "join", args)
    }

    #[pyfunction]
    #[pyo3(name = "relpath", signature = (path, start=None))]
    pub fn relpath(py: Python<'_>, path: &Bound<'_, PyAny>, start: Option<&Bound<'_, PyAny>>) -> PyResult<Py<PyAny>> {
        let ret = with_path(path, |_, path_str, is_bytes| {
            if path_str.is_empty() {
                // os.path.relpath() raises ValueError
                return None;
            }
            let start = match start {
                Some(start) => Some(fspath(start).ok()?),
                None => None,
            };
            let start_str = match &start {
                Some(start) => match utf8_path(start)? {
                    (start_str, start_is_bytes) if start_is_bytes == is_bytes => start_str,
                    _ => return None,
                },
                None => ".",
            };
            let ret = _relpath(path_str, start_str)?;
            Some(str2pyobj!(py, ret.as_str(), is_bytes))
        });
        ret.unwrap_or_else(|| os_path(py, "relpath", (path, start)))
    }

    #[pyfunction]
    #[pyo3(
        name = "realpath",
        signature = (filename, *, strict=None),
        text_signature = "(filename, *, strict=False)"
    )]
    pub fn realpath(py: Python<'_>, filename: &Bound<'_, PyAny>, strict: Option<&Bound<'_, PyAny>>) -> PyResult<Py<PyAny>> {
        let mode = realpath_mode(py, strict)?;
        let ret = with_path(filename, |_, path_str, is_bytes| match _realpath(path_str, mode) {
            Ok(ret) => Some(str2pyobj!(py, ret.as_str(), is_bytes)),
            Err(RealpathError::Os { errno, filename }) => Some(Err(realpath_error(py, errno, filename, is_bytes))),
            Err(RealpathError::EmbeddedNul | RealpathError::NonUtf8) => None,
        });
        ret.unwrap_or_else(|| realpath_fallback(py, filename, strict))
    }

    #[pyfunction]
    #[pyo3(name = "split")]
    pub fn split(py: Python<'_>, p: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        with_path(p, |_, path_str, is_bytes| {
            let (head, tail) = _split(path_str);
            Some(tuplestr2pyobj!(py, head, tail, is_bytes))
        })
        .unwrap_or_else(|| os_path(py, "split", (p,)))
    }

    #[pyfunction]
    #[pyo3(name = "splitext")]
    pub fn splitext(py: Python<'_>, p: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        with_path(p, |_, path_str, is_bytes| {
            let (root, ext) = _splitext(path_str);
            Some(tuplestr2pyobj!(py, root, ext, is_bytes))
        })
        .unwrap_or_else(|| os_path(py, "splitext", (p,)))
    }

    m.add_function(wrap_pyfunction!(abspath, m)?)?;
    m.add_function(wrap_pyfunction!(basename, m)?)?;
    m.add_function(wrap_pyfunction!(commonpath, m)?)?;
    m.add_function(wrap_pyfunction!(dirname, m)?)?;
    m.add_function(wrap_pyfunction!(exists, m)?)?;
    m.add_function(wrap_pyfunction!(expanduser, m)?)?;
    m.add_function(wrap_pyfunction!(expandvars, m)?)?;
    m.add_function(wrap_pyfunction!(isabs, m)?)?;
    m.add_function(wrap_pyfunction!(islink, m)?)?;
    m.add_function(wrap_pyfunction!(join, m)?)?;
    m.add_function(wrap_pyfunction!(relpath, m)?)?;
    m.add_function(wrap_pyfunction!(realpath, m)?)?;
    m.add_function(wrap_pyfunction!(split, m)?)?;
    m.add_function(wrap_pyfunction!(splitext, m)?)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env::current_dir;
    use super::{_abspath, _commonpath, _dirname, _realpath, RealpathError, RealpathMode};

    const ALL_MODES: [RealpathMode; 4] = [
        RealpathMode::NonStrict,
        RealpathMode::AllowMissing,
        RealpathMode::AllButLast,
        RealpathMode::Strict,
    ];

    #[test]
    fn abspath() {
        let fname = "test.txt";
        let curdir = current_dir().unwrap();
        let result_str = _abspath(fname).unwrap();
        let ok_str = curdir.join(fname);
        let ok_str = ok_str.to_str().unwrap();
        assert_eq!(result_str, ok_str);

        let fname = "/path/to/test.txt";
        let result_str = _abspath(fname).unwrap();
        assert_eq!(result_str, fname);
    }

    #[test]
    fn dirname() {
        let fname = "/path/to/test.txt";
        let result_str = _dirname(fname);
        assert_eq!(result_str, "/path/to");

        let fname = "/";
        let result_str = _dirname(fname);
        assert_eq!(result_str, "/");

        let fname = "//";
        let result_str = _dirname(fname);
        assert_eq!(result_str, "//");

        let fname = "path/to/test.txt";
        let result_str = _dirname(fname);
        assert_eq!(result_str, "path/to");

        let dpath = "/path/to/dirname/";
        let result_str = _dirname(dpath);
        assert_eq!(result_str, "/path/to/dirname");
    }

    fn commonpath_of(paths: &[&str]) -> Option<String> {
        let paths: Vec<&[u8]> = paths.iter().map(|p| p.as_bytes()).collect();
        _commonpath(&paths).map(|ret| String::from_utf8(ret).unwrap())
    }

    #[test]
    fn commonpath() {
        let cases: &[(&[&str], &str)] = &[
            (&["/usr/local"], "/usr/local"),
            (&["/usr/local/", "/usr/local"], "/usr/local"),
            (&["/usr//local", "//usr/local"], "/usr/local"),
            (&["/usr/./local", "/./usr/local"], "/usr/local"),
            (&["/", "/dev"], "/"),
            (&["/usr", "/dev"], "/"),
            (&["/usr/lib/", "/usr/lib/python3"], "/usr/lib"),
            (&["/usr/lib", "/usr/lib64"], "/usr"),
            (&["spam", "alot"], ""),
            (&["and//jam", "and/spam//"], "and"),
            (&["and/jam", "and/spam", "alot"], ""),
            (&["and/jam", "and/spam", "and"], "and"),
            (&[""], ""),
            (&["", "spam/alot"], ""),
            (&["/日本/語", "/日本/本"], "/日本"),
        ];
        for (paths, expected) in cases {
            assert_eq!(commonpath_of(paths), Some(expected.to_string()), "{:?}", paths);
        }
    }

    #[test]
    fn commonpath_error() {
        assert_eq!(commonpath_of(&[]), None);
        assert_eq!(commonpath_of(&["/usr", "usr"]), None);
        assert_eq!(commonpath_of(&["", "/spam/alot"]), None);
    }

    #[test]
    fn realpath() {
        let cwd = current_dir().unwrap().to_str().unwrap().to_string();
        for mode in ALL_MODES {
            assert_eq!(_realpath("/", mode), Ok("/".to_string()));
            assert_eq!(_realpath("//", mode), Ok("/".to_string()));
            assert_eq!(_realpath("", mode), Ok(cwd.clone()));
            assert_eq!(_realpath(".", mode), Ok(cwd.clone()));
        }
    }

    #[test]
    fn realpath_missing() {
        let missing_dir = "/fpath-nonexistent-dir";
        let missing_file = "/fpath-nonexistent-dir/file";

        // only the last component is missing
        for mode in [RealpathMode::NonStrict, RealpathMode::AllowMissing, RealpathMode::AllButLast] {
            assert_eq!(_realpath(missing_dir, mode), Ok(missing_dir.to_string()));
        }
        assert_eq!(
            _realpath(missing_dir, RealpathMode::Strict),
            Err(RealpathError::os(libc::ENOENT, missing_dir))
        );

        // a non-last component is missing
        for mode in [RealpathMode::NonStrict, RealpathMode::AllowMissing] {
            assert_eq!(_realpath(missing_file, mode), Ok(missing_file.to_string()));
        }
        for mode in [RealpathMode::AllButLast, RealpathMode::Strict] {
            assert_eq!(
                _realpath(missing_file, mode),
                Err(RealpathError::os(libc::ENOENT, missing_dir))
            );
        }
    }

    #[test]
    fn realpath_nonterminal_file() {
        // `cargo test` runs in the crate root, where Cargo.toml is a regular file.
        let file = _abspath("Cargo.toml").unwrap();
        for path in ["Cargo.toml/", "Cargo.toml/.", "Cargo.toml/..", "Cargo.toml/x"] {
            assert!(_realpath(path, RealpathMode::NonStrict).is_ok());
            for mode in [RealpathMode::AllowMissing, RealpathMode::AllButLast, RealpathMode::Strict] {
                assert_eq!(_realpath(path, mode), Err(RealpathError::os(libc::ENOTDIR, &file)));
            }
        }
    }

    #[test]
    fn realpath_embedded_nul() {
        for mode in ALL_MODES {
            assert_eq!(_realpath("/\0", mode), Err(RealpathError::EmbeddedNul));
            assert_eq!(_realpath("/\0/..", mode), Err(RealpathError::EmbeddedNul));
        }
    }
}
