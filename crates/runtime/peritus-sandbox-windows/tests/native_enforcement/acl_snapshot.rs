//! Exact ACL oracle accepting both native SDDL encodings of the same access mask.

/// Matches the explicit inheritable Everyone deny with exactly the required file access bits.
pub fn has_exact_everyone_deny(sddl: &str) -> bool {
    sddl.split('(').skip(1).any(|ace| {
        let Some((ace, _)) = ace.split_once(')') else { return false };
        let fields = ace.split(';').collect::<Vec<_>>();
        fields.len() == 6
            && fields[0] == "D"
            && matches!(fields[1], "OICI" | "CIOI")
            // File list/read, write, append/create, execute, attributes, and delete.
            && rights_mask(fields[2]) == Some(0x0001_00a7)
            && fields[3].is_empty()
            && fields[4].is_empty()
            && matches!(fields[5], "WD" | "S-1-1-0")
    })
}

fn rights_mask(rights: &str) -> Option<u32> {
    if let Some(hex) = rights.strip_prefix("0x") {
        return u32::from_str_radix(hex, 16).ok();
    }
    if rights.is_empty() || !rights.len().is_multiple_of(2) {
        return None;
    }
    // Microsoft ACE Strings documents these SDDL aliases for ACCESS_MASK constants:
    // https://learn.microsoft.com/en-us/windows/win32/secauthz/ace-strings
    // SDDL uses these two-character ACCESS_MASK names even for file ACLs. Only the exact
    // expected rights are relevant here; every other symbolic permission fails this oracle.
    rights.as_bytes().chunks_exact(2).try_fold(0, |mask, token| {
        let bit = match token {
            b"CC" => 0x0000_0001,
            b"DC" => 0x0000_0002,
            b"LC" => 0x0000_0004,
            b"WP" => 0x0000_0020,
            b"LO" => 0x0000_0080,
            b"SD" => 0x0001_0000,
            _ => return None,
        };
        Some(mask | bit)
    })
}

#[test]
fn captured_windows_sddl_and_hex_bind_the_same_exact_deny() {
    // Captured from candidate63 Windows Server2025 icacls /save, including an inherited allow.
    let captured =
        "workspace/private\r\nD:AI(D;OICI;CCDCLCWPLOSD;;;WD)(A;OICIID;CCDCLCLOSD;;;WD)\r\n";
    assert!(has_exact_everyone_deny(captured));
    assert!(has_exact_everyone_deny("D:AI(D;OICI;0x100a7;;;WD)"));
    assert!(has_exact_everyone_deny("D:AI(D;CIOI;SDLOWPLCDCCC;;;S-1-1-0)"));
}

#[test]
fn changed_bits_principal_effect_or_inheritance_do_not_satisfy_the_oracle() {
    for rejected in [
        "D:AI(D;OICI;CCDCLCWPLO;;;WD)",
        "D:AI(D;OICI;CCDCLCWPLOSDRC;;;WD)",
        "D:AI(D;OICI;0x100a6;;;WD)",
        "D:AI(D;OICI;0x100af;;;WD)",
        "D:AI(D;OICI;FA;;;WD)",
        "D:AI(D;OICI;CCDCLCWPLOSD;;;SY)",
        "D:AI(A;OICI;CCDCLCWPLOSD;;;WD)",
        "D:AI(D;OICIID;CCDCLCWPLOSD;;;WD)",
        "D:AI(D;OI;CCDCLCWPLOSD;;;WD)",
        "D:AI(D;OICI;CCDCLCWPLOSD;;;WD",
    ] {
        assert!(!has_exact_everyone_deny(rejected), "accepted changed ACL: {rejected}");
    }
}
