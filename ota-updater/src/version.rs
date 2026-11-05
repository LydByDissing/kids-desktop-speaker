use semver::Version;

pub fn read_current(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parse(v: &str) -> Option<Version> {
    Version::parse(v.trim().trim_start_matches('v')).ok()
}

/// A valid candidate is newer than an unparseable current version, so a device
/// with a missing or corrupt version can still recover by updating.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse(candidate), parse(current)) {
        (Some(c), Some(cur)) => c > cur,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// `pending` is a version already installed and waiting for a reboot. The running
/// version does not change until then, so without it each check would reinstall.
pub fn should_install(candidate: &str, current: &str, pending: Option<&str>) -> bool {
    is_newer(candidate, pending.unwrap_or(current))
}

/// A release that failed verification fails the same way every time, so it is
/// skipped until its manifest changes, rather than downloaded again each minute.
pub fn is_known_bad(version: &str, sha256: &str, failed: Option<(&str, &str)>) -> bool {
    failed.is_some_and(|(v, s)| v == version && s.eq_ignore_ascii_case(sha256))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_patch_minor_major() {
        assert!(is_newer("v1.0.1", "v1.0.0"));
        assert!(is_newer("1.1.0", "1.0.9"));
        assert!(is_newer("v2.0.0", "v1.9.9"));
    }

    #[test]
    fn not_newer_when_equal_or_older() {
        assert!(!is_newer("v1.0.0", "v1.0.0"));
        assert!(!is_newer("v1.0.0", "v1.0.1"));
        assert!(!is_newer("1.0.0", "2.0.0"));
    }

    #[test]
    fn v_prefix_optional_on_either_side() {
        assert!(is_newer("1.2.0", "v1.1.0"));
        assert!(!is_newer("v1.1.0", "1.1.0"));
    }

    #[test]
    fn unparseable_candidate_is_never_newer() {
        assert!(!is_newer("garbage", "v1.0.0"));
        assert!(!is_newer("garbage", "other-garbage"));
    }

    #[test]
    fn unparseable_current_can_be_updated() {
        assert!(is_newer("v1.0.0", "9394ee4-dirty"));
    }

    #[test]
    fn failed_release_is_skipped_until_its_manifest_changes() {
        let failed = Some(("v1.1.0", "abc"));
        assert!(is_known_bad("v1.1.0", "abc", failed));
        assert!(is_known_bad("v1.1.0", "ABC", failed));
        assert!(!is_known_bad("v1.1.0", "def", failed));
        assert!(!is_known_bad("v1.2.0", "abc", failed));
        assert!(!is_known_bad("v1.1.0", "abc", None));
    }

    #[test]
    fn pending_install_is_not_repeated() {
        assert!(should_install("v1.1.0", "v1.0.0", None));
        assert!(!should_install("v1.1.0", "v1.0.0", Some("v1.1.0")));
        assert!(should_install("v1.2.0", "v1.0.0", Some("v1.1.0")));
    }

    #[test]
    fn read_current_trims_and_rejects_empty() {
        let dir = std::env::temp_dir().join(format!("kdsver-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("kds-version");
        std::fs::write(&f, "  v1.4.2\n").unwrap();
        assert_eq!(read_current(f.to_str().unwrap()).as_deref(), Some("v1.4.2"));
        std::fs::write(&f, "   \n").unwrap();
        assert_eq!(read_current(f.to_str().unwrap()), None);
        assert_eq!(read_current("/no/such/kds-version"), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
