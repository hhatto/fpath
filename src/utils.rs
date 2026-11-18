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
            PyTuple::new($py, [py_head, py_tail]).map(|t| t.into_any().unbind())
        }
    }
}
