<!-- benchdiff:default -->
### Benchmark comparison

**1 regressed · 1 improved · 14 unchanged · 0 new · 0 removed**

#### `gob`

| Benchmark | Base | Head | Change | Evidence |
|---|--:|--:|--:|---|
| `Sort [ns/op]` | 9534 `ns/op` | 12394 `ns/op` | 🔴 +30.0% ▰▰▰▱▱ | p<0.001 (n=8/8) |
| `Join [ns/op]` | 26.88 `ns/op` | 21.5 `ns/op` | 🟢 -20.0% ▰▰▰▱▱ | p<0.001 (n=8/8) |

<details>
<summary>Environment</summary>

| | Base | Head |
|---|---|---|
| `cpu` | `AMD Ryzen 9 7900X 12-Core Processor` | `AMD Ryzen 9 7900X 12-Core Processor` |
| `goarch` | `amd64` | `amd64` |
| `goos` | `linux` | `linux` |
| `pkg` | `gob` | `gob` |

</details>

<sub>A change counts when it is ≥ 5% and, where the data allows, statistically significant. Judged: 16 by Mann-Whitney U test (α=0.05, ≥4 samples per side).</sub>
