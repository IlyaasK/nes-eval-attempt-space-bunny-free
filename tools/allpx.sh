#!/bin/bash
cd /task
FR=${FR:-60}
tot=0
for f in dev-roms/test/*.nes dev-roms/*.nes; do
  b=$(basename $f .nes | tr -c "A-Za-z0-9_" "_" | cut -c1-26)
  out=$(bash tools/cmp.sh $f $FR 2>/dev/null | tail -1)
  echo "$out"
  n=$(echo "$out" | sed 's/.*total px //')
  tot=$((tot+n))
done
echo "GRAND TOTAL PX: $tot"
