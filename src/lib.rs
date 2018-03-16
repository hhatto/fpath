#![feature(proc_macro, specialization)]

extern crate pyo3;

#[macro_use]
extern crate lazy_static;
extern crate memchr;

use std::str;
use std::collections::HashMap;
use std::env::current_dir;
use std::path::{Path, MAIN_SEPARATOR};
use pyo3::prelude::*;

pub const SEP: u8 = MAIN_SEPARATOR as u8;
lazy_static! {
    pub static ref SEP_STR: &'static str = str::from_utf8(&[SEP]).unwrap();
}

macro_rules! numsep {
    ( $x:expr ) => (
        unsafe { String::from_utf8_unchecked((0..$x).map(|_| SEP).collect::<Vec<u8>>()) }
    )
}

macro_rules! partition {
    ( $x:expr, $sep:expr ) => {
        match memchr::memchr($sep.as_bytes()[0], $x.as_bytes()) {
            Some(i) => {
                let (head, tail) = $x.split_at(i+1);
                (&head[..i], $sep, tail)
            },
            None => {
                ($x, "", "")
            },
        }
    }
}

#[inline(always)]
fn _islink(path_str: &str) -> bool {
    std::fs::read_link(path_str).is_ok()
}

fn _joinrealpath(path_str: &str, rest: &str, seen: HashMap<String, Option<String>>) -> (String, bool) {
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
        if !_islink(newpath.as_str()) {
            ret_path = newpath.to_string();
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
            return (_inner_join(newpath.as_str(), &[use_rest]), false);
        }

        use_seen.insert(newpath.clone(), None);
        let indeep = std::fs::read_link(newpath.clone()).unwrap();
        let (rp, ok) = _joinrealpath(
            ret_path.as_str(),
            indeep.to_str().unwrap(),
            use_seen.clone(),
        );
        ret_path = rp;
        if !ok {
            return (_inner_join(ret_path.as_str(), &[use_rest]), false);
        }
        use_seen.insert(newpath, Some(ret_path.clone()));
    }

    (ret_path.to_string(), true)
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

fn _abspath(path_str: &str) -> Result<String, String> {
    if _isabs(path_str) {
        return Ok(_normpath(path_str));
    }
    match current_dir() {
        Ok(c) => Ok(_normpath(c.join(path_str).to_str().unwrap())),
        Err(e) => Err(format!("{}", e)),
    }
}

fn _basename(path_str: &str, is_bytes: bool) -> PyObject {
    let i = match memchr::memrchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => v + 1,
        None => 0,
    };
    let gil = Python::acquire_gil();
    let py = gil.python();
    if is_bytes {
        PyBytes::new(py, path_str.split_at(i).1.as_bytes()).to_object(py)
    } else {
        PyString::new(py, path_str.split_at(i).1).to_object(py)
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
        head.trim_right_matches(MAIN_SEPARATOR)
    } else {
        head
    }
}

fn _exists(path_str: &str) -> bool {
    Path::new(path_str).exists()
}

#[inline(always)]
fn _isabs(path_str: &str) -> bool {
    path_str.starts_with(MAIN_SEPARATOR)
}

