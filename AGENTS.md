# AGENTS.md — plugin-bindery

> **Start at the roadmap and board on [lemonfiber.app](https://lemonfiber.app),
> rendered from the report of where every unreleased version stands. Then the
> rules** every repository shares:
> [working in the repositories](https://github.com/lemonfiber/spec/blob/main/50-governance/working-in-the-repositories.md)
> and [the rules for agents](https://github.com/lemonfiber/spec/blob/main/50-governance/ai-contributors.md).
> This file holds only what is true of this repository.

## What this repo is

Bindery as a first-party lemonfiber plugin: `plugin.toml` and the adapter that
speaks the `owner.claim@1`, `indexer.pull@1` and `library.curate@1` contracts in
front of Bindery. The adapter is the Rust crate at the root, built on
`lemonfiber-adapter-kit` from the core at the commit `Cargo.toml` pins, into the
image the `Dockerfile` builds. `release.yml` publishes that image from every
commit on `main` and from the train's tag. The
[bundle contract](https://github.com/lemonfiber/spec/blob/main/20-architecture/contracts/bundle.md)
says how the catalogue pins this repository and how lemonfiber carries it.
`plugin.toml` does not name the adapter yet: the part is one service with the
in-process adapter its `api` names.

## The rules you cannot break

- **The plugin id is the service's id** (`ARCH-R266`). The container, the
  configuration directory, the settings and the journal keep their names.
- **Its claims' recordings sit under `recordings/bindery/`**, and its starting
  files under `config/bindery/` (`ARCH-R271`).
- **Every service declares `reaches` and `asks_for`** (`ARCH-R270`).
- **The reader is spec's `first-party-plugin` workflow's,** and lemonfiber
  records the recordings itself. There is no Python here.
- **No `unwrap`, `expect`, `panic` or indexing** outside tests. The lints in
  `Cargo.toml` are the core's.
- **The adapter never writes its credentials.** lemonfiber mints Bindery's key
  and the first account's name and password, and writes them into the adapter's
  `upstream.json` as `api_key`, `username` and `password` (`ARCH-R315`,
  `ARCH-R317`); the adapter reads them for every call and never logs them.
- **A claim changes nothing Bindery already holds** (`ARCH-R328`): only a Bindery
  with no account is set up, and one set up by anybody else is answered as theirs.
- **Bindery pulls its indexers from Prowlarr alone,** named in the servarr API
  (`ARCH-R332`, `ARCH-R333`).
- **What Bindery cannot do is answered `unsupported`** (`ARCH-R334`).
- **The core's crates move with the train.** The `rev` in `Cargo.toml` is the
  core commit `targets.toml` names, the same for every `lemonfiber-*` crate.
- **Moving Bindery's digest** means recording every response again against the
  new image in the same change, moving `RELEASE` and `DIGEST` in
  `src/adapter.rs` with it, and committing the `proofs.json` the checks write.

## Checks

```
just ci
```

The gates that need nothing but this tree. The plugin verdict and the
recordings run in CI; `just recordings` takes the recordings again locally, with
Docker.
