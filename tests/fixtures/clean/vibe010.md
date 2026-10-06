Workers read each file once. The resulting analysis is shared by every enabled rule, which avoids repeating tokenization across thirty detectors. Findings retain byte offsets. Reporters then sort those findings by path, line, column, and stable rule identifier before emitting output.

- Add a flag for selecting the format of every report.
- Fix a crash when reading files that contain invalid bytes.
- Remove the deprecated flag that silenced parser error messages.
- Speed up discovery of ignored files in very large repositories.
- Report skipped files in the summary printed after each scan.
- Accept configuration files that use Windows line endings as well.
- Document every rule with an example and a known limitation.
- Sort findings by path and line before writing any output.
