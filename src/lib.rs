#![feature(proc_macro, specialization)]

extern crate pyo3;

#[macro_use]
extern crate lazy_static;
extern crate memchr;

use std::str;
use std::collections::HashMap;
use std::env::current_dir;
use std::path::MAIN_SEPARATOR;
use pyo3::prelude::*;

pub const SEP: u8 = MAIN_SEPARATOR as u8;
lazy_static! {
    pub static ref SEP_STR: &'static str = str::from_utf8(&[SEP]).unwrap();
}

macro_rules! numsep {
    ( $x:expr ) => (
        String::from_utf8((0..$x).map(|_| SEP).collect::<Vec<u8>>()).unwrap()
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
                let (rp, n) = _split(ret_path.as_str()).unwrap();
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
                },
                _ => {},
            }
            return (_inner_join(newpath.as_str(), &[use_rest]), false)
        }

        use_seen.insert(newpath.clone(), None);
        let indeep = std::fs::read_link(newpath.clone()).unwrap();
        let (rp, ok) = _joinrealpath(ret_path.as_str(), indeep.to_str().unwrap(), use_seen.clone());
        ret_path = rp;
        if !ok {
            return (_inner_join(ret_path.as_str(), &[use_rest]), false)
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
        return _normpath(path_str)
    }
    match current_dir() {
        Ok(c) => {
            _normpath(c.join(path_str).to_str().unwrap())
        },
        Err(e) => {
            Err(format!("{}", e))
        }
    }
}

fn _basename<'a>(path_str: &'a str) -> &'a str {
    let i = match memchr::memrchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => v + 1,
        None => 0,
    };
    path_str.split_at(i).1
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

#[inline(always)]
fn _isabs(path_str: &str) -> bool {
    path_str.starts_with(MAIN_SEPARATOR)
}

fn _join(path_str: &str, path_list: &PyTuple) -> PyResult<String> {
    if path_list.len() < 1 {
        return Ok(path_str.to_string())
    }

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
        let b = b.unwrap();

        if b.starts_with(MAIN_SEPARATOR) {
            ret_path = b.to_string();
        } else if ret_path.is_empty() || ret_path.ends_with(MAIN_SEPARATOR) {
            ret_path.push_str(b.as_str());
        } else {
            ret_path.push(MAIN_SEPARATOR);
            ret_path.push_str(b.as_str());
        }
    }

    Ok(ret_path)
}

fn _normpath(path_str: &str) -> Result<String, String> {
    if path_str.is_empty() {
        return Ok(".".to_string())
    }
    let initial_slashes = path_str.starts_with(MAIN_SEPARATOR);
    let initial_slashes_num = if initial_slashes &&
        path_str.starts_with("//") && !path_str.starts_with("///") {
        2
    } else {
        1
    };
    let mut new_comps: Vec<&str> = vec![];
    for comp in path_str.split('/').into_iter() {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp != ".." ||
           (!initial_slashes && new_comps.len() == 0) ||
           (!new_comps.is_empty() && *new_comps.last().unwrap() == "..") {
               new_comps.push(comp);
        } else if !new_comps.is_empty() {
            new_comps.pop();
        }
    }
    let new_comps_path = new_comps.join("/");
    if initial_slashes {
        let mut head_sep = numsep!(initial_slashes_num);
        head_sep.push_str(new_comps_path.as_str());
        Ok(head_sep)
    } else if new_comps_path.is_empty() {
        Ok(".".to_string())
    } else {
        Ok(new_comps_path)
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
            return Ok(s1[..i].iter().map(|x| x.to_string()).collect())
        }
    }
    Ok(s1.iter().map(|x| x.to_string()).collect())
}

fn _relpath(path_str: &str, start: &str) -> PyResult<String> {
    let start_list: Vec<String> = _abspath(start).unwrap()
        .split(MAIN_SEPARATOR)
        .into_iter()
        .filter(|x| !x.is_empty()).map(|x| x.to_string()).collect();
    let path_list: Vec<String> = _abspath(path_str).unwrap()
        .split(MAIN_SEPARATOR)
        .into_iter()
        .filter(|x| !x.is_empty()).map(|x| x.to_string()).collect();
    let cprefix = _commonprefix(
        &vec![
            start_list.as_slice(),
            path_list.as_slice()
        ]).unwrap();
    let i = cprefix.len();
    let num = start_list.len() - i;
    let plist_list: Vec<&str> = path_list[i..].iter().map(|x| x.as_str()).collect();
    let rel_list: Vec<&str> = (0..num).map(|_| "..").chain(plist_list).collect();
    if rel_list.len() == 0 {
        return Ok(".".to_string());
    }
    Ok(_inner_join(rel_list[0], &rel_list[1..]))
}

