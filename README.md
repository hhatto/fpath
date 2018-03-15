# fpath
Python's os.path module written in Rust


## Requirements

* [setuptools-rust](https://github.com/PyO3/setuptools-rust)
* Nightly Rust

```
$ pip install setuptools_rust
$ rustup default nightly
```


## Installation

```
$ pip install --upgrade git+https://github.com/hhatto/fpath
```

## Benchmark

```
method name      faster than os.path
------------------------------------
abspath                       37.99%
basename                      -3.64%
dirname                       18.73%
isabs                         33.62%
islink                        -0.54%
exists                         0.20%
lexists                        1.98%
split                         20.80%
splitext                      13.01%
relpath                       43.72%
normpath                      -8.20%
realpath                       3.22%
join                         -23.21%
```
[benchmark](https://gist.github.com/hhatto/d6fd0c30def0c0632c7c9b0b4c2d7a79)
