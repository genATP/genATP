//! Benchmark: vault unlock latency.
//!
//! Phase 6 testing matrix gate: **p95 < 100 ms** for `Vault::open` on
//! a vault with a small number of entries (the "warm laptop, fresh
//! process start" case).
//!
//! We model this with the `InMemoryKeyStore` to factor out the OS
//! keychain RTT (which varies wildly by platform). The vault file
//! itself does the realistic work: SQLite open, schema check,
//! migration, KDF init.

use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use genatp_vault::{Capability, Entry, InMemoryKeyStore, KeyStore, Vault};

fn populate(path: &std::path::Path, ks: &Arc<dyn KeyStore>) {
    let mut v = Vault::open_or_init(path, ks.clone()).unwrap();
    let cap = Capability::any();
    let entries: Vec<Entry> = (0..16)
        .map(|i| Entry::local_storage("github.com", format!("k{i}"), format!("v{i}")).unwrap())
        .collect();
    v.save(&cap, &entries).unwrap();
}

fn bench_unlock(c: &mut Criterion) {
    let td = tempfile::tempdir().unwrap();
    let path = td.path().join("vault.sqlite");
    let ks: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
    populate(&path, &ks);

    c.bench_function("vault_open_warm", |b| {
        b.iter(|| {
            let v = Vault::open(&path, ks.clone()).unwrap();
            std::hint::black_box(v.path().to_path_buf());
        });
    });
}

criterion_group!(benches, bench_unlock);
criterion_main!(benches);
