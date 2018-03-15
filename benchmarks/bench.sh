#!/usr/bin/env zsh

printf 'methodname              %%      real[p,r]        user[p,r]       sys[p,r]\n'

for funcname in "abspath" "basename" "dirname" "isabs" "islink" \
                "exists" "lexists" "split" "splitext" "relpath" \
                "normpath" "realpath" "join"
do
    python benchmarks.py -o result.txt $funcname >& /dev/null
    percent=$(cat result.txt | jq -r '(.Result[0].real[0] - .Result[1].real[0]) / .Result[0].real[0] * 100')
    native_real=$(cat result.txt | jq -r '.Result[0].real[0]')
    rust_real=$(cat result.txt | jq -r '.Result[1].real[0]')
    native_user=$(cat result.txt | jq -r '.Result[0].user[0]')
    rust_user=$(cat result.txt | jq -r '.Result[1].user[0]')
    native_sys=$(cat result.txt | jq -r '.Result[0].sys[0]')
    rust_sys=$(cat result.txt | jq -r '.Result[1].sys[0]')
    printf '%-15s %8.2f%%  %6.2fs,%6.2fs  %6.2fs,%6.2fs %6.2fs,%6.2fs\n' $funcname $percent $native_real $rust_real $native_user $rust_user $native_sys $rust_sys
done
rm result.txt
