extern crate memchr;
extern crate shellexpand;

use std::{env, str};
use std::borrow::Cow;
use std::collections::HashMap;
use std::env::current_dir;
use std::path::{Path, MAIN_SEPARATOR};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyBytes, PyDict, PyString, PyTuple};
use pyo3::exceptions;
use uzers::os::unix::UserExt;
use uzers::{get_user_by_uid, get_user_by_name, get_current_uid};

#[macro_use]
mod utils;
use utils::pyobj2str;

const SEP: u8 = MAIN_SEPARATOR as u8;


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
    /// Raised as `ValueError`, like `os.lstat()` does.
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
    if let Some(sentinel) = os_path_sentinel(py, &ALLOW_MISSING, "ALLOW_MISSING")? {
        if strict.is(sentinel) {
            return Ok(RealpathMode::AllowMissing);
        }
    }
    if let Some(sentinel) = os_path_sentinel(py, &ALL_BUT_LAST, "ALL_BUT_LAST")? {
        if strict.is(sentinel) {
            return Ok(RealpathMode::AllButLast);
        }
    }
    if strict.is_truthy()? {
        Ok(RealpathMode::Strict)
    } else {
        Ok(RealpathMode::NonStrict)
    }
}

/// Converts the `filename` argument of `realpath()`. Returns None for the
/// arguments that fpath can not handle (e.g. undecodable file names or
/// invalid types).
fn realpath_arg(obj: &Bound<'_, PyAny>) -> Option<(String, bool)> {
    match obj.cast::<PyString>() {
        // to_str() fails for a str with lone surrogates
        Ok(s) => s.to_str().ok().map(|s| (s.to_string(), false)),
        Err(_) => pyobj2str(obj).ok(),
    }
}

/// Delegates to `os.path.realpath()`.
fn realpath_fallback(
    py: Python<'_>,
    filename: &Bound<'_, PyAny>,
    strict: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let realpath = py.import("os")?.getattr("path")?.getattr("realpath")?;
    let kwargs = PyDict::new(py);
    if let Some(strict) = strict {
        kwargs.set_item("strict", strict)?;
    }
    Ok(realpath.call((filename,), Some(&kwargs))?.unbind())
}

