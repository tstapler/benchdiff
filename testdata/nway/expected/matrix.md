<!-- benchdiff:default -->
### Benchmark comparison: 3 variants (reference: quick)

**heap** has the best geomean (0.80x vs reference quick, over 6/6 common ns/op rows; variants are not tested against each other); slower than reference in 0 of 6 rows

reference: quick (set by 'reference')

| Benchmark | quick (ref) | heap | merge | Rank |
|---|---|---|---|---|
| `sortbench/Parse (ns/op)` | **4150 ns/op** | 4154 ns/op (1.00x) ~ | 5417 ns/op (1.31x) 🔴 | quick |
| `sortbench/Sort [n=1000] (ns/op)` | 8916 ns/op | **6190 ns/op** (0.69x) 🟢 | 8076 ns/op (0.91x) 🟢 | heap |
| `sortbench/Join (ns/op)` | 25686 ns/op | **18296 ns/op** (0.71x) 🟢 | 25908 ns/op (1.01x) ~ | heap |
| `sortbench/Hash (ns/op)` | 782.5 ns/op | **560.9 ns/op** (0.72x) 🟢 | 779.6 ns/op (1.00x) ~ | heap |
| `sortbench/Sort [n=100000] (ns/op)` | 944904 ns/op | **707743 ns/op** (0.75x) 🟢 | 914741 ns/op (0.97x) ~ | heap |
| `sortbench/Sort [n=1000] (B/op)` | **0 B/op** | 0 B/op (n/a) ~ | 0 B/op (n/a) ~ | quick |
| `sortbench/Sort [n=1000] (allocs/op)` | **0 allocs/op** | 0 allocs/op (n/a) ~ | 0 allocs/op (n/a) ~ | quick |
| `sortbench/Sort [n=100000] (B/op)` | **0 B/op** | 0 B/op (n/a) ~ | 0 B/op (n/a) ~ | quick |
| `sortbench/Sort [n=100000] (allocs/op)` | **0 allocs/op** | 0 allocs/op (n/a) ~ | 0 allocs/op (n/a) ~ | quick |
| `sortbench/Join (B/op)` | **0 B/op** | 0 B/op (n/a) ~ | 0 B/op (n/a) ~ | quick |
| `sortbench/Join (allocs/op)` | **0 allocs/op** | 0 allocs/op (n/a) ~ | 0 allocs/op (n/a) ~ | quick |
| `sortbench/Parse (B/op)` | **0 B/op** | 0 B/op (n/a) ~ | 0 B/op (n/a) ~ | quick |
| `sortbench/Parse (allocs/op)` | **0 allocs/op** | 0 allocs/op (n/a) ~ | 0 allocs/op (n/a) ~ | quick |
| `sortbench/Encode (ns/op)` | 313.1 ns/op | **300.9 ns/op** (0.96x) ~ | 317.2 ns/op (1.01x) ~ | heap |
| `sortbench/Encode (B/op)` | **0 B/op** | 0 B/op (n/a) ~ | 0 B/op (n/a) ~ | quick |
| `sortbench/Encode (allocs/op)` | **0 allocs/op** | 0 allocs/op (n/a) ~ | 0 allocs/op (n/a) ~ | quick |
| `sortbench/Hash (B/op)` | **0 B/op** | 0 B/op (n/a) ~ | 0 B/op (n/a) ~ | quick |
| `sortbench/Hash (allocs/op)` | **0 allocs/op** | 0 allocs/op (n/a) ~ | 0 allocs/op (n/a) ~ | quick |
| geomean (ns/op, n=6/6) | 1.00x | 0.80x | 1.03x | |
| geomean (B/op, n=6/6) | — | — | — | |
| geomean (allocs/op, n=6/6) | — | — | — | |

_🟢 faster than reference · 🔴 slower than reference · ~ no significant difference · **bold** and Rank order the values in a row; only each variant against the reference is tested, not variants against each other_

<details>
<summary>Method</summary>

A cell is slower or faster than the reference only when it differs by ≥ 5% and, where the data allows, the difference is statistically significant against the reference column. Holm-adjusted across 2 comparisons per benchmark; reference-only. Raw p was below α but the adjusted p was not: heap on `Encode`: mann-whitney, Holm-adjusted p=0.09 (m=2).

</details>
