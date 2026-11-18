# fpath [![build](https://github.com/hhatto/fpath/actions/workflows/python-package.yml/badge.svg?branch=main)](https://github.com/hhatto/fpath/actions/workflows/python-package.yml)
Python's os.path module written in Rust


## Requirements

* Python 3.13 - 3.15
* Rust 1.88 or later (stable)

```
$ rustup default stable
```


## Installation

```
$ pip install --upgrade git+https://github.com/hhatto/fpath
```

## Usage

```python
>>> import os.path
>>> import fpath
>>> os.path.abspath("path/to/file")
'/home/user/path/to/file'
>>> fpath.abspath("path/to/file")
'/home/user/path/to/file'
>>> import timeit
>>> timeit.timeit('import os.path;os.path.abspath("path/to/file")', number=1000*10)
0.20972810598323122
>>> timeit.timeit('import fpath;fpath.abspath("path/to/file")', number=1000*10)
0.12387347500771284
>>>
```

## Benchmark

Python 3.14, macOS (Apple Silicon)

```
methodname              %        real[p,r]        user[p,r]        sys[p,r]       n
abspath             1.72%    3.83s,  3.77s    0.43s,  0.41s   3.39s,  3.36s  100000
basename           69.51%    0.09s,  0.03s    0.09s,  0.03s   0.00s,  0.00s  100000
dirname            71.74%    0.15s,  0.04s    0.15s,  0.05s   0.00s,  0.00s  100000
isabs              70.63%    0.06s,  0.02s    0.07s,  0.02s   0.00s,  0.00s  100000
islink             44.34%    0.05s,  0.03s    0.03s,  0.01s   0.02s,  0.03s   10000
exists             45.76%    0.05s,  0.03s    0.03s,  0.01s   0.03s,  0.02s   10000
split              63.31%    0.18s,  0.07s    0.18s,  0.06s   0.00s,  0.00s  100000
splitext           70.31%    0.18s,  0.05s    0.18s,  0.05s   0.00s,  0.00s  100000
relpath             2.94%    1.25s,  1.22s    0.21s,  0.16s   1.04s,  1.05s   10000
realpath           27.56%    0.77s,  0.55s    0.21s,  0.07s   0.51s,  0.48s   10000
join               28.40%    0.02s,  0.02s    0.03s,  0.01s   0.00s,  0.00s  100000
expanduser         71.82%    0.17s,  0.05s    0.17s,  0.04s   0.00s,  0.00s  100000
expandvars         84.97%    0.26s,  0.04s    0.26s,  0.04s   0.00s,  0.00s  100000
commonpath         78.35%    0.23s,  0.05s    0.24s,  0.05s   0.00s,  0.00s  100000
```

## For Developer

install dev dependencies:

```console
$ pip install ".[dev]"
```

build and install:

```console
$ maturin build
$ pip install .
```

run tests:

```
$ pytest tests/
```
