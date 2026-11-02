<!-- benchdiff:default -->
### Benchmark comparison

**0 regressed · 0 improved · 10 unchanged · 0 new · 0 removed**

No meaningful changes.

<details>
<summary>10 unchanged</summary>

| Benchmark | Base | Head | Change | Evidence |
|---|--:|--:|--:|---|
| `true [memory_peak_resident]` | 3063808 `B` | 3063808 `B` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=10/10) |
| `true [time_cpu]` | 0.000549 `s` | 0.000549 `s` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=10/10) |
| `true [time_system]` | 0 `s` | 0 `s` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `true [time_user]` | 0.000549 `s` | 0.000549 `s` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=10/10) |
| `true [time_wall_clock]` | 0.0005876 `s` | 0.0005876 `s` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=10/10) |
| `sleep 0.002 [memory_peak_resident]` | 3063808 `B` | 3063808 `B` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=10/10) |
| `sleep 0.002 [time_cpu]` | 0.0008065 `s` | 0.0008065 `s` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=10/10) |
| `sleep 0.002 [time_system]` | 0 `s` | 0 `s` | ⚪ n/a (base is 0) | p=1.000 (n=10/10) |
| `sleep 0.002 [time_user]` | 0.000705 `s` | 0.000705 `s` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=10/10) |
| `sleep 0.002 [time_wall_clock]` | 0.002921 `s` | 0.002921 `s` | ⚪ 0.0% ▱▱▱▱▱ | p=1.000 (n=10/10) |

</details>

<details>
<summary>Environment</summary>

| | Base | Head |
|---|---|---|
| `hyperfine` | `2.0.0` | `2.0.0` |

</details>

<sub>A change counts when it is ≥ 5% and, where the data allows, statistically significant. Judged: 10 by Mann-Whitney U test (α=0.05, ≥4 samples per side).</sub>
