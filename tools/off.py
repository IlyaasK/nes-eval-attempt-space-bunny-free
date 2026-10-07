import sys, numpy as np, os
rd,md,fr=sys.argv[1],sys.argv[2],int(sys.argv[3])
def load(p):
    try: return np.fromfile(p,dtype='<u2').reshape(240,256)
    except: return None
tot=0
for f in range(fr):
    a=load('%s/frame_%05d.idx'%(rd,f))
    if a is None: continue
    best=(10**9,0)
    for g in range(max(0,f-6),min(fr,f+7)):
        b=load('%s/frame_%05d.idx'%(md,g))
        if b is None: continue
        d=int((a!=b).sum())
        if d<best[0]: best=(d,g-f)
    tot+=best[0]
    if f<40: print('ref f%03d best off %+d diff %d'%(f,best[1],best[0]))
print('TOTAL best-offset px:',tot)
