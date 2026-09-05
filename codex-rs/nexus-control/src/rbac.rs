//! RBAC permission check (T1-1).

/// Permission strings are `resource:action` (e.g. `threads:read`).
/// Wildcards supported: `*:*` (admin), `threads:*` (all actions on resource).
pub fn check_permission(permissions: &[String], resource: &str, action: &str) -> bool {
    let needle = format!("{resource}:{action}");
    permissions.iter().any(|p| {
        p == "*:*"
            || p == &needle
            || (p.ends_with(":*") && p.starts_with(&format!("{resource}:")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_allows_all() {
        assert!(check_permission(&["*:*".into()], "threads", "read"));
        assert!(check_permission(&["*:*".into()], "any", "thing"));
    }

    #[test]
    fn resource_wildcard() {
        assert!(check_permission(&["threads:*".into()], "threads", "read"));
        assert!(!check_permission(&["threads:*".into()], "users", "read"));
    }

    #[test]
    fn exact_deny() {
        assert!(!check_permission(&["threads:read".into()], "threads", "write"));
        assert!(check_permission(&["threads:read".into()], "threads", "read"));
    }

    #[test]
    fn no_permissions_denies() {
        assert!(!check_permission(&[], "threads", "read"));
    }
}
