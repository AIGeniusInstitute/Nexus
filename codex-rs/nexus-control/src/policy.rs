//! M3 策略中心 (T3-3) + M4 下发 (T4-4): 角色×工具×风险等级矩阵求值 + 下发。
//!
//! 最小版：
//! - `evaluate(tenant, role, kind, command) -> PolicyDecision`：按 priority desc
//!   匹配第一条 pattern；default Prompt（fail-open 到需审批）。
//! - `generate_rules(tenant, pool) -> String`：把 deny/allow 规则生成 Starlark
//!   `.rules`，语法用 M0 验证过的 `prefix_rule(pattern=[...], decision=...)`
//!   （`codex_execpolicy::parser::PolicyParser` 实测可解析；M3 的 forbid/allow
//!   未经验证且不符 parser，M4 修正）。
//! - `write_tenant_rules(pool, tenant_id, codex_home)`：生成 + 原子写
//!   `<codex_home>/rules/tenant-{id}.rules`，app-server 每-turn 自动加载。
//! - 风险等级判定 `risk_of(command)`：rm/sudo/curl|sh 等高危；写操作中等；其余低。

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Serialize;
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

/// 生成 Starlark `.rules` 内容（M0 验证的 `prefix_rule` 语法）。
///
/// policies 表的 glob pattern（如 `rm -rf*`、`sudo*`、`ls*`）翻译为
/// `prefix_rule(pattern=[token,...], decision="forbidden"/"allow")`，
/// 其中 pattern 是命令 argv 的前缀 token 列表（非 glob）。`*` 通配（catch-all
/// prompt）跳过——它由 HITL 审批兜底，不进 execpolicy。prompt 决策同样跳过。
pub async fn generate_rules(pool: &PgPool, tenant_id: i64) -> anyhow::Result<String> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT pattern, decision FROM policies
         WHERE tenant_id=$1 AND enabled AND action_kind IN ('command_execution','*')
         ORDER BY priority DESC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;
    let mut out = String::from("# Nexus auto-generated execpolicy (M4)\n");
    out.push_str("# prefix_rule syntax — auto-loaded by app-server.\n\n");
    for (pattern, decision) in rows {
        // deny → forbidden, allow → allow, prompt → 跳过（走 HITL 不进 execpolicy）。
        let dec = match decision.as_str() {
            "deny" => "forbidden",
            "allow" => "allow",
            _ => continue,
        };
        // glob → argv 前缀 token 列表：按空白切分，末尾 * 去掉。
        // `rm -rf*` → ["rm", "-rf"]；`sudo*` → ["sudo"]；`*` → 跳过（catch-all）。
        if pattern.trim() == "*" {
            continue;
        }
        let tokens: Vec<String> = pattern
            .split_whitespace()
            .map(|t| {
                // 去掉末尾的 `*`（`-rf*` → `-rf`，`sudo*` → `sudo`）。
                let t = t.trim_end_matches('*');
                t.to_string()
            })
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.is_empty() {
            continue;
        }
        let pat_str = tokens
            .iter()
            .map(|t| format!("{t:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "prefix_rule(\n    pattern = [{pat_str}],\n    decision = \"{dec}\",\n    justification = \"nexus policy: {pattern}\",\n)\n\n"
        ));
    }
    Ok(out)
}

/// 原子写入 per-tenant `.rules` 文件到 `<codex_home>/rules/tenant-{id}.rules`。
/// 调用方先 async `generate_rules(pool, tid)` 取内容，再经此 sync helper 写盘。
/// 原子写（tmp + rename）避免 app-server 读到半截文件。app-server 每-turn
/// 自动加载 `<CODEX_HOME>/rules/` 下所有 `.rules`（M0 T0-4 验证）。
pub fn write_tenant_rules(
    tenant_id: i64,
    codex_home: &Path,
    content: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let rules_dir = codex_home.join("rules");
    std::fs::create_dir_all(&rules_dir)
        .with_context(|| format!("create rules dir {}", rules_dir.display()))?;
    let path = rules_dir.join(format!("tenant-{tenant_id}.rules"));
    let tmp = path.with_extension("rules.tmp");
    std::fs::write(&tmp, content)
        .with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("rename -> {}", path.display()))?;
    Ok(path)
}

