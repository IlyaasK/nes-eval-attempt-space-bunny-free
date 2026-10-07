#!/usr/bin/env python3
import numpy as np, sys, os, glob
pal=np.frombuffer(open('/task/spec/nes_palette.pal','rb').read(),dtype=np.uint8).reshape(-1,3)
def load(p): return np.fromfile(p,dtype='<u2').reshape(240,256)
# args: refdir minedir frames...
rd,md=sys.argv[1],sys.argv[2]
frames=[int(x) for x in sys.argv[3:]]
imgs=[]
for f in frames:
    for d in (rd,md):
        p='%s/frame_%05d.idx'%(d,f)
        if os.path.exists(p):
            a=load(p); imgs.append(pal[a&0x1ff].reshape(240,256,3))
        else:
            imgs.append(np.zeros((240,256,3),dtype=np.uint8))
import zlib,struct
def writepng(path,img):
    h,w,_=img.shape
    raw=b''.join(b'\x00'+img[y].tobytes() for y in range(h))
    def chunk(t,data):
        c=t+data
        return struct.pack('>I',len(data))+c+struct.pack('>I',zlib.crc32(c)&0xffffffff)
    open(path,'wb').write(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(raw))+chunk(b'IEND',b''))
writepng('/tmp/cmp.png', np.concatenate(imgs,axis=1))
print('/tmp/cmp.png', len(imgs))