import os.path as ospath
import _fpath


def abspath(path):
    ret = _fpath.abspath(path)
    if type(path) == bytes:
        return ret.encode("utf-8")
    return ret

def basename(path):
    ret = _fpath.basename(path)
    if type(path) == bytes:
        return ret.encode("utf-8")
    return ret

def join(path, *paths):
    ret = _fpath.join(path, *paths)
    if type(path) == bytes:
        return ret.encode("utf-8")
    return ret

def realpath(path):
    ret = _fpath.realpath(path)
    if type(path) == bytes:
        return ret.encode("utf-8")
    return ret

def dirname(path):
    ret = _fpath.dirname(path)
    if type(path) == bytes:
        return ret.encode("utf-8")
    return ret

def isabs(path):
    return _fpath.isabs(path)

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

def split(path):
    head, tail = _fpath.split(path)
    if type(path) == bytes:
        return head.encode("utf-8"), tail.encode("utf-8")
    return head, tail

def splitext(path):
    f, ext = _fpath.splitext(path)
    if type(path) == bytes:
        return f.encode("utf-8"), ext.encode("utf-8")
    return f, ext

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

def normpath(path):
    return ospath.normpath(path)
