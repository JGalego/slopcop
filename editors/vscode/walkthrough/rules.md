# Directive 3: Uphold the rules

Set rule severities in `.slopcop.toml`, or turn a rule off:

```toml
[slopcop.rules]
VIBE009 = "off"
```

When a finding is deliberate, the quick fix inserts a suppression directive. Give it a reason after `--`:

```python
# slopcop: ignore DEAD004 -- optional compatibility probe
```
