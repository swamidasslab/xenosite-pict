# Maintainer notes (not published)

These pages are excluded from the public MkDocs site (`exclude_docs: dev/**`).

| Doc | Topic |
| --- | --- |
| [`bindings.md`](bindings.md) | Language binding layout |
| [`layout-notes.md`](layout-notes.md) | Layout / paint design notes |
| [`migration-xenosite.md`](migration-xenosite.md) | xenosite.org migration track |

Root [`Makefile`](../../Makefile) (`make help`) is the front door for build /
test / pages / version bump / publish dry-runs. CI and agent bootstrap use the
same targets.

Package release tags, tokens, and registries: [`.github/PUBLISH.md`](../../.github/PUBLISH.md).
