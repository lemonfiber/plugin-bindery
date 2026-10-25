# Bindery for lemonfiber

**Watches for books and audiobooks and fetches them.**
Bindery is one of the parts the stack lemonfiber ships is built from.
Without it: No book automation (Readarr is discontinued).

## How lemonfiber carries it

The default bundle in
[lemonfiber-plugins](https://github.com/lemonfiber/lemonfiber-plugins) pins this
repository by a revision and the digests of its manifest and its tree, and
lemonfiber compiles that tree in. lemonfiber installs Bindery with the adapter
this repository holds, which speaks lemonfiber's `owner.claim`, `indexer.pull`
and `library.curate` contracts in front of Bindery: a Rust crate built on the
core's adapter kit, into the image `ghcr.io/lemonfiber/bindery-adapter`,
published from every commit on `main` and from each tag the release train cuts.
It makes Bindery's first account for the administrator lemonfiber mints before
anybody else can, tells Bindery which Prowlarr to pull its indexers from, and
registers and reads its download clients, root folders, quality profiles and
queue. lemonfiber reaches Bindery through this adapter alone. lemonfiber mints
Bindery's key and its first account's name and password and writes them into the
adapter's `upstream.json`; Bindery adopts the key from its environment, and the
adapter's claim makes the account.

## What is here

| Path | What it is |
| --- | --- |
| `plugin.toml` | The part's manifest |
| `config/bindery/` | Where Bindery's starting files go |
| `recordings/bindery/` | The responses its proofs are held to |
| `src/` | The adapter |
| `conformance/owner.claim@1/` | The adapter's answer to each conformance case, in front of the pinned Bindery |
| `conformance/indexer.pull@1/` | The adapter's answer to each conformance case, in front of the pinned Bindery |
| `conformance/library.curate@1/` | The adapter's answer to each conformance case, in front of the pinned Bindery |
| `Dockerfile` | The adapter's image |
| `targets.toml` | The lemonfiber its checks run against, and the core the adapter is built on |
| `proofs.json` | What those checks said, committed |

## Checks

The plugin verdict is one job of spec's
[`first-party-plugin`](https://github.com/lemonfiber/spec/blob/main/.github/workflows/first-party-plugin.yml)
workflow: the manifest as the lemonfiber `targets.toml` names reads it, every
proof against its recording, and every pinned digest. The adapter is held to its
format, its lints, its tests, full line coverage and its dependency policy, its
image runs as lemonfiber runs it, and `recordings` takes every recording again
against the pinned Bindery. `just ci` runs the gates that need nothing but this
tree.

## Licence

The code and data in this repository are under the licence in `LICENSE`.
Bindery itself is under MIT and is not distributed here: this
repository names an image, it does not contain one.
