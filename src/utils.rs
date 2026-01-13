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
                Ok(PyBytes::new($py, $s.as_bytes()).unbind().into_any())
            } else {
                Ok(PyString::new($py, $s).unbind().into_any())
            }
        }
    }
}

macro_rules! tuplestr2pyobj {
    ( $py:expr, $head:expr, $tail:expr, $is_bytes:expr ) => {
        {
            let (py_head, py_tail): (Py<PyAny>, Py<PyAny>) = if $is_bytes {
                (
                    PyBytes::new($py, $head.as_bytes()).unbind().into_any(),
                    PyBytes::new($py, $tail.as_bytes()).unbind().into_any(),
                )
            } else {
                (
                    PyString::new($py, $head).unbind().into_any(),
                    PyString::new($py, $tail).unbind().into_any(),
                )
            };
            Ok(PyTuple::new($py, &[py_head, py_tail])?.unbind().into_any())
        }
    }
}

pub fn pyobj2str(obj: &Bound<'_, PyAny>) -> Result<(String, bool), String> {
    if let Ok(s) = obj.cast::<PyString>() {
        return Ok((s.to_string(), false));
    }
    if let Ok(arg) = obj.cast::<PyBytes>() {
        let s = String::from_utf8(arg.as_bytes().to_vec());
        match s {
            Err(e) => return Err(format!("undecoded data: {:?}", e)),
            Ok(s) => return Ok((s, true)),
        }
    }
    pypathlike2str(obj)
}

pub fn pypathlike2str(obj: &Bound<'_, PyAny>) -> Result<(String, bool), String> {
    match obj.getattr("__fspath__") {
        Ok(func) => {
            match func.call0() {
                Ok(o) => pyobj2str(&o),
                Err(_) => Err(format!("expected str, bytes or os.PathLike object, not '{}'", obj.get_type().name().unwrap())),
            }
        },
        Err(_) => Err(format!("expected str, bytes or os.PathLike object, not '{}'", obj.get_type().name().unwrap())),
    }
}
