#!/usr/bin/env zsh

for funcname in "abspath" "basename" "dirname" "isabs" "islink" \
                "exists" "lexists" "split" "splitext" "relpath" \
                "normpath" "realpath" "join"
do
    python benchmarks.py -o result.txt $funcname >& /dev/null
    printf '%-15s %5.2f%%\n' $funcname $(cat result.txt | jq -r '(.Result[0].real[0] - .Result[1].real[0]) / .Result[0].real[0] * 100')
done
rm result.txt
