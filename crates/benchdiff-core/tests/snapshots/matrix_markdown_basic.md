<!-- benchdiff:default -->
### Benchmark comparison: 3 variants (reference: quick)

**merge** has the best geomean (0.82x vs reference quick, over 3/4 common ns/op rows; variants are not tested against each other); slower than reference in 0 of 3 rows; heap slower in 2 rows · 1 incomplete

reference: quick (first listed; set 'reference' to change)

| Benchmark | quick (ref) | heap | merge | Rank |
|---|---|---|---|---|
| `Sort/size=100` | 16300 ns | 40000 ns (2.5x slower) 🔴 | **12000 ns** (0.74x) 🟢 | merge |
| `Sort/size=10` | 1630 ns | 2100 ns (1.29x) 🔴 | **1210 ns** (0.74x) 🟢 | merge |
| `Parse` | **500 ns** | 502 ns (1.00x) ~ | 500 ns (1.00x) ~ | quick |
| `Hash` | **90 ns** | — | 91 ns (1.01x) ~ | quick |
| geomean (ns/op, n=3/4) | 1.00x | 1.47x | 0.82x | |

_🟢 faster than reference · 🔴 slower than reference · ~ no significant difference · — not run · **bold** and Rank order the values in a row; only each variant against the reference is tested, not variants against each other_

<details>
<summary>Method</summary>

A cell is slower or faster than the reference only when it differs by ≥ 5% and, where the data allows, the difference is statistically significant against the reference column. Holm-adjusted across 2 comparisons per benchmark; reference-only. heap is missing 1 benchmark. Raw p was below α but the adjusted p was not: heap on `Parse`: mann-whitney, Holm-adjusted p=0.06 (m=2).

</details>
