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

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse(candidate), parse(current)) {
        (Some(c), Some(cur)) => c > cur,
        _ => false,
    }
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
    fn unparseable_never_newer() {
        assert!(!is_newer("garbage", "v1.0.0"));
        assert!(!is_newer("v1.0.0", "garbage"));
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
