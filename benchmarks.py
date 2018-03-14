import sys
import os
import fpath
from benchmarker import Benchmarker

N = 1000 * 100
FILENAME = "file.txt"
B_FILE_PATH = b"/home/user/path/to/file.txt"
FILE_PATH = "/home/user/path/to/file.txt"
DIR_PATH = "/home/user/path/to"
B_DIR_PATH = b"/home/user/path/to"
SEPEND_DIR_PATH = "/home/user/path/to/"

def bench_one_arg(arg):
    for funcname in ("abspath", "basename", "dirname", "isabs", "islink",
                     "exists", "lexists", "split", "splitext", "relpath",
                     "normpath", "realpath"):
        if arg is not None and funcname != arg:
            continue
        # benchmark of file system dependent
        n = N if funcname not in ("islink", "lexists", "exists", "realpath") else 10
        with Benchmarker(n, width=30) as b:
            @b("naitive.%s" % (funcname))
            def _(bm):
                func = getattr(os.path, funcname)
                for i in bm:
                    func(B_FILE_PATH)
                    func(FILENAME)
                    func(FILE_PATH)
                    func(DIR_PATH)
                    func(SEPEND_DIR_PATH)

            @b("rust.%s" % (funcname))
            def _(bm):
                func = getattr(fpath, funcname)
                for i in bm:
                    func(B_FILE_PATH)
                    func(FILENAME)
                    func(FILE_PATH)
                    func(DIR_PATH)
                    func(SEPEND_DIR_PATH)
        print("=*=" * 40)


def bench_two_arg(arg):
    for funcname in ("join", ):
        if arg is not None and funcname != arg:
            continue
        n = N
        with Benchmarker(n, width=30) as b:
            @b("naitive.%s" % (funcname))
            def _(bm):
                func = getattr(os.path, funcname)
                for i in bm:
                    func(DIR_PATH, FILE_PATH)
                    func(B_DIR_PATH, B_FILE_PATH)

            @b("rust.%s" % (funcname))
            def _(bm):
                func = getattr(fpath, funcname)
                for i in bm:
                    func(DIR_PATH, FILE_PATH)
                    func(B_DIR_PATH, B_FILE_PATH)
        print("=*=" * 40)

arg = sys.argv[1] if len(sys.argv) == 2 else None
bench_one_arg(arg)
bench_two_arg(arg)