fn _join(path_str: &str, path_list: &PyTuple, is_bytes: bool) -> PyResult<PyObject> {
    // path_list > 0

    let mut is_first = true;
    let mut ret_path = String::from(path_str);
    for x in path_list.as_slice() {
        if is_first {
            is_first = false;
            continue;
        }
        let b = pyobj2str(x);
        match b {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (b, _) = b.unwrap();

        if b.starts_with(MAIN_SEPARATOR) {
            ret_path = b.to_string();
        } else if ret_path.is_empty() || ret_path.ends_with(MAIN_SEPARATOR) {
            ret_path.push_str(b.as_str());
        } else {
            ret_path.push(MAIN_SEPARATOR);
            ret_path.push_str(b.as_str());
        }
    }

    let gil = Python::acquire_gil();
    let py = gil.python();
    if is_bytes {
        Ok(PyBytes::new(py, ret_path.as_bytes()).to_object(py))
    } else {
        Ok(PyString::new(py, ret_path.as_str()).to_object(py))
    }
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

fn _realpath(path_str: &str) -> Result<String, String> {
    let seen = HashMap::new();
    let (ret_path, _) = _joinrealpath("", path_str, seen);
    _abspath(ret_path.as_str())
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

fn _relpath(path_str: &str, start: &str) -> PyResult<String> {
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
        return Ok(".".to_string());
    }
    Ok(_inner_join(rel_list[0], &rel_list[1..]))
}

fn _inner_split(path_str: &str) -> Result<(String, String), String> {
    let (mut head, tail) = match memchr::memrchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => path_str.split_at(v + 1),
        None => ("", path_str),
    };
    let head_sep = numsep!(head.len());
    if !head.is_empty() && head != head_sep {
        head = head.trim_right_matches(MAIN_SEPARATOR);
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
        head = head.trim_right_matches(MAIN_SEPARATOR);
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

fn pyobj2str(obj: &PyObject) -> Result<(String, bool), String> {
    let gil = Python::acquire_gil();
    let py = gil.python();
    match obj.extract::<String>(py) {
        Ok(s) => Ok((s, false)),
        Err(_) => match obj.extract::<&PyBytes>(py) {
            Ok(arg) => Ok((String::from_utf8(arg.data().to_vec()).unwrap(), true)),
            Err(_) => pypathlike2str(obj),
        },
    }
}

fn pypathlike2str(obj: &PyObject) -> Result<(String, bool), String> {
    let gil = Python::acquire_gil();
    let py = gil.python();
    match obj.getattr(py, "__fspath__") {
        Ok(func) => {
            match func.call0(py) {
                Ok(o) => pyobj2str(&o),
                Err(_) => Err("not PathLike object".to_string()),
            }
        },
        Err(_) => Err("not PathLike object".to_string()),
    }
}


#[py::modinit(_fpath)]
fn init_mod(py: Python, m: &PyModule) -> PyResult<()> {
    #[pyfn(m, "abspath")]
    pub fn abspath(path_str: PyObject) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();

        match _abspath(arg_str.as_str()) {
            Ok(s) => {
                let gil = Python::acquire_gil();
                let py = gil.python();
                if is_bytes {
                    Ok(PyBytes::new(py, s.as_bytes()).to_object(py))
                } else {
                    Ok(PyString::new(py, s.as_str()).to_object(py))
                }
            }
            Err(_) => exc::OSError.into(),
        }
    }

    #[pyfn(m, "basename")]
    pub fn basename(path_str: PyObject) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        Ok(_basename(arg_str.as_str(), is_bytes))
    }

    #[pyfn(m, "dirname")]
    pub fn dirname(path_str: PyObject) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        let gil = Python::acquire_gil();
        let py = gil.python();
        if is_bytes {
            Ok(PyBytes::new(py, _dirname(arg_str.as_str()).as_bytes()).to_object(py))
        } else {
            Ok(PyString::new(py, _dirname(arg_str.as_str())).to_object(py))
        }
    }

    #[pyfn(m, "exists")]
    pub fn exists(path_str: PyObject) -> PyResult<bool> {
        let arg_str = pyobj2str(&path_str);
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
        //            Err(_) => Err(exc::TypeError::new(e)),
        //        }
        //    }
        //    Ok(s) => Ok(s.0)
        //};
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (arg_str, _) = arg_str.unwrap();
        Ok(_exists(arg_str.as_str()))
    }

    #[pyfn(m, "isabs")]
    pub fn isabs(path_str: PyObject) -> PyResult<bool> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (arg_str, _is_bytes) = arg_str.unwrap();
        Ok(_isabs(arg_str.as_str()))
    }

    #[pyfn(m, "islink")]
    pub fn islink(path_str: PyObject) -> PyResult<bool> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (arg_str, _is_bytes) = arg_str.unwrap();
        Ok(_islink(arg_str.as_str()))
    }

    #[pyfn(m, "join", path_str, path_list = "*")]
    pub fn join(path_str: PyObject, path_list: &PyTuple) -> PyResult<PyObject> {
        if path_list.len() < 1 {
            return Ok(path_str);
        }

        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        _join(arg_str.as_str(), path_list, is_bytes)
    }

    #[pyfn(m, "normpath")]
    pub fn normpath(path_str: PyObject) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        let ret_str = _normpath(arg_str.as_str());
        let gil = Python::acquire_gil();
        let py = gil.python();
        if is_bytes {
            Ok(PyBytes::new(py, ret_str.as_bytes()).to_object(py))
        } else {
            Ok(PyString::new(py, ret_str.as_str()).to_object(py))
        }
    }

    #[pyfn(m, "relpath")]
    pub fn relpath(path_str: PyObject, start: PyObject) -> PyResult<String> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (arg_str, _is_bytes) = arg_str.unwrap();

        let start_str = pyobj2str(&start);
        match start_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (start_str, _) = start_str.unwrap();

        _relpath(arg_str.as_str(), start_str.as_str())
    }

    #[pyfn(m, "realpath")]
    pub fn realpath(path_str: PyObject) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        match _realpath(arg_str.as_str()) {
            Ok(s) => {
                let gil = Python::acquire_gil();
                let py = gil.python();
                if is_bytes {
                    Ok(PyBytes::new(py, s.as_bytes()).to_object(py))
                } else {
                    Ok(PyString::new(py, s.as_str()).to_object(py))
                }
            }
            Err(e) => Err(exc::OSError::new(e)),
        }
    }

    #[pyfn(m, "split")]
    pub fn split(path_str: PyObject) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        match _split(arg_str.as_str()) {
            Ok((head, tail)) => {
                let gil = Python::acquire_gil();
                let py = gil.python();
                let (py_head, py_tail) = if is_bytes {
                    (
                        PyBytes::new(py, head.as_bytes()).to_object(py),
                        PyBytes::new(py, tail.as_bytes()).to_object(py),
                    )
                } else {
                    (
                        PyString::new(py, head).to_object(py),
                        PyString::new(py, tail).to_object(py),
                    )
                };
                Ok(PyTuple::new(py, &[py_head, py_tail]).to_object(py))
            }
            Err(_) => exc::OSError.into(),
        }
    }

    #[pyfn(m, "splitext")]
    pub fn splitext(path_str: PyObject) -> PyResult<PyObject> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let (arg_str, is_bytes) = arg_str.unwrap();
        match _splitext(arg_str.as_str()) {
            Ok((head, tail)) => {
                let gil = Python::acquire_gil();
                let py = gil.python();
                let (py_head, py_tail) = if is_bytes {
                    (
                        PyBytes::new(py, head.as_bytes()).to_object(py),
                        PyBytes::new(py, tail.as_bytes()).to_object(py),
                    )
                } else {
                    (
                        PyString::new(py, head).to_object(py),
                        PyString::new(py, tail).to_object(py),
                    )
                };
                Ok(PyTuple::new(py, &[py_head, py_tail]).to_object(py))
            }
            Err(_) => exc::OSError.into(),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env::current_dir;
    use std::collections::HashMap;
    use {_abspath, _dirname, _joinrealpath, _realpath};

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
        let result_str = _realpath(fname).unwrap();
        assert_eq!(result_str, "/");
    }

    #[test]
    fn test_joinrealpath() {
        let fname = "//";
        let ret = _joinrealpath("", fname, HashMap::new());
        assert_eq!(ret, ("/".to_string(), true));
    }
}
