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
methodname              %       real[p,r]       user[p,r]      sys[p,r]
abspath            43.62%    7.32s,  4.13s    4.85s,  2.33s   1.92s,  1.69s
basename           29.11%    0.78s,  0.55s    0.69s,  0.54s   0.00s,  0.01s
dirname            26.71%    0.95s,  0.69s    0.88s,  0.66s   0.01s,  0.01s
isabs              19.15%    0.51s,  0.41s    0.47s,  0.35s   0.01s,  0.01s
islink             -5.06%    0.91s,  0.95s    0.01s,  0.00s   0.00s,  0.00s
exists              5.25%    1.09s,  1.03s    0.00s,  0.00s   0.01s,  0.00s
lexists             3.06%    0.92s,  0.89s    0.00s,  0.00s   0.00s,  0.01s
split              32.49%    1.15s,  0.78s    0.99s,  0.76s   0.02s,  0.00s
splitext           21.28%    0.97s,  0.76s    0.95s,  0.71s   0.01s,  0.01s
relpath            45.51%   26.50s, 14.44s   19.29s,  8.54s   6.94s,  5.82s
normpath           42.38%    1.62s,  0.93s    1.61s,  0.93s   0.01s,  0.00s
realpath           -0.30%    2.77s,  2.78s    0.01s,  0.01s   0.01s,  0.00s
join              -24.37%    0.25s,  0.32s    0.26s,  0.31s   0.00s,  0.00s
```
[benchmark](https://gist.github.com/hhatto/d6fd0c30def0c0632c7c9b0b4c2d7a79)
