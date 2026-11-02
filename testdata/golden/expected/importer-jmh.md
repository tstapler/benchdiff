<!-- benchdiff:default -->
### Benchmark comparison

**0 regressed · 0 improved · 3 unchanged · 0 new · 0 removed**

No meaningful changes.

<details>
<summary>3 unchanged</summary>

**`dev.stapler.stelekit.benchmarks.SearchBenchmark`**

| Benchmark | Base | Head | Change | Evidence |
|---|--:|--:|--:|---|
| `search[size=1000] [thrpt]` | 1520 `ops/s` | 1520 `ops/s` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=6/6) |
| `search[size=10] [avgt]` | 650.1 `ns/op` | 650.1 `ns/op` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=4/4) |
| `search[size=10] [gc.alloc.rate.norm]` | 128 `B/op` | 128 `B/op` | ⚪ 0.0% ▱▱▱▱▱ | — |

</details>

<details>
<summary>Environment</summary>

| | Base | Head |
|---|---|---|
| `jvm` | `21.0.4` | `21.0.4` |
| `vm` | `OpenJDK 64-Bit Server VM` | `OpenJDK 64-Bit Server VM` |

</details>

<sub>A change counts when it is ≥ 5% and, where the data allows, statistically significant. Judged: 2 by Mann-Whitney U test (α=0.05, ≥4 samples per side), 1 by threshold only (point estimates).</sub>
