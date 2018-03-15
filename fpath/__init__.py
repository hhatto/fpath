import os.path as ospath
import _fpath


def abspath(path):
    ret = _fpath.abspath(path)
    if type(path) == bytes:
        return ret.encode("utf-8")
    return ret

basename = _fpath.basename

join = _fpath.join

realpath = _fpath.realpath

dirname = _fpath.dirname

def isabs(path):
    return _fpath.isabs(path)

def normpath(path):
    ret = _fpath.normpath(path)
    if type(path) == bytes:
        return ret.encode("utf-8")
    return ret

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

split = _fpath.split
splitext = _fpath.splitext

# not support methods by fpath module
def commonpath(paths):
    return ospath.commonpath(paths)

def commonprefix(m):
    return _fpath.commonprefix(m)

def exists(path):
    return ospath.exists(path)

def expanduser(path):
    return ospath.expanduser(path)

def ismount(path):
    return ospath.ismount(path)

def islink(path):
    return ospath.islink(path)

def lexists(path):
    return ospath.lexists(path)
