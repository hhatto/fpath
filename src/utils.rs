use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

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

macro_rules! str2pyobj {
    ( $py:expr, $s:expr, $is_bytes:expr ) => {
        {
            if $is_bytes {
                Ok(PyBytes::new($py, $s.as_bytes()).to_object($py))
            } else {
                Ok(PyString::new($py, $s).to_object($py))
            }
        }
    }
}

macro_rules! tuplestr2pyobj {
    ( $py:expr, $head:expr, $tail:expr, $is_bytes:expr ) => {
        {
            let (py_head, py_tail) = if $is_bytes {
                (
                    PyBytes::new($py, $head.as_bytes()).to_object($py),
                    PyBytes::new($py, $tail.as_bytes()).to_object($py),
                )
            } else {
                (
                    PyString::new($py, $head).to_object($py),
                    PyString::new($py, $tail).to_object($py),
                )
            };
            Ok(PyTuple::new($py, &[py_head, py_tail]).to_object($py))
        }
    }
}

pub fn pyobj2str(py: &Python, obj: &PyAny) -> Result<(String, bool), String> {
    match obj.downcast::<PyString>() {
        Ok(s) => Ok((s.to_string(), false)),
        Err(_) => match obj.downcast::<PyBytes>() {
            Ok(arg) => {
                let s = String::from_utf8(arg.as_bytes().to_vec());
                match s {
                    Err(e) => return Err(format!("undecoded data: {:?}", e)),
                    _ => {},
                }
                let s = s.unwrap();
                Ok((s, true))
            },
            Err(_) => pypathlike2str(py, obj),
        },
    }
}

pub fn pypathlike2str(py: &Python, obj: &PyAny) -> Result<(String, bool), String> {
    match obj.getattr("__fspath__") {
        Ok(func) => {
            match func.call0() {
                Ok(o) => pyobj2str(py, &o),
                Err(_) => Err("expected str, bytes or os.PathLike object".to_string()),
            }
        },
        Err(_) => Err("expected str, bytes or os.PathLike object".to_string()),
    }
}
