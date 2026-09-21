use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use clearra_pc4_activation::{
    verify_and_link, verify_generation_envelope, ActivationError, PinnedPublicKey,
    ProfileActivation, ReplayDecision, ReplayState, StaticPublicKeyring,
    GENERATION_STATEMENT_SCHEMA, PRODUCTION_CHANNEL, ROLLOUT_STATEMENT_SCHEMA,
    SIGNED_GENERATION_ENVELOPE_SCHEMA, SIGNED_ROLLOUT_ENVELOPE_SCHEMA,
};

const TEST_SEED: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
const TEST_PUBLIC: [u8; 32] = [
    0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07, 0x3a,
    0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07, 0x51, 0x1a,
];
const COMPATIBILITY: &str =
    "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const GENERATION_DOMAIN: &[u8] = b"clearra.pc4.production-generation-statement.v1\0";
const ROLLOUT_DOMAIN: &[u8] = b"clearra.pc4.production-rollout-statement.v1\0";

#[test]
fn rust_ed25519_matches_the_node_rfc_8032_kat() {
    let signature = hex_array::<64>(concat!(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155",
        "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
    ));
    VerifyingKey::from_bytes(&TEST_PUBLIC)
        .expect("public RFC key")
        .verify_strict(b"", &Signature::from_bytes(&signature))
        .expect("RFC signature");
}

#[test]
fn links_one_profile_and_keeps_setup_disabled_without_its_receipt() {
    let fixtures = fixtures(false, "one");
    let generation = signed_generation(&fixtures.authority);
    let pointer = signed_pointer(&generation, 1, None, 100, 200, vec![retained(&generation)]);
    let linked = verify_and_link(
        &canonical(&fixtures.reader),
        &generation.envelope,
        &pointer.envelope,
        &keyring(),
        &ReplayState::empty(),
        150,
        1,
        PRODUCTION_CHANNEL,
        COMPATIBILITY,
    )
    .expect("linked activation");
    assert_eq!(linked.replay_decision(), ReplayDecision::Initial);
    let statuses = linked
        .profiles()
        .iter()
        .map(|profile| (profile.profile(), profile.activation()))
        .collect::<Vec<_>>();
    assert_eq!(statuses.len(), 5);
    assert!(matches!(
        statuses[0].1,
        ProfileActivation::Unavailable { .. }
    ));
    assert!(matches!(
        statuses[3].1,
        ProfileActivation::Ready {
            pc_search_target_lines,
            setup_search_target_lines,
        } if pc_search_target_lines == &[4] && setup_search_target_lines.is_empty()
    ));

    let mut injected_reader = fixtures.reader;
    injected_reader["profiles"][3]["pc_search_target_lines"] = json!([4]);
    injected_reader["profiles"][3]["target_qualification_receipts"] =
        json!([pc_receipt("jstris-180")]);
    assert_code(
        verify_and_link(
            &canonical(&injected_reader),
            &generation.envelope,
            &pointer.envelope,
            &keyring(),
            &ReplayState::empty(),
            150,
            1,
            PRODUCTION_CHANNEL,
            COMPATIBILITY,
        )
        .unwrap_err(),
        "pc4_activation_reader_minted_authority",
    );
}

#[test]
fn separately_signed_setup_receipt_enables_setup_only_for_that_profile() {
    let fixtures = fixtures(true, "setup");
    let generation = signed_generation(&fixtures.authority);
    let pointer = signed_pointer(&generation, 1, None, 100, 200, vec![retained(&generation)]);
    let linked = verify_and_link(
        &canonical(&fixtures.reader),
        &generation.envelope,
        &pointer.envelope,
        &keyring(),
        &ReplayState::empty(),
        150,
        1,
        PRODUCTION_CHANNEL,
        COMPATIBILITY,
    )
    .expect("Setup-linked activation");
    assert!(matches!(
        linked.profiles()[3].activation(),
        ProfileActivation::Ready {
            setup_search_target_lines,
            ..
        } if setup_search_target_lines == &[4]
    ));
}

