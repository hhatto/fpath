use pyo3::prelude::*;

pub fn pyobj2str(obj: &PyObject) -> Result<(String, bool), String> {
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

pub fn pypathlike2str(obj: &PyObject) -> Result<(String, bool), String> {
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

