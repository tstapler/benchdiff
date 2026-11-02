<!-- benchdiff:default -->
### Benchmark comparison

**0 regressed · 4 improved · 14 unchanged · 0 new · 0 removed**

#### `sortbench`

| Benchmark | Base | Head | Change | Evidence |
|---|--:|--:|--:|---|
| `Sort/n=1000 [ns/op]` | 8916 `ns/op` | 6190 `ns/op` | 🟢 -30.6% ▰▰▰▰▱ | p<0.001 (n=10/10) |
| `Join [ns/op]` | 25686 `ns/op` | 18296 `ns/op` | 🟢 -28.8% ▰▰▰▱▱ | p<0.001 (n=10/10) |
| `Hash [ns/op]` | 782.5 `ns/op` | 560.9 `ns/op` | 🟢 -28.3% ▰▰▰▱▱ | p<0.001 (n=10/10) |
| `Sort/n=100000 [ns/op]` | 944904 `ns/op` | 707743 `ns/op` | 🟢 -25.1% ▰▰▰▱▱ | p<0.001 (n=10/10) |

<details>
<summary>14 unchanged</summary>

**`sortbench`**

| Benchmark | Base | Head | Change | Evidence |
|---|--:|--:|--:|---|
| `Sort/n=1000 [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Sort/n=1000 [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Sort/n=100000 [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Sort/n=100000 [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Join [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Join [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Parse [ns/op]` | 4150 `ns/op` | 4154 `ns/op` | ⚪ +0.1% ▱▱▱▱▱ | p=0.529 (n=10/10) |
| `Parse [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Parse [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Encode [ns/op]` | 313.1 `ns/op` | 300.9 `ns/op` | ⚪ -3.9% ▱▱▱▱▱ | p=0.043 (n=10/10) |
| `Encode [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Encode [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Hash [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Hash [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |

</details>

<details>
<summary>Environment</summary>

| | Base | Head |
|---|---|---|
| `cpu` | `Fixture CPU @ 3.0GHz` | `Fixture CPU @ 3.0GHz` |
| `goarch` | `amd64` | `amd64` |
| `goos` | `linux` | `linux` |
| `pkg` | `sortbench` | `sortbench` |

</details>

<sub>A change counts when it is ≥ 5% and, where the data allows, statistically significant. Judged: 18 by Mann-Whitney U test (α=0.05, ≥4 samples per side).</sub>
