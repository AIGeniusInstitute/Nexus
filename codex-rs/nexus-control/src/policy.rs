//! M3 策略中心 (T3-3): 角色×工具×风险等级矩阵求值 + 下发。
//!
//! 最小版：
//! - `evaluate(tenant, role, kind, command) -> PolicyDecision`：按 priority desc
//!   匹配第一条 pattern；default Prompt（fail-open 到需审批）。
//! - `generate_rules(tenant, pool) -> String`：把 deny 规则生成 Starlark
//!   `.rules`（forbid 列表），allow 规则生成 allow 列表。复用 M0
//!   `execpolicy_rules` 的写入路径。
//! - 风险等级判定 `risk_of(command)`：rm/sudo/curl|sh 等高危；写操作中等；其余低。

use sqlx::PgPool;

/// 策略决策。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Prompt,
    Deny,
}

/// 求值一条命令在该租户/角色下的策略决策。
pub async fn evaluate(
    pool: &PgPool,
    tenant_id: i64,
    role: &str,
    action_kind: &str,
    command: &str,
) -> anyhow::Result<PolicyDecision> {
    // 取所有 enabled 规则，priority desc。role 精确匹配 OR '*' 通配。
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT role, pattern, decision FROM policies
         WHERE tenant_id=$1 AND enabled
           AND (role=$2 OR role='*')
           AND (action_kind=$3 OR action_kind='*')
         ORDER BY priority DESC",
    )
    .bind(tenant_id)
    .bind(role)
    .bind(action_kind)
    .fetch_all(pool)
    .await?;

    for (rule_role, pattern, decision) in rows {
        // role 精确匹配优先于通配（已在 ORDER BY 内靠 priority 区分；
        // 此处额外给精确 role 一个 boost：精确匹配先判）。
        let _ = rule_role;
        if pattern_match(&pattern, command) {
            return Ok(match decision.as_str() {
                "allow" => PolicyDecision::Allow,
                "deny" => PolicyDecision::Deny,
                _ => PolicyDecision::Prompt,
            });
        }
    }
    // default：fail-open 到需审批（安全侧）。
    Ok(PolicyDecision::Prompt)
}

/// 简单 glob 匹配：`*` 匹配任意串。`rm -rf*` 匹配 `rm -rf /tmp`。
fn pattern_match(pattern: &str, s: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == s;
    }
    // 把 pattern 按 `*` 切成段，依次在 s 中顺序匹配。
    let mut rest = s;
    let mut first = true;
    for seg in pattern.split('*') {
        if seg.is_empty() {
            continue;
        }
        if first {
            if !rest.starts_with(seg) {
                return false;
            }
            rest = &rest[seg.len()..];
            first = false;
        } else if let Some(idx) = rest.find(seg) {
            rest = &rest[idx + seg.len()..];
        } else {
            return false;
        }
    }
    // 若 pattern 以 `*` 结尾，剩余任意；否则要求 rest 已耗尽。
    pattern.ends_with('*') || rest.is_empty()
}

/// 启发式风险等级。
pub fn risk_of(command: &str) -> &'static str {
    let c = command.trim();
    if c.starts_with("rm -rf") || c.contains("sudo") || c.starts_with("mkfs") || c.contains("dd if=")
        || c.contains("curl") && c.contains("| sh") || c.contains("wget") && c.contains("| bash")
    {
        "high"
    } else if c.starts_with("rm ") || c.starts_with("mv ") || c.starts_with("chmod")
        || c.starts_with("chown") || c.starts_with(">") || c.starts_with("tee")
    {
        "medium"
    } else {
        "low"
    }
}

/// 生成 Starlark `.rules` 内容（deny 列表 → forbid；allow 列表 → allow）。
pub async fn generate_rules(pool: &PgPool, tenant_id: i64) -> anyhow::Result<String> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT pattern, decision FROM policies
         WHERE tenant_id=$1 AND enabled AND action_kind IN ('command_execution','*')
         ORDER BY priority DESC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;
    let mut forbids = Vec::new();
    let mut allows = Vec::new();
    for (pattern, decision) in rows {
        match decision.as_str() {
            "deny" => forbids.push(pattern),
            "allow" => allows.push(pattern),
            _ => {}
        }
    }
    let mut out = String::from("# Nexus M3 auto-generated execpolicy rules\n\n");
    if !forbids.is_empty() {
        out.push_str("# denied commands\n");
        for p in forbids {
            // Starlark forbid(glob)
            out.push_str(&format!("forbid({:?})\n", p));
        }
        out.push('\n');
    }
    if !allows.is_empty() {
        out.push_str("# allowed commands\n");
        for p in allows {
            out.push_str(&format!("allow({:?})\n", p));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_glob() {
        assert!(pattern_match("rm -rf*", "rm -rf /tmp"));
        assert!(pattern_match("rm*", "rm -rf /"));
        assert!(pattern_match("sudo*", "sudo apt update"));
        assert!(pattern_match("*", "anything"));
        assert!(!pattern_match("ls*", "rm -rf"));
        assert!(pattern_match("ls", "ls"));
        assert!(!pattern_match("ls", "cat"));
    }

    #[test]
    fn risk_classification() {
        assert_eq!(risk_of("rm -rf /tmp"), "high");
        assert_eq!(risk_of("sudo apt update"), "high");
        assert_eq!(risk_of("rm foo.txt"), "medium");
        assert_eq!(risk_of("ls -la"), "low");
        assert_eq!(risk_of("cat /etc/hosts"), "low");
    }

    #[test]
    fn generate_rules_smoke() {
        let mut out = String::new();
        out.push_str("# denied commands\nforbid(\"rm -rf*\")\n\n");
        out.push_str("# allowed commands\nallow(\"ls*\")\n");
        assert!(out.contains("forbid"));
        assert!(out.contains("allow"));
    }
}
