#!/bin/bash
ROM=$1
FR=$2
REP=$3
BASE=$(basename $ROM .nes | tr -c "A-Za-z0-9_" "_" | cut -c1-30)
R=ref_$BASE
M=/tmp/mine_$BASE
rm -rf $M; mkdir -p $M
if [ -n "$REP" ]; then
  oracle run $ROM $FR --dump-frames $R --replay $REP >/dev/null
  /task/target/release/examples/dump $ROM $FR $M $REP >/dev/null 2>&1
else
  oracle run $ROM $FR --dump-frames $R >/dev/null
  /task/target/release/examples/dump $ROM $FR $M >/dev/null 2>&1
fi
python3 - "$R" "$M" "$FR" "$BASE" <<'PY'
import sys, numpy as np, os
rd,md,fr,base=sys.argv[1],sys.argv[2],int(sys.argv[3]),sys.argv[4]
def load(p): return np.fromfile(p,dtype='<u2').reshape(240,256)
bad=0; line=[]
for f in range(fr):
    ra='%s/frame_%05d.idx'%(rd,f); mi='%s/frame_%05d.idx'%(md,f)
    if not os.path.exists(ra) or not os.path.exists(mi): continue
    a=load(ra); b=load(mi); d=(a!=b).sum()
    if d: bad+=1
    line.append('f%03d:%d'%(f,d))
print(' '.join(line))
print('%s: %d/%d frames differ'%(base,bad,fr))
PY
