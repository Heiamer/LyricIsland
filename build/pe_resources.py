"""Add the application icon and named RCDATA to unsigned Windows executables."""
from pathlib import Path
import ctypes as c
import struct

def update_resources(executable, resources):
    api=c.WinDLL('kernel32',use_last_error=True)
    api.BeginUpdateResourceW.argtypes=[c.c_wchar_p,c.c_int]
    api.BeginUpdateResourceW.restype=c.c_void_p
    api.UpdateResourceW.argtypes=[c.c_void_p,c.c_void_p,c.c_void_p,c.c_ushort,c.c_void_p,c.c_ulong]
    api.UpdateResourceW.restype=c.c_int
    api.EndUpdateResourceW.argtypes=[c.c_void_p,c.c_int]
    api.EndUpdateResourceW.restype=c.c_int
    handle=api.BeginUpdateResourceW(str(Path(executable).resolve()),False)
    if not handle: raise c.WinError(c.get_last_error())
    try:
        for kind,name,blob in resources:
            name_buffer=c.create_unicode_buffer(name) if isinstance(name,str) else None
            name_ptr=c.cast(name_buffer,c.c_void_p) if name_buffer is not None else c.c_void_p(name)
            data=c.create_string_buffer(blob)
            if not api.UpdateResourceW(handle,c.c_void_p(kind),name_ptr,1033,data,len(blob)):
                raise c.WinError(c.get_last_error())
    except BaseException:
        api.EndUpdateResourceW(handle,True)
        raise
    if not api.EndUpdateResourceW(handle,False): raise c.WinError(c.get_last_error())

def embed_icon(executable,icon):
    data=Path(icon).read_bytes()
    reserved,kind,count=struct.unpack_from('<HHH',data)
    if reserved or kind!=1 or not count: raise ValueError('Invalid ICO')
    resources=[]
    group=bytearray(struct.pack('<HHH',0,1,count))
    for i in range(count):
        w,h,colors,reserved,planes,bits,size,offset=struct.unpack_from('<BBBBHHII',data,6+i*16)
        blob=data[offset:offset+size]
        if len(blob)!=size: raise ValueError('Truncated ICO')
        resources.append((3,i+1,blob))
        group+=struct.pack('<BBBBHHIH',w,h,colors,reserved,planes,bits,size,i+1)
    resources.append((14,1,bytes(group)))
    update_resources(executable,resources)