#[test]
fn replay_state_requires_exact_next_pointer_and_retained_previous_generation() {
    let first_fixture = fixtures(false, "one");
    let first_generation = signed_generation(&first_fixture.authority);
    let first_pointer = signed_pointer(
        &first_generation,
        1,
        None,
        100,
        400,
        vec![retained(&first_generation)],
    );
    let first = verify_and_link(
        &canonical(&first_fixture.reader),
        &first_generation.envelope,
        &first_pointer.envelope,
        &keyring(),
        &ReplayState::empty(),
        150,
        1,
        PRODUCTION_CHANNEL,
        COMPATIBILITY,
    )
    .expect("first activation");

    let second_fixture = fixtures(false, "two");
    let second_generation = signed_generation(&second_fixture.authority);
    let second_pointer = signed_pointer(
        &second_generation,
        2,
        Some(&first_pointer.identity),
        160,
        400,
        vec![retained(&second_generation), retained(&first_generation)],
    );
    let second = verify_and_link(
        &canonical(&second_fixture.reader),
        &second_generation.envelope,
        &second_pointer.envelope,
        &keyring(),
        first.next_replay_state(),
        170,
        1,
        PRODUCTION_CHANNEL,
        COMPATIBILITY,
    )
    .expect("second activation");
    assert_eq!(second.replay_decision(), ReplayDecision::Advanced);

    assert_code(
        verify_and_link(
            &canonical(&first_fixture.reader),
            &first_generation.envelope,
            &first_pointer.envelope,
            &keyring(),
            second.next_replay_state(),
            180,
            1,
            PRODUCTION_CHANNEL,
            COMPATIBILITY,
        )
        .unwrap_err(),
        "pc4_activation_rollout_replay",
    );
}

#[test]
fn canonical_lf_and_signature_mutations_fail_closed() {
    let fixtures = fixtures(false, "one");
    let generation = signed_generation(&fixtures.authority);
    for changed in [
        generation.envelope.trim_end_matches('\n').to_owned(),
        generation.envelope.replace('\n', "\r\n"),
        format!("{}\n", generation.envelope),
        format!("\u{feff}{}", generation.envelope),
        generation
            .envelope
            .replacen("{\"schema\":", "{\"schema\": ", 1),
    ] {
        assert!(verify_generation_envelope(&changed, &keyring()).is_err());
    }
    let mut parsed: Value = serde_json::from_str(&generation.envelope).expect("envelope JSON");
    parsed["signature_hex"] = Value::String(format!(
        "{}0",
        &parsed["signature_hex"].as_str().unwrap()[..127]
    ));
    assert_code(
        verify_generation_envelope(&canonical(&parsed), &keyring()).unwrap_err(),
        "pc4_activation_signature",
    );
}

struct Fixtures {
    reader: Value,
    authority: Value,
}

struct SignedGeneration {
    envelope: String,
    generation_identity: String,
    authority_identity: String,
}

struct SignedPointer {
    envelope: String,
    identity: String,
}

fn fixtures(setup: bool, generation: &str) -> Fixtures {
    let reader_profiles = profiles(|profile| {
        if profile == "jstris-180" {
            reader_ready(profile)
        } else {
            unavailable(profile)
        }
    });
    let authority_profiles = profiles(|profile| {
        if profile == "jstris-180" {
            authority_ready(profile, setup)
        } else {
            unavailable(profile)
        }
    });
    Fixtures {
        reader: json!({
            "schema": "clearra.pc4.host-generation.v1",
            "repository": "muse918/tetris-4lpc-mdp-vstar-policy",
            "revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "profiles": reader_profiles,
            "transferred_bytes": 0,
        }),
        authority: json!({
            "schema": "clearra.pc4.host-generation.v1",
            "repository": "muse918/tetris-4lpc-mdp-vstar-policy",
            "revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "admission": {
                "generation_id": generation,
                "manifest_content_identity": if generation == "one" { id('d') } else { id('e') },
            },
            "profiles": authority_profiles,
            "transferred_bytes": 0,
        }),
    }
}

fn profiles(mut build: impl FnMut(&str) -> Value) -> Vec<Value> {
    ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"]
        .into_iter()
        .map(&mut build)
        .collect()
}

fn unavailable(profile: &str) -> Value {
    json!({
        "profile": profile,
        "upstream_complete": false,
        "status": "unavailable",
        "reason": "not-admitted",
    })
}

fn base_ready(profile: &str) -> Value {
    json!({
        "profile": profile,
        "upstream_complete": true,
        "status": "ready",
        "reader_contract": "hydra-jstris-180-complete-graph-v1",
        "field_count": 817740,
        "target_width": 3,
        "target_lines": [4],
        "terminal_id": 817739,
        "artifacts": {
            "fields": { "path": "field_hash_to_id.bin", "byte_length": 6541936, "content_identity": id('1') },
            "offsets": { "path": "graph_offsets.bin", "byte_length": 6541936, "content_identity": id('2') },
            "graph": { "path": "graph.bin", "byte_length": 511000000, "content_identity": id('3') },
        },
        "evidence": { "reader": id('4') },
    })
}

fn reader_ready(profile: &str) -> Value {
    let mut value = base_ready(profile);
    value["pc_search_target_lines"] = json!([]);
    value["setup_search_target_lines"] = json!([]);
    value["target_qualification_receipts"] = json!([]);
    value
}

