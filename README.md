# Bindery for lemonfiber

**Watches for books and audiobooks and fetches them.**
Bindery is one of the parts the stack lemonfiber ships is built from.
Without it: No book automation (Readarr is discontinued).

## How lemonfiber carries it

The default bundle in
[lemonfiber-plugins](https://github.com/lemonfiber/lemonfiber-plugins) pins this
repository by a revision and the digests of its manifest and its tree, and
lemonfiber compiles that tree in. lemonfiber installs Bindery with the adapter
this repository holds, which speaks lemonfiber's `owner.claim`, `indexer.pull`,
`library.curate` and `books.curate` contracts in front of Bindery: a Rust crate built on the
core's adapter kit, into the image `ghcr.io/lemonfiber/bindery-adapter`,
published from every commit on `main` and from each tag the release train cuts.
It makes Bindery's first account for the administrator lemonfiber mints before
anybody else can, tells Bindery which Prowlarr to pull its indexers from, and
registers and reads its download clients, root folders, quality profiles and
queue, and finds and asks for the books, authors and series household members ask
for. lemonfiber reaches Bindery through this adapter alone. lemonfiber mints
Bindery's key and its first account's name and password and writes them into the
adapter's `upstream.json`; Bindery adopts the key from its environment, and the
adapter's claim makes the account.

## What a member's ask comes to

`books.curate` finds books, authors and series by words and asks Bindery for one of
them. Bindery reads nothing it does not hold by its own id for it, so the reference
the adapter gives each candidate is that id with the title and author that find it
again: a candidate is read from Bindery's library first, and otherwise by searching
those words and taking the result with that id. Each candidate carries the format
the household files books in, Bindery's `default.media_type` setting, or a held
book's own.

- **A book** is added alone. An author Bindery did not follow before is added
  unfollowed, under the root folder and quality profile the ask names; one it
  follows keeps its own.
- **An author** is followed with every book and those that come later, filed under
  the root folder and quality profile the ask names where Bindery adds them.
- **A series** is found only through Hardcover, so Bindery needs a Hardcover token
  and its Hardcover series setting on. Without them, a series is refused as
  unsupported, saying which is missing. Bindery holds the series linked to its
  Hardcover id, fills in every book of it and searches for them, whether or not the
  ask says to search. Each author the ask adds is filed under the root folder and
  quality profile it names and follows that series alone; an author Bindery already
  follows keeps the following it has. Later books arrive only through those
  authors' series following, which takes a new book where the metadata Bindery
  reads files it under a series the author follows by its id: the series Bindery
  holds under its Hardcover id where it held one when asked. A later book by an
  author outside the series' first authors is missed.

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
| `conformance/books.curate@1/` | The adapter's answer to each conformance case, in front of the pinned Bindery |
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