fn realpath_error(py: Python<'_>, err: RealpathError, is_bytes: bool) -> PyErr {
    match err {
        RealpathError::Os { errno, filename } => {
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
        RealpathError::EmbeddedNul => exceptions::PyValueError::new_err("embedded null byte"),
        RealpathError::NonUtf8 => exceptions::PyValueError::new_err("path is not valid UTF-8"),
    }
}

pub fn _inner_join(path_str: &str, path_list: &[&str]) -> String {
    let mut ret_path = String::from(path_str);
    for b in path_list {
        if b.starts_with(MAIN_SEPARATOR) {
            ret_path = b.to_string();
        } else if ret_path.is_empty() || ret_path.ends_with(MAIN_SEPARATOR) {
            ret_path.push_str(b);
        } else {
            ret_path.push(MAIN_SEPARATOR);
            ret_path.push_str(b);
        }
    }

    ret_path
}

fn _abspath(path_str: &str) -> Result<String, PyErr> {
    if _isabs(path_str) {
        return Ok(_normpath(path_str));
    }
    match current_dir() {
        Ok(c) => Ok(_normpath(c.join(path_str).to_string_lossy().into_owned().as_str())),
        Err(e) => Err(exceptions::PyOSError::new_err(format!("{}", e))),
    }
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

// NOTE: use shellexpand::tilde?
fn _expanduser(path_str: &str) -> String {
    let i = match memchr::memchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => v,
        None => path_str.len(),
    };

    let userhome: String = if i == 1 {
        match env::var("HOME") {
            Ok(v) => v,
            Err(_) => {
                match get_user_by_uid(get_current_uid() as u32) {
                    Some(u) => u.home_dir().to_str().unwrap().to_string(),
                    None => path_str.to_string(),
                }
            }
        }
    } else {
        let name = str::from_utf8(&path_str.as_bytes()[1..i]).unwrap();
        match get_user_by_name(name) {
            Some(u) => u.home_dir().to_str().unwrap().to_string(),
            None => path_str.to_string(),
        }
    };

    let mut ret_userhome = userhome.trim_end_matches(MAIN_SEPARATOR).to_string();
    let strip_path_str = str::from_utf8(&path_str.as_bytes()[i..]).unwrap();
    ret_userhome.push_str(strip_path_str);

    if ret_userhome.is_empty() {
        MAIN_SEPARATOR.to_string()
    } else {
        ret_userhome
    }
}

fn _expandvars(path_str: &str) -> String {
    match shellexpand::env(path_str) {
        Ok(v) => v.into(),
        Err(_) => path_str.to_string(),
    }
}

#[inline(always)]
fn _isabs(path_str: &str) -> bool {
    path_str.starts_with(MAIN_SEPARATOR)
}

fn _join(py: Python<'_>, path_str: &str, path_list: &Bound<'_, PyTuple>, is_bytes: bool) -> PyResult<Py<PyAny>> {
    let mut ret_path = String::from(path_str);
    for x in path_list.iter() {
        let b = pyobj2str(&x);
        match b {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (b, b_is_bytes) = b.unwrap();
        if is_bytes != b_is_bytes {
            return Err(exceptions::PyTypeError::new_err("Can't mix strings and bytes in path components"));
        }

        if b.starts_with(MAIN_SEPARATOR) {
            ret_path = b.to_string();
        } else if ret_path.is_empty() || ret_path.ends_with(MAIN_SEPARATOR) {
            ret_path.push_str(b.as_str());
        } else {
            ret_path.push(MAIN_SEPARATOR);
            ret_path.push_str(b.as_str());
        }
    }

    str2pyobj!(py, ret_path.as_str(), is_bytes)
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

static OS_FSPATH: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static OS_PATH_COMMONPATH: PyOnceLock<Py<PyAny>> = PyOnceLock::new();

/// Delegates to `os.path.commonpath()`, e.g. to raise the same errors.
fn commonpath_fallback(py: Python<'_>, paths: &[Bound<'_, PyAny>]) -> PyResult<Py<PyAny>> {
    let commonpath = OS_PATH_COMMONPATH.import(py, "os.path", "commonpath")?;
    Ok(commonpath.call1((PyTuple::new(py, paths)?,))?.unbind())
}

fn _relpath(path_str: &str, start: &str) -> String {
    let start_list: Vec<String> = _abspath(start)
        .unwrap()
        .split(MAIN_SEPARATOR)
        .into_iter()
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect();
    let path_list: Vec<String> = _abspath(path_str)
        .unwrap()
        .split(MAIN_SEPARATOR)
        .into_iter()
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect();
    let cprefix = _commonprefix(&vec![start_list.as_slice(), path_list.as_slice()]).unwrap();
    let i = cprefix.len();
    let num = start_list.len() - i;
    let plist_list: Vec<&str> = path_list[i..].iter().map(|x| x.as_str()).collect();
    let rel_list: Vec<&str> = (0..num).map(|_| "..").chain(plist_list).collect();
    if rel_list.len() == 0 {
        return ".".to_string();
    }
    _inner_join(rel_list[0], &rel_list[1..])
}

fn _split<'a>(path_str: &'a str) -> Result<(&'a str, &'a str), String> {
    let (mut head, tail) = match memchr::memrchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => path_str.split_at(v + 1),
        None => ("", path_str),
    };
    let head_sep = numsep!(head.len());
    if !head.is_empty() && head != head_sep {
        head = head.trim_end_matches(MAIN_SEPARATOR);
    }
    return Ok((head, tail));
}

