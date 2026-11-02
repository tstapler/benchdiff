<!-- benchdiff:default -->
### Benchmark comparison

**1 regressed · 1 improved · 16 unchanged · 0 new · 0 removed**

#### `sortbench`

| Benchmark | Base | Head | Change | Evidence |
|---|--:|--:|--:|---|
| `Parse [ns/op]` | 4150 `ns/op` | 5417 `ns/op` | 🔴 +30.5% ▰▰▰▰▱ | p<0.001 (n=10/10) |
| `Sort/n=1000 [ns/op]` | 8916 `ns/op` | 8076 `ns/op` | 🟢 -9.4% ▰▱▱▱▱ | p<0.001 (n=10/10) |

<details>
<summary>16 unchanged</summary>

**`sortbench`**

| Benchmark | Base | Head | Change | Evidence |
|---|--:|--:|--:|---|
| `Sort/n=1000 [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Sort/n=1000 [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Sort/n=100000 [ns/op]` | 944904 `ns/op` | 914741 `ns/op` | ⚪ -3.2% ▱▱▱▱▱ | p<0.001 (n=10/10) |
| `Sort/n=100000 [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Sort/n=100000 [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Join [ns/op]` | 25686 `ns/op` | 25908 `ns/op` | ⚪ +0.9% ▱▱▱▱▱ | p=0.912 (n=10/10) |
| `Join [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Join [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Parse [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Parse [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Encode [ns/op]` | 313.1 `ns/op` | 317.2 `ns/op` | ⚪ +1.3% ▱▱▱▱▱ | p=0.684 (n=10/10) |
| `Encode [B/op]` | 0 `B/op` | 0 `B/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Encode [allocs/op]` | 0 `allocs/op` | 0 `allocs/op` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `Hash [ns/op]` | 782.5 `ns/op` | 779.6 `ns/op` | ⚪ -0.4% ▱▱▱▱▱ | p=0.684 (n=10/10) |
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