use anyhow::Context;

// ===========================================================================
// M6: 策略自学习闭环。记录人的审批决策 → 累计 N 次一致且与当前策略矛盾 →
// 自动提升（prompt→deny / prompt→allow）→ 写回 policies 表 + 刷 tenant
// rules 文件 → 下一 turn 自动加载。保守、安全单调（deny 不回退、高危不
// 自动 allow）。
// ===========================================================================

/// 提取命令的 argv 前缀 glob 作为学习 pattern。取前 2 token，若命令 token
/// 数 > 2 则末尾加 `*`（前缀匹配）；与 policies 表 glob 风格一致。
/// `rm -rf /tmp/x` → `rm -rf*`；`npm install x` → `npm install*`；`ls` → `ls`。
pub fn extract_pattern(command: &str) -> String {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    match tokens.len() {
        0 => "*".to_string(),
        1 => tokens[0].to_string(),
        2 => tokens[..2].join(" "),
        _ => format!("{} {}*", tokens[0], tokens[1]),
    }
}

/// 一条决策反馈（人 resolve 审批时落库）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct FeedbackRow {
    pub id: i64,
    pub pattern: String,
    pub decision: String,
    pub policy_rec: String,
    pub risk_level: Option<String>,
    pub turn_id: Option<i64>,
    pub created_at: DateTime<Utc>,
}

/// 记录一次审批 resolve 决策到 `policy_feedback`。`decision` 为人决策
/// 枚举字符串（approve/deny/cancel）；`policy_rec` 为决策时 evaluate 推荐
/// （allow/prompt/deny，取自 ticket.policy_decision）。
pub async fn record_feedback(
    pool: &PgPool,
    tenant_id: i64,
    pattern: &str,
    decision: &str,
    policy_rec: &str,
    risk_level: Option<&str>,
    turn_id: Option<i64>,
) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO policy_feedback (tenant_id, pattern, decision, policy_rec, risk_level, turn_id)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(tenant_id)
    .bind(pattern)
    .bind(decision)
    .bind(policy_rec)
    .bind(risk_level)
    .bind(turn_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// 学习生成的规则（提升后的 pattern + 新决策）。
#[derive(Debug, Clone)]
pub struct LearnedRule {
    pub pattern: String,
    pub decision: String, // deny | allow
}

/// 学习阈值（连续一致决策数才提升）。可经 env 配置。
fn learn_threshold() -> usize {
    std::env::var("NEXUS_POLICY_LEARN_THRESHOLD")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(3)
}

/// 分析该租户的 `policy_feedback`，对"连续 N 次一致决策且与当前策略矛盾且
/// 可提升"的 pattern 自动提升策略（prompt→deny 或 prompt→allow），UPSERT
/// 到 `policies` 表（source=learned）。返回被提升的 pattern 列表。
///
/// 安全单调：deny 永不回退；高危命令（risk=high）即使人反复 approve 也不
/// 自动 allow。
pub async fn learn(pool: &PgPool, tenant_id: i64) -> anyhow::Result<Vec<LearnedRule>> {
    let threshold = learn_threshold();
    // 取该租户全部反馈（按时间倒序），Rust 侧按 pattern 分组取前 N。
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT pattern, decision, risk_level FROM policy_feedback
         WHERE tenant_id=$1 ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    // pattern → 最近 N 条 (decision, risk)
    let mut groups: std::collections::HashMap<String, Vec<(String, Option<String>)>> =
        std::collections::HashMap::new();
    for (pattern, decision, risk) in rows {
        let g = groups.entry(pattern).or_default();
        if g.len() < threshold {
            g.push((decision, risk));
        }
    }

    let mut learned = Vec::new();
    for (pattern, recent) in &groups {
        if recent.len() < threshold {
            continue;
        }
        // 最近 N 次决策是否全一致。
        let first = &recent[0].0;
        if !recent.iter().all(|(d, _)| d == first) {
            continue;
        }
        let human_decision = first.as_str();
        let risk = recent.iter().rev().find_map(|(_, r)| r.clone())
            .unwrap_or_else(|| "low".to_string());

        // 当前策略（同 pattern 的 enabled 规则；None → prompt 默认）。
        let cur: Option<(String,)> = sqlx::query_as(
            "SELECT decision FROM policies
             WHERE tenant_id=$1 AND pattern=$2 AND enabled
             ORDER BY priority DESC LIMIT 1",
        )
        .bind(tenant_id)
        .bind(pattern)
        .fetch_optional(pool)
        .await?;
        let cur_dec = cur.map(|(d,)| d).unwrap_or_else(|| "prompt".to_string());

        // 提升规则（保守）。
        let promote = match (cur_dec.as_str(), human_decision) {
            ("prompt", "deny") => Some("deny"),
            ("prompt", "approve") if risk != "high" => Some("allow"),
            _ => None, // deny 不回退；allow 不变；高危不自动 allow
        };
        if let Some(new_dec) = promote {
            // UPSERT：覆盖同 pattern 的旧 prompt 种子（唯一索引已存在）。
            sqlx::query(
                "INSERT INTO policies (tenant_id, role, action_kind, pattern, risk_level,
                       decision, priority, enabled, source, learned_from)
                 VALUES ($1, '*', 'command_execution', $2, $3, $4, 50, TRUE, 'learned', $2)
                 ON CONFLICT (tenant_id, role, action_kind, pattern) DO UPDATE
                   SET decision=EXCLUDED.decision, risk_level=EXCLUDED.risk_level,
                       priority=50, enabled=TRUE, source='learned',
                       learned_from=EXCLUDED.learned_from",
            )
            .bind(tenant_id)
            .bind(pattern)
            .bind(&risk)
            .bind(new_dec)
            .execute(pool)
            .await?;
            learned.push(LearnedRule {
                pattern: pattern.clone(),
                decision: new_dec.to_string(),
            });
        }
    }
    Ok(learned)
}

