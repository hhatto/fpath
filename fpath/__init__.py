import os.path as ospath
import _fpath


abspath = _fpath.abspath
basename = _fpath.basename
join = _fpath.join
realpath = _fpath.realpath
dirname = _fpath.dirname
isabs = _fpath.isabs
islink = _fpath.islink
normpath = _fpath.normpath
split = _fpath.split
splitext = _fpath.splitext

def exists(path):
    if type(path) == int:
        return ospath.exists(path)
    if '\x00' in path:
        raise ValueError("embedded null byte")
    return _fpath.exists(path)

def relpath(path, start=None):
    if not path:
        raise ValueError("no path specified")
    if type(path) is str and (start is not None and type(start) is not str):
        raise(TypeError("must be str or None, not bytes"))
    elif type(path) is bytes and (start is not None and type(start) is not bytes):
        raise(TypeError("a bytes-like object is required, not 'str"))
    if start is None:
        start = "."
    ret = _fpath.relpath(path, start)
    if type(path) == bytes:
        return ret.encode("utf-8")
    return ret

# not support methods by fpath module
commonpath = ospath.commonpath
commonprefix = ospath.commonprefix
expanduser = ospath.expanduser
expandvars = ospath.expandvars
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
