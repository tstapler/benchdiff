<!-- benchdiff:default -->
### Benchmark comparison

**1 regressed · 1 improved · 14 unchanged · 0 new · 0 removed**

#### `gob`

| Benchmark | Base | Head | Change | Evidence |
|---|--:|--:|--:|---|
| `Sort [ns/op]` | 9534 `ns/op` | 12394 `ns/op` | 🔴 +30.0% ▰▰▰▱▱ | p<0.001 (n=8/8) |
| `Join [ns/op]` | 26.88 `ns/op` | 21.5 `ns/op` | 🟢 -20.0% ▰▰▰▱▱ | p<0.001 (n=8/8) |

<details>
<summary>14 unchanged</summary>

**`gob`**

| Benchmark | Base | Head | Change | Evidence |
|---|--:|--:|--:|---|
| `Sort [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=8/8) |
| `Sort [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=8/8) |
| `Join [B/op]` | 8 `B/op` | 8 `B/op` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=8/8) |
| `Join [allocs/op]` | 1 `allocs/op` | 1 `allocs/op` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=8/8) |
| `Throughput [ns/op]` | 224.5 `ns/op` | 224.5 `ns/op` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=8/8) |
| `Throughput [MB/s]` | 4562 `MB/s` | 4562 `MB/s` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=8/8) |
| `Throughput [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=8/8) |
| `Throughput [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=8/8) |
| `Sub/n=x0 [ns/op]` | 0.2095 `ns/op` | 0.2095 `ns/op` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=8/8) |
| `Sub/n=x0 [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=8/8) |
| `Sub/n=x0 [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=8/8) |
| `Sub/n=x1 [ns/op]` | 31.66 `ns/op` | 31.66 `ns/op` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=8/8) |
| `Sub/n=x1 [B/op]` | 112 `B/op` | 112 `B/op` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=8/8) |
| `Sub/n=x1 [allocs/op]` | 1 `allocs/op` | 1 `allocs/op` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=8/8) |

</details>

<details>
<summary>Environment</summary>

| | Base | Head |
|---|---|---|
| `cpu` | `AMD Ryzen 9 7900X 12-Core Processor` | `AMD Ryzen 9 7900X 12-Core Processor` |
| `goarch` | `amd64` | `amd64` |
| `goos` | `linux` | `linux` |
| `pkg` | `gob` | `gob` |

</details>

<sub>A change counts when it is ≥ 1% and ≥ 5 absolute and, where the data allows, statistically significant. Judged: 16 by Mann-Whitney U test (α=0.05, ≥4 samples per side).</sub>
