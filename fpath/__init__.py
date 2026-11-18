import os.path as ospath

from . import _fpath

# Paths that fpath does not handle itself, e.g. a str with lone surrogates or
# bytes that are not valid UTF-8, are passed to os.path.
abspath = _fpath.abspath
basename = _fpath.basename
commonpath = _fpath.commonpath
realpath = _fpath.realpath
dirname = _fpath.dirname
isabs = _fpath.isabs
split = _fpath.split
splitext = _fpath.splitext
expanduser = _fpath.expanduser
expandvars = _fpath.expandvars
exists = _fpath.exists
islink = _fpath.islink
relpath = _fpath.relpath

# Sentinels for realpath(strict=...): ALLOW_MISSING (Python 3.13.4+) and
# ALL_BUT_LAST (Python 3.15+).
if hasattr(ospath, "ALLOW_MISSING"):
    ALLOW_MISSING = ospath.ALLOW_MISSING
if hasattr(ospath, "ALL_BUT_LAST"):
    ALL_BUT_LAST = ospath.ALL_BUT_LAST


def join(path, *args):
    return _fpath.join(path, args)


# not support methods by fpath module
commonprefix = ospath.commonprefix
curdir = ospath.curdir
pardir = ospath.pardir
extsep = ospath.extsep
sep = ospath.sep
pathsep = ospath.pathsep
defpath = ospath.defpath
altsep = ospath.altsep
devnull = ospath.devnull
getatime = ospath.getatime
getctime = ospath.getctime
getmtime = ospath.getmtime
getsize = ospath.getsize
ismount = ospath.ismount
isfile = ospath.isfile
isdir = ospath.isdir
isdevdrive = ospath.isdevdrive
isjunction = ospath.isjunction
lexists = ospath.lexists
normcase = ospath.normcase
# os.path.normpath is implemented in C, which is as fast as or faster than a
# Rust implementation called through PyO3.
normpath = ospath.normpath
samefile = ospath.samefile
sameopenfile = ospath.sameopenfile
samestat = ospath.samestat
splitdrive = ospath.splitdrive
splitroot = ospath.splitroot
supports_unicode_filenames = ospath.supports_unicode_filenames
