import os
import os.path as ospath

from . import _fpath


def abspath(path):
    if isinstance(path, bytes):
        return ospath.abspath(path)
    return _fpath.abspath(path)


basename = _fpath.basename
realpath = _fpath.realpath
dirname = _fpath.dirname
isabs = _fpath.isabs
normpath = _fpath.normpath
split = _fpath.split
splitext = _fpath.splitext
expanduser = _fpath.expanduser
expandvars = _fpath.expandvars


def islink(path):
    if isinstance(path, bytes):
        return ospath.islink(path)
    return _fpath.islink(path)


def join(path, *args):
    return _fpath.join(path, args)


def exists(path):
    if type(path) == int:
        return ospath.exists(path)
    if (type(path) is bytes and b'\x00' in path) or (type(path) is str and '\x00' in path):
        return ospath.exists(path)
    try:
        return _fpath.exists(path)
    except Exception:
        return ospath.exists(path)


def relpath(path, start=None):
    if not path:
        raise ValueError("no path specified")
    path_type = type(path)
    start_type = type(start)
    if path_type is str and (start is not None and start_type is bytes):
        raise(TypeError("Can't mix strings and bytes in path components"))
    elif path_type is bytes and (start is not None and start_type is str):
        raise(TypeError("Can't mix strings and bytes in path components"))
    if start is None:
        start = "."
    else:
        os.fspath(start)
    try:
        ret = _fpath.relpath(path, start)
    except TypeError:
        raise(TypeError('"%s" does not match "%s"' % (path_type, start_type)))
    return ret


# not support methods by fpath module
commonpath = ospath.commonpath
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
lexists = ospath.lexists
normcase = ospath.normcase
samefile = ospath.samefile
sameopenfile = ospath.sameopenfile
samestat = ospath.samestat
splitdrive = ospath.splitdrive
