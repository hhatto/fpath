use pyo3::prelude::*;

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
    ( $s:expr, $is_bytes:expr ) => {
        {
            let gil = Python::acquire_gil();
            let py = gil.python();
            if $is_bytes {
                Ok(PyBytes::new(py, $s.as_bytes()).to_object(py))
            } else {
                Ok(PyString::new(py, $s).to_object(py))
            }
        }
    }
}

macro_rules! tuplestr2pyobj {
    ( $head:expr, $tail:expr, $is_bytes:expr ) => {
        {
            let gil = Python::acquire_gil();
            let py = gil.python();
            let (py_head, py_tail) = if $is_bytes {
                (
                    PyBytes::new(py, $head.as_bytes()).to_object(py),
                    PyBytes::new(py, $tail.as_bytes()).to_object(py),
                )
            } else {
                (
                    PyString::new(py, $head).to_object(py),
                    PyString::new(py, $tail).to_object(py),
                )
            };
            Ok(PyTuple::new(py, &[py_head, py_tail]).to_object(py))
        }
    }
}

pub fn pyobj2str(obj: &PyObject) -> Result<(String, bool), String> {
    let gil = Python::acquire_gil();
    let py = gil.python();
    match obj.extract::<String>(py) {
        Ok(s) => Ok((s, false)),
        Err(_) => match obj.extract::<&PyBytes>(py) {
            Ok(arg) => {
                let s = String::from_utf8(arg.data().to_vec());
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

pub fn pypathlike2str(obj: &PyObject) -> Result<(String, bool), String> {
    let gil = Python::acquire_gil();
    let py = gil.python();
    match obj.getattr(py, "__fspath__") {
        Ok(func) => {
            match func.call0(py) {
                Ok(o) => pyobj2str(&o),
                Err(_) => Err("expected str, bytes or os.PathLike object".to_string()),
            }
        },
        Err(_) => Err("expected str, bytes or os.PathLike object".to_string()),
    }
}
