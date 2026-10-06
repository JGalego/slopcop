---
first: Metadata entry one.
second: Metadata entry two.
third: Metadata entry three.
fourth: Metadata entry four.
fifth: Metadata entry five.
sixth: Metadata entry six.
seventh: Metadata entry seven.
eighth: Metadata entry eight.
---

Fields use the form `name: value`. Unknown fields are rejected before parsing begins.

Install the package:

```sh
pip install example
```

Then start the server:

```sh
example serve
```

Open the dashboard in a browser:

```sh
open http://localhost:8000
```

Create an administrator account:

```sh
example users create --admin
```

Load the sample data:

```sh
example fixtures load
```

Stop the server with Ctrl+C when finished:

```sh
kill %1
```
