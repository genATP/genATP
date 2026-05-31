//! Property tests: domain-based DEKs are *strongly* isolated.
//!
//! Phase 6 testing matrix gate: "wrong-domain DEK never decrypts" —
//! enforced here with `proptest`. We sample arbitrary plaintext +
//! arbitrary domain pairs and assert that decryption with the
//! mismatched domain's DEK always fails.

use genatp_vault::crypto::{derive_dek, open, seal};
use genatp_vault::keystore::MasterKey;
use proptest::prelude::*;

fn arb_domain() -> impl Strategy<Value = String> {
    "[a-z0-9]{1,8}\\.(com|org|dev|net)".prop_map(|s| s)
}

fn arb_plaintext() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..256)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        // Wrong-domain decrypt failures are deterministic in our
        // crypto layer: AES-GCM authentication tag mismatch is
        // *guaranteed* for distinct keys, so no shrinking is needed.
        max_shrink_iters: 16,
        ..ProptestConfig::default()
    })]

    #[test]
    fn wrong_domain_dek_never_decrypts(
        kek_seed in any::<[u8; 32]>(),
        a in arb_domain(),
        b in arb_domain(),
        plaintext in arb_plaintext(),
    ) {
        prop_assume!(a != b);

        let kek = MasterKey::from_bytes(kek_seed);
        let dek_a = derive_dek(&kek, &a);
        let dek_b = derive_dek(&kek, &b);

        let aad = format!("cookie:{a}:k").into_bytes();
        let sealed = seal(&dek_a, &aad, &plaintext).unwrap();
        let err = open(&dek_b, &aad, &sealed);
        prop_assert!(err.is_err(), "decrypt with wrong DEK should fail");
    }

    #[test]
    fn aad_binding_prevents_relocation(
        kek_seed in any::<[u8; 32]>(),
        d in arb_domain(),
        plaintext in arb_plaintext(),
        sub_a in "[a-z]{1,8}",
        sub_b in "[a-z]{1,8}",
    ) {
        prop_assume!(sub_a != sub_b);

        let kek = MasterKey::from_bytes(kek_seed);
        let dek = derive_dek(&kek, &d);
        let aad_a = format!("cookie:{d}:{sub_a}").into_bytes();
        let aad_b = format!("cookie:{d}:{sub_b}").into_bytes();
        let sealed = seal(&dek, &aad_a, &plaintext).unwrap();

        prop_assert!(
            open(&dek, &aad_b, &sealed).is_err(),
            "ciphertext bound to AAD-A must not open under AAD-B"
        );
        prop_assert!(open(&dek, &aad_a, &sealed).is_ok());
    }

    #[test]
    fn dek_derivation_is_deterministic(
        kek_seed in any::<[u8; 32]>(),
        d in arb_domain(),
    ) {
        let kek = MasterKey::from_bytes(kek_seed);
        let a = derive_dek(&kek, &d);
        let b = derive_dek(&kek, &d);
        // Compare via seal+open behaviour to avoid exposing the
        // private bytes accessor.
        let s = seal(&a, b"x", b"hello").unwrap();
        prop_assert!(open(&b, b"x", &s).is_ok());
    }
}
