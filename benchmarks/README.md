# Benchmarks

`slopcop benchmark` builds five deterministic in-memory corpora. This avoids filesystem cache noise while measuring classification, analysis, all enabled rules, suppression handling, parallel scheduling, and finding collation. The command warms code and documentation analyzers, runs each profile three times, and reports the median.

| Corpus | Files | Mix |
| --- | ---: | --- |
| tiny repo | 32 | code and documentation |
| medium repo | 512 | code and documentation |
| large repo | 4,096 | code and documentation |
| docs-heavy | 1,024 | Markdown |
| code-heavy | 1,024 | Rust |

Observed range across two x86_64 Linux release invocations:

| Corpus | Elapsed | Files/sec | MB/sec |
| --- | ---: | ---: | ---: |
| tiny repo | 2.3–5.7 ms | 5,643–13,721 | 1.9–4.7 |
| medium repo | 8.8–17.0 ms | 30,066–58,395 | 10.4–20.2 |
| large repo | 63.7–78.8 ms | 51,961–64,286 | 18.2–22.5 |
| docs-heavy | 36.0–39.3 ms | 26,044–28,474 | 10.0–10.9 |
| code-heavy | 9.3–14.3 ms | 71,383–110,034 | 23.5–36.2 |

These numbers describe one machine, not a promise. CI runs the command as a smoke test and preserves its output for comparison; hard wall-clock gates would be unreliable on shared runners.