fn _split(path_str: &str) -> Result<(String, String), String> {
    let (mut head, tail) = match memchr::memrchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => path_str.split_at(v + 1),
        None => ("", path_str),
    };
    let head_sep = numsep!(head.len());
    if !head.is_empty() && head != head_sep {
        head = head.trim_right_matches(MAIN_SEPARATOR);
    }
    return Ok((head.to_string(), tail.to_string()))
}

fn _splitext(path_str: &str) -> Result<(String, String), String> {
    let sep_index= match memchr::memrchr(MAIN_SEPARATOR as u8, path_str.as_bytes()) {
        Some(v) => v as i32,
        None => -1,
    };
    let ext_index= match memchr::memrchr('.' as u8, path_str.as_bytes()) {
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
                        return Ok((head.to_string(), tail.to_string()))
                    }
                },
                None => {
                    let (head, tail) = path_str.split_at(ext_index as usize);
                    return Ok((head.to_string(), tail.to_string()))
                },
            };
            filename_index += 1
        }
    }

    return Ok((path_str.to_string(), "".to_string()))
}

fn pyobj2str(obj: &PyObject) -> Result<String, String> {
    let gil = Python::acquire_gil();
    let py = gil.python();
    match obj.extract::<String>(py) {
        Ok(s) => {
            Ok(s)
        },
        Err(_) => {
            match obj.cast_as::<PyBytes>(py) {
                Ok(arg) => {
                    Ok(String::from_utf8(arg.data().to_vec()).unwrap())
                },
                Err(_) => {
                    Err("invalid argument type".to_string())
                }
            }
        },
    }
}

#[py::modinit(_fpath)]
fn init_mod(py: Python, m: &PyModule) -> PyResult<()> {

    #[pyfn(m, "abspath")]
    pub fn abspath(path_str: PyObject) -> PyResult<String> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let arg_str = arg_str.unwrap();

        match _abspath(arg_str.as_str()) {
            Ok(s) => Ok(s),
            Err(_) => exc::OSError.into(),
        }
    }

    #[pyfn(m, "basename")]
    pub fn basename(path_str: PyObject) -> PyResult<String> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let arg_str = arg_str.unwrap();
        Ok(_basename(arg_str.as_str()).to_string())
    }

    #[pyfn(m, "dirname")]
    pub fn dirname(path_str: PyObject) -> PyResult<String> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let arg_str = arg_str.unwrap();
        Ok(_dirname(arg_str.as_str()).to_string())
    }

    #[pyfn(m, "isabs")]
    pub fn isabs(path_str: PyObject) -> PyResult<bool> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let arg_str = arg_str.unwrap();
        Ok(_isabs(arg_str.as_str()))
    }

    #[pyfn(m, "join", path_str, path_list="*")]
    pub fn join(path_str: PyObject, path_list: &PyTuple) -> PyResult<String> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let arg_str = arg_str.unwrap();

        _join(arg_str.as_str(), path_list)
    }

    #[pyfn(m, "relpath")]
    pub fn relpath(path_str: PyObject, start: PyObject) -> PyResult<String> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let arg_str = arg_str.unwrap();

        let start_str = pyobj2str(&start);
        match start_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let start_str = start_str.unwrap();

        _relpath(arg_str.as_str(), start_str.as_str())
    }

    #[pyfn(m, "realpath")]
    pub fn realpath(path_str: PyObject) -> PyResult<String> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let arg_str = arg_str.unwrap();
        match _realpath(arg_str.as_str()) {
            Ok(s) => Ok(s),
            Err(_) => exc::OSError.into(),
        }
    }

    #[pyfn(m, "split")]
    pub fn split(path_str: PyObject) -> PyResult<(String, String)> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let arg_str = arg_str.unwrap();
        match _split(arg_str.as_str()) {
            Ok(s) => Ok(s),
            Err(_) => exc::OSError.into(),
        }
    }

    #[pyfn(m, "splitext")]
    pub fn splitext(path_str: PyObject) -> PyResult<(String, String)> {
        let arg_str = pyobj2str(&path_str);
        match arg_str {
            Err(e) => return Err(exc::TypeError::new(e)),
            _ => {}
        }
        let arg_str = arg_str.unwrap();
        match _splitext(arg_str.as_str()) {
            Ok(s) => Ok(s),
            Err(_) => exc::OSError.into(),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env::current_dir;
    use std::collections::HashMap;
    use ::{_abspath, _basename, _dirname, _realpath, _joinrealpath};

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
    fn basename() {
        let fname = "/path/to/test.txt";
        let result_str = _basename(fname);
        assert_eq!(result_str, "test.txt");

        let fname = "test.txt";
        let result_str = _basename(fname);
        assert_eq!(result_str, fname);

        let dpath = "/path/to/dirname/";
        let result_str = _basename(dpath);
        assert_eq!(result_str, "");
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