fn authority_ready(profile: &str, setup: bool) -> Value {
    let mut value = base_ready(profile);
    let mut receipts = vec![pc_receipt(profile)];
    if setup {
        receipts.push(setup_receipt(profile));
    }
    value["pc_search_target_lines"] = json!([4]);
    value["setup_search_target_lines"] = if setup { json!([4]) } else { json!([]) };
    value["target_qualification_receipts"] = Value::Array(receipts);
    value["admission"] = json!({ "profile_binding_identity": id('5') });
    value
}

fn pc_receipt(profile: &str) -> Value {
    json!({
        "schema": "clearra.pc4.exact-target-qualification.v1",
        "profile": profile,
        "use_case": "pc-search",
        "target_lines": 4,
    })
}

fn setup_receipt(profile: &str) -> Value {
    json!({
        "schema": "clearra.pc4.exact-setup-target-qualification.v1",
        "profile": profile,
        "use_case": "setup-search",
        "target_lines": 4,
    })
}

fn signed_generation(authority: &Value) -> SignedGeneration {
    let generation_json = canonical(authority);
    let generation_identity = sha256_identity(generation_json.as_bytes());
    let statement = json!({
        "schema": GENERATION_STATEMENT_SCHEMA,
        "algorithm": "ed25519",
        "key_id": key_id(),
        "channel": PRODUCTION_CHANNEL,
        "compatibility_identity": COMPATIBILITY,
        "generation_id": authority["admission"]["generation_id"],
        "generation_identity": generation_identity,
        "generation_json": generation_json,
        "repository": authority["repository"],
        "revision": authority["revision"],
    });
    let statement_text = canonical(&statement);
    SignedGeneration {
        envelope: signed_envelope(
            SIGNED_GENERATION_ENVELOPE_SCHEMA,
            GENERATION_DOMAIN,
            &statement_text,
        ),
        generation_identity,
        authority_identity: sha256_identity(statement_text.as_bytes()),
    }
}

fn signed_pointer(
    generation: &SignedGeneration,
    sequence: u64,
    previous: Option<&str>,
    issued: u64,
    expires: u64,
    retained: Vec<Value>,
) -> SignedPointer {
    let statement = json!({
        "schema": ROLLOUT_STATEMENT_SCHEMA,
        "algorithm": "ed25519",
        "key_id": key_id(),
        "channel": PRODUCTION_CHANNEL,
        "rollout_sequence": sequence.to_string(),
        "selected_generation_identity": generation.generation_identity,
        "selected_authority_identity": generation.authority_identity,
        "previous_pointer_identity": previous,
        "retained_generations": retained,
        "rollback_limit": "2",
        "issued_at_unix_seconds": issued.to_string(),
        "expires_at_unix_seconds": expires.to_string(),
    });
    let statement_text = canonical(&statement);
    SignedPointer {
        envelope: signed_envelope(
            SIGNED_ROLLOUT_ENVELOPE_SCHEMA,
            ROLLOUT_DOMAIN,
            &statement_text,
        ),
        identity: sha256_identity(statement_text.as_bytes()),
    }
}

fn retained(generation: &SignedGeneration) -> Value {
    json!({
        "generation_identity": generation.generation_identity,
        "authority_identity": generation.authority_identity,
    })
}

fn signed_envelope(schema: &str, domain: &[u8], statement_text: &str) -> String {
    let mut preimage = domain.to_vec();
    preimage.extend_from_slice(statement_text.as_bytes());
    let signature = SigningKey::from_bytes(&TEST_SEED).sign(&preimage);
    canonical(&json!({
        "schema": schema,
        "statement": statement_text,
        "signature_hex": hex(signature.to_bytes()),
    }))
}

fn keyring() -> StaticPublicKeyring {
    let key = Box::leak(Box::new([PinnedPublicKey::new(
        Box::leak(key_id().into_boxed_str()),
        TEST_PUBLIC,
    )]));
    StaticPublicKeyring::from_pinned(key).expect("test keyring")
}

fn key_id() -> String {
    format!("ed25519-raw-sha256:{:x}", Sha256::digest(TEST_PUBLIC))
}

fn canonical(value: &Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string(value).expect("canonical fixture JSON")
    )
}

fn sha256_identity(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn id(character: char) -> String {
    format!("sha256:{}", character.to_string().repeat(64))
}

fn hex(bytes: impl IntoIterator<Item = u8>) -> String {
    bytes
        .into_iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hex_array<const N: usize>(value: &str) -> [u8; N] {
    let mut bytes = [0_u8; N];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).expect("hex fixture");
    }
    bytes
}

fn assert_code(error: ActivationError, expected: &str) {
    assert_eq!(error.code(), expected);
}
