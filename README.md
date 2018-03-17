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
methodname              %        real[p,r]        user[p,r]        sys[p,r]       n
abspath            50.00%    0.00s,  0.00s    0.00s,  0.00s   0.00s,  0.00s      10
basename           52.36%    0.77s,  0.37s    0.76s,  0.36s   0.01s,  0.00s  100000
dirname            57.26%    1.05s,  0.45s    1.04s,  0.44s   0.01s,  0.00s  100000
isabs              54.53%    0.58s,  0.27s    0.58s,  0.26s   0.00s,  0.01s  100000
islink             -0.67%    0.79s,  0.79s    0.00s,  0.00s   0.00s,  0.00s      10
exists             -0.67%    0.79s,  0.79s    0.00s,  0.00s   0.00s,  0.00s      10
lexists             1.93%    0.79s,  0.78s    0.00s,  0.00s   0.00s,  0.00s      10
split              52.87%    1.18s,  0.56s    1.17s,  0.56s   0.01s,  0.00s  100000
splitext           64.39%    1.28s,  0.46s    1.27s,  0.45s   0.01s,  0.00s  100000
relpath            48.72%    0.00s,  0.00s    0.00s,  0.00s   0.00s,  0.00s      10
normpath           53.75%    2.16s,  1.00s    2.13s,  0.99s   0.02s,  0.00s  100000
realpath            4.28%    2.87s,  2.75s    0.01s,  0.00s   0.01s,  0.00s      10
join               22.73%    0.25s,  0.19s    0.25s,  0.19s   0.00s,  0.01s  100000
expanduser         67.97%    1.54s,  0.49s    1.53s,  0.49s   0.01s,  0.00s  100000
expandvars          0.82%    1.14s,  1.13s    1.14s,  1.12s   0.01s,  0.00s  100000
```
