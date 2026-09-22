# ELK packaging hook

Python multi-molecule diagram layout uses **elkrs** in `xpict-core` (default).
Browser / Node may use **elkjs** (`js/` optionalDependency).

This directory holds an optional **ELK JAR** fetch helper for environments that
still want a JVM ELK binary (not required for the MVP paint path).

```bash
./vendor/elk/fetch_elk.sh
```

Do not commit large JAR binaries unless the release process explicitly vendors them.
