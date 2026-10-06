The scanner reads files in parallel. Reporters sort findings by path and source location.

- Fix handling of recursive wildcard patterns in nested ignore files during directory traversal.
- Fix handling of repeated recursive wildcard patterns in nested ignore files during directory traversal.

| Pattern | Behavior |
| --- | --- |
| `**` | Matches recursive wildcard patterns in nested ignore files during directory traversal. |
| `**/*` | Matches repeated recursive wildcard patterns in nested ignore files during directory traversal. |
