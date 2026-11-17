use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

macro_rules! numsep {
    ( $x:expr ) => (
        unsafe { String::from_utf8_unchecked((0..$x).map(|_| SEP).collect::<Vec<u8>>()) }
    )
}

macro_rules! str2pyobj {
    ( $py:expr, $s:expr, $is_bytes:expr ) => {
        {
            if $is_bytes {
                Ok(PyBytes::new($py, $s.as_bytes()).into_any().unbind())
            } else {
                Ok(PyString::new($py, $s).into_any().unbind())
            }
        }
    }
}

macro_rules! tuplestr2pyobj {
    ( $py:expr, $head:expr, $tail:expr, $is_bytes:expr ) => {
        {
            let (py_head, py_tail) = if $is_bytes {
                (
                    PyBytes::new($py, $head.as_bytes()).into_any(),
                    PyBytes::new($py, $tail.as_bytes()).into_any(),
                )
            } else {
                (
                    PyString::new($py, $head).into_any(),
                    PyString::new($py, $tail).into_any(),
                )
            };
            Ok(PyTuple::new($py, [py_head, py_tail])?.into_any().unbind())
        }
    }
}

pub fn pyobj2str(obj: &Bound<'_, PyAny>) -> Result<(String, bool), String> {
    match obj.cast::<PyString>() {
        Ok(s) => Ok((s.to_string(), false)),
        Err(_) => match obj.cast::<PyBytes>() {
            Ok(arg) => {
                let s = String::from_utf8(arg.as_bytes().to_vec());
                match s {
                    Err(e) => return Err(format!("undecoded data: {:?}", e)),
                    _ => {},
                }
                let s = s.unwrap();
                Ok((s, true))
            },
            Err(_) => pypathlike2str(obj),
        },
    }
}

fn type_name(obj: &Bound<'_, PyAny>) -> String {
    match obj.get_type().name() {
        Ok(name) => name.to_string(),
        Err(_) => "<unknown>".to_string(),
    }
}

pub fn pypathlike2str(obj: &Bound<'_, PyAny>) -> Result<(String, bool), String> {
    match obj.getattr("__fspath__") {
        Ok(func) => {
            match func.call0() {
                Ok(o) => pyobj2str(&o),
                Err(_) => Err(format!("expected str, bytes or os.PathLike object, not '{}'", type_name(obj))),
            }
        },
        Err(_) => Err(format!("expected str, bytes or os.PathLike object, not '{}'", type_name(obj))),
    }
}
