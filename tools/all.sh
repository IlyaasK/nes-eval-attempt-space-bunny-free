#!/bin/bash
# Run all dev test roms + games and report frame-diff summary.
cd /task
FR=${FR:-90}
for f in dev-roms/test/*.nes dev-roms/*.nes; do
  b=$(basename $f .nes | tr -c "A-Za-z0-9_" "_" | cut -c1-26)
  out=$(bash tools/cmp.sh $f $FR 2>/dev/null | tail -1)
  echo "$out"
done