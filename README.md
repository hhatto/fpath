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
methodname              %      real[p,r]        user[p,r]       sys[p,r]
abspath            49.22%    5.96s,  3.02s    4.28s,  1.67s   1.67s,  1.33s
basename           60.68%    0.59s,  0.23s    0.58s,  0.23s   0.00s,  0.00s
dirname            57.63%    0.75s,  0.32s    0.75s,  0.32s   0.00s,  0.00s
isabs              60.20%    0.45s,  0.18s    0.45s,  0.18s   0.00s,  0.00s
islink              1.05%    0.80s,  0.79s    0.01s,  0.00s   0.00s,  0.00s
exists              1.05%    0.80s,  0.79s    0.01s,  0.00s   0.00s,  0.00s
lexists            -1.50%    0.77s,  0.78s    0.00s,  0.00s   0.00s,  0.00s
split              54.59%    0.84s,  0.38s    0.84s,  0.38s   0.01s,  0.00s
splitext           62.86%    0.90s,  0.33s    0.89s,  0.33s   0.00s,  0.00s
relpath            43.98%   25.70s, 14.40s   18.78s,  8.63s   6.83s,  5.73s
normpath           55.92%    1.61s,  0.71s    1.60s,  0.71s   0.00s,  0.01s
realpath           -0.35%    2.72s,  2.73s    0.01s,  0.01s   0.01s,  0.00s
join               25.72%    0.25s,  0.19s    0.25s,  0.19s   0.00s,  0.00s
```
