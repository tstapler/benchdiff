<!-- benchdiff:default -->
### Benchmark comparison

**0 regressed · 0 improved · 3 unchanged · 0 new · 0 removed**

No meaningful changes.

<details>
<summary>3 unchanged</summary>

**`api`**

| Benchmark | Base | Head | Change | Evidence |
|---|--:|--:|--:|---|
| `create [p50]` | 0.44 `ms` | 0.44 `ms` | ⚪ 0.0% ▱▱▱▱▱ | — |
| `create [p95]` | 0.66 `ms` | 0.66 `ms` | ⚪ 0.0% ▱▱▱▱▱ | intervals |
| `echo` | 9100 `ops/s` | 9100 `ops/s` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=5/5) |

</details>

<details>
<summary>Environment</summary>

| | Base | Head |
|---|---|---|
| `runner` | `ubuntu-24.04` | `ubuntu-24.04` |
| `sha` | `abc123` | `abc123` |

</details>

<sub>A change counts when it is ≥ 5% and, where the data allows, statistically significant. Judged: 1 by Mann-Whitney U test (α=0.05, ≥4 samples per side), 1 by non-overlapping intervals, 1 by threshold only (point estimates).</sub>
