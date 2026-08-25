#!/usr/bin/env bash
set -euo pipefail

target="${1:-}"
case "$target" in
  parse_module|parse_validate) ;;
  *)
    echo "usage: bash fuzz/render-coverage.sh <parse_module|parse_validate>" >&2
    exit 2
    ;;
esac

host="$(rustc +nightly -vV | sed -n 's/^host: //p')"
if [[ -z "$host" ]]; then
  echo "failed to determine nightly host triple" >&2
  exit 1
fi

sysroot="$(rustc +nightly --print sysroot)"
llvm_cov="$sysroot/lib/rustlib/$host/bin/llvm-cov"
profile="fuzz/coverage/$target/coverage.profdata"
binary="target/$host/coverage/$host/release/$target"
report_dir="fuzz/coverage/$target/report"
html_dir="$report_dir/html"
ignore_regex='/([.]cargo/registry|rustc)/'

if [[ ! -x "$llvm_cov" ]]; then
  echo "llvm-cov is unavailable at $llvm_cov; install nightly llvm-tools-preview" >&2
  exit 1
fi
if [[ ! -s "$profile" ]]; then
  echo "coverage profile is missing or empty: $profile" >&2
  exit 1
fi
if [[ ! -x "$binary" ]]; then
  echo "coverage-instrumented fuzz target is missing or not executable: $binary" >&2
  exit 1
fi

rm -rf "$report_dir"
mkdir -p "$html_dir"

"$llvm_cov" report "$binary" \
  --instr-profile="$profile" \
  --ignore-filename-regex="$ignore_regex" \
  > "$report_dir/summary.txt"

"$llvm_cov" export "$binary" \
  --instr-profile="$profile" \
  --ignore-filename-regex="$ignore_regex" \
  --format=lcov \
  > "$report_dir/lcov.info"

"$llvm_cov" show "$binary" \
  --instr-profile="$profile" \
  --ignore-filename-regex="$ignore_regex" \
  --format=html \
  --output-dir="$html_dir"

for required in "$report_dir/summary.txt" "$report_dir/lcov.info" "$html_dir/index.html"; do
  if [[ ! -s "$required" ]]; then
    echo "coverage report output is missing or empty: $required" >&2
    exit 1
  fi
done

echo "coverage_report=$report_dir target=$target host=$host"