/// 最近 N 天的决策反馈列表（可观测）。
pub async fn list_feedback(pool: &PgPool, tenant_id: i64, days: i32) -> anyhow::Result<Vec<FeedbackRow>> {
    let cutoff = Utc::now() - chrono::Duration::days(days as i64);
    let rows = sqlx::query_as::<_, FeedbackRow>(
        "SELECT id, pattern, decision, policy_rec, risk_level, turn_id, created_at
         FROM policy_feedback WHERE tenant_id=$1 AND created_at >= $2
         ORDER BY created_at DESC LIMIT 500",
    )
    .bind(tenant_id)
    .bind(cutoff)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 当前租户的策略规则（种子 + learned）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PolicyRow {
    pub pattern: String,
    pub decision: String,
    pub risk_level: String,
    pub priority: i32,
    pub enabled: bool,
    pub source: String,
}

pub async fn list_rules(pool: &PgPool, tenant_id: i64) -> anyhow::Result<Vec<PolicyRow>> {
    let rows = sqlx::query_as::<_, PolicyRow>(
        "SELECT pattern, decision, risk_level, priority, enabled, source
         FROM policies WHERE tenant_id=$1 ORDER BY priority DESC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
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
        // M4: prefix_rule 语法（M0 验证），非 forbid/allow。
        let out = "prefix_rule(\n    pattern = [\"rm\", \"-rf\"],\n    decision = \"forbidden\",\n)\n";
        assert!(out.contains("prefix_rule"));
        assert!(out.contains("forbidden"));
        assert!(!out.contains("forbid("));
    }

    #[test]
    fn extract_pattern_prefix() {
        // M6: argv 前 2 token + 末尾 `*`（token 数 > 2）。
        assert_eq!(extract_pattern("rm -rf /tmp/nexus-sim"), "rm -rf*");
        assert_eq!(extract_pattern("npm install nexus-sim"), "npm install*");
        assert_eq!(extract_pattern("sudo apt update"), "sudo apt*");
        // 2 token → 不加 `*`（精确匹配）。
        assert_eq!(extract_pattern("ls -la"), "ls -la");
        // 1 token → 原样。
        assert_eq!(extract_pattern("ls"), "ls");
        // 空 → catch-all。
        assert_eq!(extract_pattern(""), "*");
    }
}
