# fpath [![](https://travis-ci.org/hhatto/fpath.svg?branch=master)](https://travis-ci.org/hhatto/fpath)
Python's os.path module written in Rust


## Requirements

* Python3
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

```
methodname              %        real[p,r]        user[p,r]        sys[p,r]       n
abspath            45.53%   10.15s,  5.53s    6.86s,  2.81s   3.25s,  2.69s  100000
basename           53.52%    0.71s,  0.33s    0.70s,  0.33s   0.00s,  0.00s  100000
dirname            57.43%    1.02s,  0.43s    1.01s,  0.43s   0.00s,  0.00s  100000
isabs              56.55%    0.59s,  0.25s    0.59s,  0.25s   0.00s,  0.00s  100000
islink              0.25%    3.78s,  3.77s    0.01s,  0.01s   0.01s,  0.00s      50
exists              0.25%    3.78s,  3.77s    0.01s,  0.01s   0.01s,  0.00s      50
lexists             0.70%    3.77s,  3.74s    0.01s,  0.01s   0.01s,  0.00s      50
split              53.85%    1.17s,  0.54s    1.17s,  0.54s   0.00s,  0.00s  100000
splitext           62.02%    1.22s,  0.46s    1.21s,  0.46s   0.00s,  0.00s  100000
relpath            52.97%    0.02s,  0.01s    0.01s,  0.01s   0.01s,  0.00s      50
normpath           57.27%    2.02s,  0.86s    2.01s,  0.86s   0.00s,  0.00s  100000
realpath            1.08%   13.39s, 13.25s    0.05s,  0.02s   0.02s,  0.03s      50
join               23.01%    0.24s,  0.19s    0.24s,  0.18s   0.00s,  0.00s  100000
expanduser         67.45%    1.50s,  0.49s    1.49s,  0.48s   0.00s,  0.00s  100000
expandvars         61.37%    1.21s,  0.47s    1.19s,  0.47s   0.00s,  0.00s  100000
```
