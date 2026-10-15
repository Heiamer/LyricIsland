"""Embed the existing app ICO in a PE without changing its manifest."""
from pathlib import Path
import ctypes as c,struct,sys

def embed_icon(executable,icon):
    data=Path(icon).read_bytes()
    reserved,kind,count=struct.unpack_from('<HHH',data)
    if reserved or kind!=1 or not count:raise ValueError('Invalid ICO')
    api=c.WinDLL('kernel32',use_last_error=True)
    api.BeginUpdateResourceW.argtypes=[c.c_wchar_p,c.c_int];api.BeginUpdateResourceW.restype=c.c_void_p
    api.UpdateResourceW.argtypes=[c.c_void_p,c.c_void_p,c.c_void_p,c.c_ushort,c.c_void_p,c.c_ulong]
    api.EndUpdateResourceW.argtypes=[c.c_void_p,c.c_int]
    handle=api.BeginUpdateResourceW(str(Path(executable).resolve()),False)
    if not handle:raise c.WinError(c.get_last_error())
    def update(kind,number,blob):
        buffer=c.create_string_buffer(blob)
        if not api.UpdateResourceW(handle,kind,number,1033,buffer,len(blob)):raise c.WinError(c.get_last_error())
    try:
        group=bytearray(struct.pack('<HHH',0,1,count))
        for i in range(count):
            w,h,colors,reserved,planes,bits,size,offset=struct.unpack_from('<BBBBHHII',data,6+i*16)
            blob=data[offset:offset+size]
            if len(blob)!=size:raise ValueError('Truncated ICO image')
            update(3,i+1,blob)
            group+=struct.pack('<BBBBHHIH',w,h,colors,reserved,planes,bits,size,i+1)
        update(14,1,bytes(group))
    except BaseException:
        api.EndUpdateResourceW(handle,True);raise
    if not api.EndUpdateResourceW(handle,False):raise c.WinError(c.get_last_error())

if __name__=='__main__':embed_icon(sys.argv[1],sys.argv[2])
