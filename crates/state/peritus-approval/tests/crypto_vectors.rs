//! Published SHA-256 and Ed25519 vectors plus independent differential evidence.

use ed25519_dalek::{Signature, VerifyingKey};
use peritus_approval::{ApprovalPublicKey, ApprovalSignature};
use sha2::{Digest, Sha256};

fn decode<const N: usize>(hex: &str) -> [u8; N] {
    assert_eq!(hex.len(), N * 2);
    let mut output = [0_u8; N];
    let bytes = hex.as_bytes();
    for index in 0..N {
        let nibble = |value: u8| match value {
            b'0'..=b'9' => value - b'0',
            b'a'..=b'f' => value - b'a' + 10,
            _ => panic!("non-hex vector"),
        };
        output[index] = (nibble(bytes[index * 2]) << 4) | nibble(bytes[index * 2 + 1]);
    }
    output
}

#[allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "independent SHA-256 transcription retains the standard's named eight-word round state"
)]
fn reference_sha256(input: &[u8]) -> [u8; 32] {
    let constants: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let bit_len = (input.len() as u64) * 8;
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    let mut state = [
        0x6a09_e667_u32,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    for block in padded.chunks_exact(64) {
        let mut schedule = [0_u32; 64];
        for (index, word) in block.chunks_exact(4).enumerate() {
            schedule[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temporary1 = h
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(constants[index])
                .wrapping_add(schedule[index]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temporary2 = sum0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temporary1);
            d = c;
            c = b;
            b = a;
            a = temporary1.wrapping_add(temporary2);
        }
        for (target, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *target = target.wrapping_add(value);
        }
    }
    let mut output = [0_u8; 32];
    for (index, word) in state.iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

#[test]
fn nist_sha256_known_answers_and_reference_differential() {
    let vectors = [
        (
            &b""[..],
            decode::<32>("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        ),
        (
            &b"abc"[..],
            decode::<32>("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        ),
    ];
    for (message, expected) in vectors {
        let production: [u8; 32] = Sha256::digest(message).into();
        assert_eq!(production, expected);
        assert_eq!(reference_sha256(message), expected);
    }
    for length in 0_usize..257 {
        let message: Vec<u8> = (0..length)
            .map(|index| u8::try_from((index * 31) % 256).expect("value reduced to one byte"))
            .collect();
        let production: [u8; 32] = Sha256::digest(&message).into();
        assert_eq!(production, reference_sha256(&message));
    }
}

#[test]
fn rfc8032_vector_is_accepted_by_both_verifiers() {
    let public = decode::<32>("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
    let signature = decode::<64>(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155\
         5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
            .replace(' ', "")
            .as_str(),
    );
    let dalek_key = VerifyingKey::from_bytes(&public).expect("RFC public key");
    dalek_key.verify_strict(b"", &Signature::from_bytes(&signature)).expect("RFC signature");
    let compact_key = ed25519_compact::PublicKey::new(public);
    compact_key
        .verify(b"", &ed25519_compact::Signature::new(signature))
        .expect("alternate RFC verification");
}

#[test]
fn ordinary_mutations_fail_both_ed25519_implementations() {
    let public = decode::<32>("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
    let signature = decode::<64>(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
    );
    let dalek_key = VerifyingKey::from_bytes(&public).expect("RFC public key");
    let compact_key = ed25519_compact::PublicKey::new(public);
    for index in [0, 17, 31, 32, 47, 63] {
        let mut mutated = signature;
        mutated[index] ^= 1;
        assert!(dalek_key.verify_strict(b"", &Signature::from_bytes(&mutated)).is_err());
        assert!(compact_key.verify(b"", &ed25519_compact::Signature::new(mutated)).is_err());
    }
    assert!(dalek_key.verify_strict(b"x", &Signature::from_bytes(&signature)).is_err());
    assert!(compact_key.verify(b"x", &ed25519_compact::Signature::new(signature)).is_err());
    for index in [0, 15, 31] {
        let mut mutated = public;
        mutated[index] ^= 1;
        let dalek_rejects = VerifyingKey::from_bytes(&mutated).map_or(true, |key| {
            key.verify_strict(b"", &Signature::from_bytes(&signature)).is_err()
        });
        let compact_rejects = ed25519_compact::PublicKey::new(mutated)
            .verify(b"", &ed25519_compact::Signature::new(signature))
            .is_err();
        assert!(dalek_rejects && compact_rejects, "mutated public-key byte {index}");
    }
}

#[test]
fn crypto_wrappers_reject_every_adjacent_malformed_length() {
    assert!(ApprovalPublicKey::from_slice(&[0; 31]).is_err());
    assert!(ApprovalPublicKey::from_slice(&[0; 33]).is_err());
    assert!(ApprovalSignature::from_slice(&[0; 63]).is_err());
    assert!(ApprovalSignature::from_slice(&[0; 65]).is_err());
}
