# Contributing to wisperChad

Thanks for helping. Some ground rules keep this project easy to work on.

## Setup

See *Build from source* in the [README](README.md). Then:

```bash
npm install
npm run dev
```

The UI in `ui/` is plain HTML/CSS/JS with no bundler. Edit and reload.

## Before opening a PR

```bash
cd src-tauri
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs the same checks on Windows, macOS and Linux.

If you added, removed or updated a Rust dependency, refresh the license list shown in About:

```bash
python scripts/licenses.py
```

A new AI model needs its credit and license added to the top of `scripts/licenses.py` and to the
About page.

## Good first contributions

- **Add or fix a model** in `src-tauri/src/catalog.rs`. Please include where your size and memory
  numbers came from, ideally measured on your own hardware.
- **Improve the prompts** in `src-tauri/src/prompts.rs`. Include before/after examples in the PR
  and the model you tested with. Small models behave very differently from big ones.
- **Try it on a Linux PC or an Intel Mac.** Nobody has yet. Work through the Linux or macOS part of
  [TESTING.md](TESTING.md) and open an issue with what happens.
- **GPU detection for Intel GPUs on Linux and for Intel Macs** in `hardware.rs`.
- Anything on the roadmap in the README.

## Principles

1. **Local first.** No feature may send audio or text to a server unless the user explicitly
   configures one.
2. **Never send a reply for the user.** wisperChad drafts. Humans press Send. Auto submit is opt-in
   and for dictation only.
3. **Runs on modest hardware.** Defaults should work on an 8 GB laptop without a GPU.
4. **No telemetry.**
