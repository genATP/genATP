# Perception fixtures

Hand-authored raw a11y trees used by `genatp-perception` integration tests
and the `perception_tokens` benchmark.

## Layout

```
fixtures/perception/
└── raw/                    # serialised genatp_perception::RawTree JSON
    ├── 01_hello.json
    ├── 02_login.json
    └── … (12 files)
```

Each file deserialises directly into [`RawTree`][raw]. Update them when the
input shape changes, then run `cargo insta review --workspace` to accept the
golden snapshot updates that follow.

[raw]: ../../crates/genatp-perception/src/raw.rs

## Adding a fixture

1. Drop a new `<NN>_<slug>.json` file under `raw/`.
2. Run `cargo test -p genatp-perception --test golden`.
3. Inspect the generated `tests/snapshots/golden__<slug>.snap.new` files;
   `cargo insta accept` if they look correct.

The Phase 2 acceptance corpus targets ~50 pages — the initial 12 cover the
common shapes (forms, search, modals, articles, decorative noise).
