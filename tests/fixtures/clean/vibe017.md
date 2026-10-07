The scanner reads files in parallel. Reporters sort findings by path and source location.

- Fix handling of recursive wildcard patterns in nested ignore files during directory traversal.
- Fix handling of repeated recursive wildcard patterns in nested ignore files during directory traversal.

| Pattern | Behavior |
| --- | --- |
| `**` | Matches recursive wildcard patterns in nested ignore files during directory traversal. |
| `**/*` | Matches repeated recursive wildcard patterns in nested ignore files during directory traversal. |

Validate extension JSON: Error: Field 'classes/Control/methods/get_theme_font/arguments/1': default_value changed value in new API, from none to empty.
Validate extension JSON: Error: Field 'classes/Control/methods/get_theme_icon/arguments/1': default_value changed value in new API, from none to empty.
Validate extension JSON: Error: Field 'classes/Control/methods/get_theme_color/arguments/1': default_value changed value in new API, from none to empty.
