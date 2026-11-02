# N-way falsifier questions

Reference variant: `quick` (`../quick.txt`). Other variants: `heap`, `merge`.
Readers see only `pairwise-heap.md` and `pairwise-merge.md` (no artifact) and
answer Q1-Q3 against a stopwatch.

## Questions

1. Which variant has the best overall result, and against which reference?
2. Is the gap on `Parse [ns/op]` between the reference and `heap` significant?
3. Is any variant slower than the reference?

## Answer key

Derived from the committed pairwise outputs (`benchdiff` unmodified build).

1. `heap`, against reference `quick`: 4 rows significantly faster
   (`Sort/n=1000` -30.6%, `Join` -28.8%, `Hash` -28.3%, `Sort/n=100000` -25.1%,
   all p<0.001) and none slower. `merge` has 1 improved (`Sort/n=1000` -9.4%)
   and 1 regressed row.
2. No. `Parse [ns/op]`, quick 4150 vs heap 4154 ns/op: +0.1%, p=0.529.
3. Yes. `merge` is slower than `quick` on `Parse [ns/op]`: 4150 to 5417 ns/op,
   +30.5%, p<0.001. `heap` has no slower row.

## Regeneration

From the repository root, after `cargo build --release -p benchdiff`:

```sh
cd testdata/nway
../../target/release/benchdiff --base quick.txt --head heap.txt  --markdown baseline/pairwise-heap.md
../../target/release/benchdiff --base quick.txt --head merge.txt --markdown baseline/pairwise-merge.md
```

Output is deterministic; rerunning must leave `git diff` empty.
