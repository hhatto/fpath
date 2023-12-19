extern crate memchr;
extern crate shellexpand;

use std::{env, str};
use std::collections::HashMap;
use std::env::current_dir;
use std::path::{Path, MAIN_SEPARATOR};
use pyo3::prelude::*;
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

fn _joinrealpath(path_str: &str, rest: &str, strict: bool, seen: &HashMap<String, Option<String>>) -> Result<(String, bool), PyErr> {
    let mut use_seen = seen.clone();
    let (mut ret_path, mut use_rest) = if _isabs(rest) {
        let (_head, tail) = rest.split_at(1);
        (MAIN_SEPARATOR.to_string(), tail)
    } else {
        (path_str.to_string(), rest)
    };

    while !use_rest.is_empty() {
        let (name, _, tmp_rest) = partition!(use_rest, "/");
        use_rest = tmp_rest;
        if name.is_empty() || name == "." {
            continue;
        }
        if name == ".." {
            if ret_path.is_empty() {
                ret_path = "..".to_string();
            } else {
                let (rp, n) = _inner_split(ret_path.as_str()).unwrap();
                ret_path = rp;
                if n == ".." {
                    let rp = _inner_join(ret_path.as_str(), &["..", ".."]);
                    ret_path = rp;
                }
            }
            continue;
        }

        let newpath = _inner_join(ret_path.as_str(), &[name]);
        let is_link = match std::fs::symlink_metadata(newpath.as_str()) {
            Ok(meta) => {
                meta.file_type().is_symlink()
            },
            Err(_) => {
                if strict {
                    return Err(exceptions::PyFileNotFoundError::new_err(format!("invalid path: {}", newpath)));
                }
                false
            }
        };
        if !is_link {
            ret_path = newpath;
            continue;
        }
        if use_seen.contains_key(newpath.as_str()) {
            match use_seen.get(newpath.as_str()).unwrap() {
                &Some(ref v) => {
                    ret_path = v.to_string();
                    continue;
                }
                _ => {}
            }
            if strict {
                if std::fs::metadata(newpath.as_str()).is_err() {
                    return Err(exceptions::PyOSError::new_err(format!("invalid path: {}", newpath)));
                }
            } else {
                return Ok((_inner_join(newpath.as_str(), &[use_rest]), false));
            }
        }

        use_seen.insert(newpath.clone(), None);
        let indeep = std::fs::read_link(newpath.clone()).unwrap();
        match _joinrealpath(
            ret_path.as_str(),
            indeep.to_str().unwrap(),
            strict,
            &use_seen.clone(),
        ) {
            Ok((rp, ok)) => {
                ret_path = rp;
                if !ok {
                    return Ok((_inner_join(ret_path.as_str(), &[use_rest]), false));
                }
            },
            Err(e) => return Err(e),
        };
        use_seen.insert(newpath, Some(ret_path.clone()));
    }

    Ok((ret_path.to_string(), true))
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

fn _basename(py: &Python, path_str: &str, is_bytes: bool) -> PyObject {
    let i = match memchr::memrchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => v + 1,
        None => 0,
    };
    if is_bytes {
        PyBytes::new(*py, path_str.split_at(i).1.as_bytes()).to_object(*py)
    } else {
        PyString::new(*py, path_str.split_at(i).1).to_object(*py)
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

fn _join(py: &Python, path_str: &str, path_list: &PyTuple, is_bytes: bool) -> PyResult<PyObject> {
    let mut ret_path = String::from(path_str);
    for x in path_list.get_slice(0, path_list.len()) {
        let b = pyobj2str(py, x);
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

    str2pyobj!(*py, ret_path.as_str(), is_bytes)
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

fn _realpath(path_str: &str, strict: bool) -> Result<String, PyErr> {
    let seen = HashMap::new();
    match _joinrealpath("", path_str, strict, &seen) {
        Ok((ret_path, _)) => return _abspath(ret_path.as_str()),
        Err(e) => return Err(e),
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

fn _inner_split(path_str: &str) -> Result<(String, String), String> {
    let (mut head, tail) = match memchr::memrchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => path_str.split_at(v + 1),
        None => ("", path_str),
    };
    let head_sep = numsep!(head.len());
    if !head.is_empty() && head != head_sep {
        head = head.trim_end_matches(MAIN_SEPARATOR);
    }
    return Ok((head.to_string(), tail.to_string()));
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
fn init_mod(_py: Python, m: &PyModule) -> PyResult<()> {

    #[pyfunction]
    #[pyo3(name = "abspath")]
    pub fn abspath(py: Python, path_str: &PyAny) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&py, path_str);
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
    pub fn basename(py: Python, path_str: &PyAny) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&py, path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        Ok(_basename(&py, arg_str.as_str(), is_bytes))
    }

    #[pyfunction]
    #[pyo3(name = "dirname")]
    pub fn dirname(py: Python, path_str: &PyAny) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&py, path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        str2pyobj!(py, _dirname(arg_str.as_str()), is_bytes)
    }

    #[pyfunction]
    #[pyo3(name = "exists")]
    pub fn exists(py: Python, path_str: &PyAny) -> PyResult<bool> {
        let arg_str = pyobj2str(&py, path_str);
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
    pub fn expanduser(py: Python, path_str: &PyAny) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&py, path_str);
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
    pub fn expandvars(py: Python, path_str: &PyAny) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&py, path_str);
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
    pub fn isabs(py: Python, path_str: &PyAny) -> PyResult<bool> {
        let arg_str = pyobj2str(&py, path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, _is_bytes) = arg_str.unwrap();
        Ok(_isabs(arg_str.as_str()))
    }

    #[pyfunction]
    #[pyo3(name = "islink")]
    pub fn islink(py: Python, path_str: &PyAny) -> PyResult<bool> {
        let arg_str = pyobj2str(&py, path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, _is_bytes) = arg_str.unwrap();
        Ok(_islink(arg_str.as_str()))
    }

    #[pyfunction]
    #[pyo3(name = "join", text_signature = "(path_str, *args)")]
    pub fn join(py: Python, path_str: &PyAny, args: &PyTuple) -> PyResult<PyObject> {
        if args.len() < 1 {
            let arg_str = pyobj2str(&py, path_str);
            match arg_str {
                Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
                _ => {}
            }
            let (arg_str, is_bytes) = arg_str.unwrap();
            return str2pyobj!(py, arg_str.as_str(), is_bytes)
        }

        let arg_str = pyobj2str(&py, path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        _join(&py, arg_str.as_str(), args, is_bytes)
    }

    #[pyfunction]
    #[pyo3(name = "normpath")]
    pub fn normpath(py: Python, path_str: &PyAny) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&py, path_str);
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
    pub fn relpath(py: Python, path_str: &PyAny, start: &PyAny) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&py, path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();

        let start_str = pyobj2str(&py, start);
        match start_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (start_str, _) = start_str.unwrap();

        str2pyobj!(py, _relpath(arg_str.as_str(), start_str.as_str()).as_str(), is_bytes)
    }

    #[pyfunction]
    #[pyo3(name = "realpath", signature = (path_str, *_py_args, **py_kwargs))]
    pub fn realpath(py: Python, path_str: &PyAny, _py_args: &PyTuple, py_kwargs: Option<&PyDict>) -> PyResult<PyObject> {
        let strict = if py_kwargs.is_some() {
            let kwargs = py_kwargs.expect("kwargs parse error");
            match kwargs.get_item("strict").expect("kwargs parse error") {
                Some(x) => x.extract::<bool>().expect("invalid strict value"),
                None => false,
            }
        } else {
            false
        };
        let arg_str = pyobj2str(&py, &path_str);
        match arg_str {
            Err(e) => return Err(exceptions::PyTypeError::new_err(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        match _realpath(arg_str.as_str(), strict) {
            Ok(s) => str2pyobj!(py, s.as_str(), is_bytes),
            Err(e) => Err(e),
        }
    }

    #[pyfunction]
    #[pyo3(name = "split")]
    pub fn split(py: Python, path_str: &PyAny) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&py, path_str);
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
    pub fn splitext(py: Python, path_str: &PyAny) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&py, path_str);
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
    use std::collections::HashMap;
    use super::{_abspath, _dirname, _joinrealpath, _realpath};

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

    #[test]
    fn realpath() {
        let fname = "//";
        let result_str = _realpath(fname, false).unwrap();
        assert_eq!(result_str, "/");
    }

    #[test]
    fn test_joinrealpath() {
        let fname = "//";
        let ret = _joinrealpath("", fname, false, &HashMap::new()).expect("joinrealpath error");
        assert_eq!(ret, ("/".to_string(), true));
    }
}