fn _splitext<'a>(path_str: &'a str) -> Result<(&'a str, &'a str), String> {
    let sep_index = match memchr::memrchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => v as i32,
        None => -1,
    };
    let ext_index = match memchr::memrchr('.' as u8, path_str.as_bytes()) {
        Some(v) => v as i32,
        None => -1,
    };

    if ext_index > sep_index {
        let mut filename_index = sep_index + 1;
        while filename_index < ext_index {
            match path_str.chars().nth(filename_index as usize) {
                Some(c) => {
                    if c != '.' {
                        let (head, tail) = path_str.split_at(ext_index as usize);
                        return Ok((head, tail));
                    }
                }
                None => {
                    let (head, tail) = path_str.split_at(ext_index as usize);
                    return Ok((head, tail));
                }
            };
            filename_index += 1
        }
    }

    return Ok((path_str, ""));
}

#[pymodule]
#[pyo3(name = "_fpath")]
fn init_mod(m: &Bound<'_, PyModule>) -> PyResult<()> {

    #[pyfunction]
    #[pyo3(name = "abspath")]
    pub fn abspath(py: Python<'_>, path_str: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let arg_str = pyobj2str(path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();

        match _abspath(arg_str.as_str()) {
            Ok(s) => {
                str2pyobj!(py, s.as_str(), is_bytes)
            }
            Err(e) => Err(exceptions::PyOSError::new_err(e)),
        }
    }

    #[pyfunction]
    #[pyo3(name = "basename")]
    pub fn basename(py: Python<'_>, path_str: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let arg_str = pyobj2str(path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        Ok(_basename(py, arg_str.as_str(), is_bytes))
    }

    #[pyfunction]
    #[pyo3(name = "commonpath")]
    pub fn commonpath(py: Python<'_>, paths: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        // paths = tuple(map(os.fspath, paths))
        let mut items = Vec::new();
        for item in paths.try_iter()? {
            let item = item?;
            if item.is_exact_instance_of::<PyString>() || item.is_exact_instance_of::<PyBytes>() {
                items.push(item);
            } else {
                items.push(OS_FSPATH.import(py, "os", "fspath")?.call1((item,))?);
            }
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
    pub fn dirname(py: Python<'_>, path_str: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let arg_str = pyobj2str(path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        str2pyobj!(py, _dirname(arg_str.as_str()), is_bytes)
    }

    #[pyfunction]
    #[pyo3(name = "exists")]
    pub fn exists(path_str: &Bound<'_, PyAny>) -> PyResult<bool> {
        let arg_str = pyobj2str(path_str);
        // TODO: from file descriptor
        //let arg_str = match arg_str {
        //    Err(e) => {
        //        // for file descriptor argument
        //        let gil = Python::acquire_gil();
        //        let py = gil.python();
        //        match path_str.extract::<i32>(py) {
        //            Ok(fd) => {
        //                let f = unsafe { fs::File::from_raw_fd(fd) };
        //                Ok(f.exists())
        //            }
        //            Err(_) => Err(exceptions::TypeError::py_err(e)),
        //        }
        //    }
        //    Ok(s) => Ok(s.0)
        //};
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, _) = arg_str.unwrap();
        Ok(_exists(arg_str.as_str()))
    }

    #[pyfunction]
    #[pyo3(name = "expanduser")]
    pub fn expanduser(py: Python<'_>, path_str: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let arg_str = pyobj2str(path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        if !arg_str.starts_with("~") {
            return str2pyobj!(py, arg_str.as_str(), is_bytes);
        }

        let ret_str = _expanduser(arg_str.as_str());
        str2pyobj!(py, ret_str.as_str(), is_bytes)
    }

    #[pyfunction]
    #[pyo3(name = "expandvars")]
    pub fn expandvars(py: Python<'_>, path_str: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let arg_str = pyobj2str(path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        if memchr::memchr(b'$', arg_str.as_bytes()).is_none() {
            return str2pyobj!(py, arg_str.as_str(), is_bytes);
        }

        let ret_str = _expandvars(arg_str.as_str());
        str2pyobj!(py, ret_str.as_str(), is_bytes)
    }

    #[pyfunction]
    #[pyo3(name = "isabs")]
    pub fn isabs(path_str: &Bound<'_, PyAny>) -> PyResult<bool> {
        let arg_str = pyobj2str(path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, _is_bytes) = arg_str.unwrap();
        Ok(_isabs(arg_str.as_str()))
    }

    #[pyfunction]
    #[pyo3(name = "islink")]
    pub fn islink(path_str: &Bound<'_, PyAny>) -> PyResult<bool> {
        let arg_str = pyobj2str(path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, _is_bytes) = arg_str.unwrap();
        Ok(_islink(arg_str.as_str()))
    }

    #[pyfunction]
    #[pyo3(name = "join", text_signature = "(path_str, *args)")]
    pub fn join(py: Python<'_>, path_str: &Bound<'_, PyAny>, args: &Bound<'_, PyTuple>) -> PyResult<Py<PyAny>> {
        if args.len() < 1 {
            let arg_str = pyobj2str(path_str);
            match arg_str {
                Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
                _ => {}
            }
            let (arg_str, is_bytes) = arg_str.unwrap();
            return str2pyobj!(py, arg_str.as_str(), is_bytes)
        }

        let arg_str = pyobj2str(path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        _join(py, arg_str.as_str(), args, is_bytes)
    }

    #[pyfunction]
    #[pyo3(name = "normpath")]
    pub fn normpath(py: Python<'_>, path_str: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let arg_str = pyobj2str(path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        let ret_str = _normpath(arg_str.as_str());
        str2pyobj!(py, ret_str.as_str(), is_bytes)
    }

    #[pyfunction]
    #[pyo3(name = "relpath")]
    pub fn relpath(py: Python<'_>, path_str: &Bound<'_, PyAny>, start: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let arg_str = pyobj2str(path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();

        let start_str = pyobj2str(start);
        match start_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (start_str, _) = start_str.unwrap();

        str2pyobj!(py, _relpath(arg_str.as_str(), start_str.as_str()).as_str(), is_bytes)
    }

    #[pyfunction]
    #[pyo3(
        name = "realpath",
        signature = (filename, *, strict=None),
        text_signature = "(filename, *, strict=False)"
    )]
    pub fn realpath(py: Python<'_>, filename: &Bound<'_, PyAny>, strict: Option<&Bound<'_, PyAny>>) -> PyResult<Py<PyAny>> {
        let mode = realpath_mode(py, strict)?;
        let (arg_str, is_bytes) = match realpath_arg(filename) {
            Some(arg) => arg,
            None => return realpath_fallback(py, filename, strict),
        };
        match _realpath(arg_str.as_str(), mode) {
            Ok(s) => str2pyobj!(py, s.as_str(), is_bytes),
            Err(RealpathError::NonUtf8) => realpath_fallback(py, filename, strict),
            Err(e) => Err(realpath_error(py, e, is_bytes)),
        }
    }

    #[pyfunction]
    #[pyo3(name = "split")]
    pub fn split(py: Python<'_>, path_str: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let arg_str = pyobj2str(path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        match _split(arg_str.as_str()) {
            Ok((head, tail)) => tuplestr2pyobj!(py, head, tail, is_bytes),
            Err(e) => Err(exceptions::PyOSError::new_err(e)),
        }
    }

    #[pyfunction]
    #[pyo3(name = "splitext")]
    pub fn splitext(py: Python<'_>, path_str: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let arg_str = pyobj2str(path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        match _splitext(arg_str.as_str()) {
            Ok((head, tail)) => tuplestr2pyobj!(py, head, tail, is_bytes),
            Err(e) => Err(exceptions::PyOSError::new_err(e)),
        }
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
    m.add_function(wrap_pyfunction!(normpath, m)?)?;
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
