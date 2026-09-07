//! M20 评测中心 — 版本化 Golden Set + Rubric + 运行冻结快照（roadmap 评测体系 P1 MVP）。
//!
//! 核心思想：所有实体一律版本化、不可变，运行时绑定到具体版本集合（快照），
//! 从根上保证可复现、可追溯。复用 M17 skill_versions 的 version+content_ref 模式，
//! 泛化为 `entity_version` 六类骨架表（M20 激活 golden_set + rubric）。
//!
//! 评分仅确定性规则层（must_hit_points 命中率 + must_not 违规 + acceptable_answers
//! 匹配）；Judge LLM 逐点评分留 M21（P2）。批量评测运行经 HTTP self-call 复用
//! /v1/threads/{id}/turns + /v1/threads/{id}/items（M18 orchestrator 模式），
//! 不重构 turn_start drain（外科手术原则）。

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

use crate::auth::{Claims, JwtIssuer};

// ───────────────────────── 数据行结构 ─────────────────────────

#[derive(sqlx::FromRow, Serialize)]
pub struct GoldenSetRow {
    pub id: i64,
    pub tenant_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub active_version_id: Option<i64>,
    pub created_by: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct GoldenSetCaseRow {
    pub id: i64,
    pub golden_set_id: i64,
    pub tenant_id: i64,
    pub case_key: String,
    pub domain: Option<String>,
    pub difficulty: Option<String>,
    pub source: Option<String>,
    pub input_json: Value,
    pub expected_json: Value,
    pub phi_check_status: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct RubricRow {
    pub id: i64,
    pub tenant_id: i64,
    pub name: String,
    pub status: String,
    pub active_version_id: Option<i64>,
    pub created_by: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct EntityVersionRow {
    pub id: i64,
    pub tenant_id: i64,
    pub entity_type: String,
    pub entity_id: i64,
    pub version_no: i32,
    pub semver: String,
    pub manifest_hash: String,
    pub status: String,
    pub created_by: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub parent_id: Option<i64>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct EvalBatchRunRow {
    pub id: i64,
    pub tenant_id: i64,
    pub golden_set_id: i64,
    pub golden_set_version_id: i64,
    pub rubric_version_id: Option<i64>,
    pub judge_version_id: Option<i64>,
    pub thread_id: Option<Uuid>,
    pub snapshot_manifest: Value,
    pub snapshot_hash: String,
    pub trigger_type: String,
    pub status: String,
    pub baseline_run_id: Option<i64>,
    pub gate_result: Option<String>,
    pub aggregate: Option<Value>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct EvalCaseResultRow {
    pub id: i64,
    pub run_id: i64,
    pub case_id: i64,
    pub case_key: String,
    pub turn_id: Option<i64>,
    pub agent_output: Option<String>,
    pub scores: Value,
    pub created_at: DateTime<Utc>,
}

// ───────────────────────── 请求结构 ─────────────────────────

#[derive(Deserialize)]
pub struct CreateGoldenSetReq {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct AddCaseReq {
    pub case_key: String,
    pub domain: Option<String>,
    pub difficulty: Option<String>,
    pub source: Option<String>,
    pub input_json: Value,
    pub expected_json: Value,
}

#[derive(Deserialize, Default)]
pub struct PublishReq {
    pub bump_type: Option<String>, // patch / minor / major，默认 patch
}

#[derive(Deserialize)]
pub struct CreateRubricReq {
    pub name: String,
}

#[derive(Deserialize)]
pub struct PublishRubricReq {
    pub bump_type: Option<String>,
    pub criteria: Value, // [{name, weight, tolerance}, ...]
}

#[derive(Deserialize)]
pub struct StartRunReq {
    pub golden_set_id: i64,
    pub rubric_id: Option<i64>,
    pub thread_id: Uuid,
    pub trigger_type: Option<String>,
    pub judge_config_id: Option<i64>,
}

// ───────────────────────── 辅助函数 ─────────────────────────

fn sha256_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 语义化版本号递增。首次发布返回 1.0.0。
fn bump_semver(current: Option<&str>, bump_type: &str) -> String {
    match current {
        None => "1.0.0".to_string(),
        Some(v) => {
            let parts: Vec<&str> = v.split('.').collect();
            if parts.len() != 3 {
                return "1.0.0".to_string();
            }
            let mut major: u32 = parts[0].parse().unwrap_or(1);
            let mut minor: u32 = parts[1].parse().unwrap_or(0);
            let mut patch: u32 = parts[2].parse().unwrap_or(0);
            match bump_type {
                "major" => {
                    major += 1;
                    minor = 0;
                    patch = 0;
                }
                "minor" => {
                    minor += 1;
                    patch = 0;
                }
                _ => {
                    patch += 1;
                }
            }
            format!("{major}.{minor}.{patch}")
        }
    }
}

// ───────────────────────── Golden Set 管理 ─────────────────────────

pub async fn create_golden_set(
    pool: &PgPool,
    tenant_id: i64,
    user_id: i64,
    req: CreateGoldenSetReq,
) -> Result<GoldenSetRow> {
    sqlx::query_as::<_, GoldenSetRow>(
        "INSERT INTO golden_sets (tenant_id, name, description, status, created_by)
         VALUES ($1, $2, $3, 'draft', $4)
         RETURNING id, tenant_id, name, description, status, active_version_id,
                   created_by, created_at, updated_at",
    )
    .bind(tenant_id)
    .bind(req.name)
    .bind(req.description)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("create_golden_set: {e:?}"))
}

pub async fn list_golden_sets(pool: &PgPool, tenant_id: i64) -> Result<Vec<GoldenSetRow>> {
    sqlx::query_as::<_, GoldenSetRow>(
        "SELECT id, tenant_id, name, description, status, active_version_id,
                created_by, created_at, updated_at
         FROM golden_sets WHERE tenant_id=$1 ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list_golden_sets: {e:?}"))
}

pub async fn get_golden_set(pool: &PgPool, tenant_id: i64, id: i64) -> Result<GoldenSetRow> {
    sqlx::query_as::<_, GoldenSetRow>(
        "SELECT id, tenant_id, name, description, status, active_version_id,
                created_by, created_at, updated_at
         FROM golden_sets WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("get_golden_set: {e:?}"))
}

pub async fn add_case(
    pool: &PgPool,
    tenant_id: i64,
    golden_set_id: i64,
    req: AddCaseReq,
) -> Result<i64> {
    // locked 的 golden_set 不允许新增 case（版本不可变）
    let gs: Option<(String,)> =
        sqlx::query_as("SELECT status FROM golden_sets WHERE id=$1 AND tenant_id=$2")
            .bind(golden_set_id)
            .bind(tenant_id)
            .fetch_optional(pool)
            .await
            .context("add_case: load gs")?;
    let (status,) = gs.ok_or_else(|| anyhow!("golden_set not found"))?;
    if status != "draft" {
        return Err(anyhow!("golden_set is {status}, cannot add cases (immutable)"));
    }
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO golden_set_cases (golden_set_id, tenant_id, case_key, domain, difficulty,
         source, input_json, expected_json)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(golden_set_id)
    .bind(tenant_id)
    .bind(&req.case_key)
    .bind(req.domain.as_deref())
    .bind(req.difficulty.as_deref())
    .bind(req.source.as_deref())
    .bind(&req.input_json)
    .bind(&req.expected_json)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("add_case: {e:?}"))?;
    Ok(row.0)
}

pub async fn list_cases(pool: &PgPool, tenant_id: i64, golden_set_id: i64) -> Result<Vec<GoldenSetCaseRow>> {
    sqlx::query_as::<_, GoldenSetCaseRow>(
        "SELECT id, golden_set_id, tenant_id, case_key, domain, difficulty, source,
                input_json, expected_json, phi_check_status, created_at
         FROM golden_set_cases WHERE golden_set_id=$1 AND tenant_id=$2
         ORDER BY created_at ASC",
    )
    .bind(golden_set_id)
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list_cases: {e:?}"))
}

/// 发布 GoldenSet 版本：序列化全部 case 快照 → SHA-256 → INSERT entity_version(locked)
/// → UPDATE golden_sets.active_version_id + status=locked。版本不可变。
pub async fn publish_golden_set_version(
    pool: &PgPool,
    tenant_id: i64,
    golden_set_id: i64,
    user_id: i64,
    req: PublishReq,
) -> Result<EntityVersionRow> {
    let bump = req.bump_type.as_deref().unwrap_or("patch");
    let mut tx = pool.begin().await.map_err(|e| anyhow!("tx: {e:?}"))?;

    // 加载 golden_set + 当前 active 版本（确定 semver 基线）
    let gs: Option<(String, Option<i64>)> = sqlx::query_as(
        "SELECT status, active_version_id FROM golden_sets WHERE id=$1 AND tenant_id=$2",
    )
    .bind(golden_set_id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| anyhow!("publish: load gs: {e:?}"))?;
    let (status, active_vid) = gs.ok_or_else(|| anyhow!("golden_set not found"))?;

    // 查当前版本的 semver + version_no（如有）
    let (prev_semver, prev_vno): (Option<String>, Option<i32>) = match active_vid {
        Some(vid) => {
            let r: Option<(String, i32)> = sqlx::query_as(
                "SELECT semver, version_no FROM entity_version WHERE id=$1",
            )
            .bind(vid)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| anyhow!("publish: load prev ver: {e:?}"))?;
            match r {
                Some((s, v)) => (Some(s), Some(v)),
                None => (None, None),
            }
        }
        None => (None, None),
    };
    let new_semver = bump_semver(prev_semver.as_deref(), bump);
    let new_vno = prev_vno.unwrap_or(0) + 1;

    // 序列化全部 case 为 content_ref（快照全文）
    let cases: Vec<(i64, String, Value, Value)> = sqlx::query_as(
        "SELECT id, case_key, input_json, expected_json FROM golden_set_cases
         WHERE golden_set_id=$1 AND tenant_id=$2 ORDER BY id ASC",
    )
    .bind(golden_set_id)
    .bind(tenant_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| anyhow!("publish: load cases: {e:?}"))?;
    let cases_json: Vec<Value> = cases
        .iter()
        .map(|(id, key, inp, exp)| {
            json!({"id": id, "case_key": key, "input_json": inp, "expected_json": exp})
        })
        .collect();
    let content = json!({"golden_set_id": golden_set_id, "cases": cases_json}).to_string();
    let manifest_hash = sha256_hex(&content);

    let row = sqlx::query_as::<_, EntityVersionRow>(
        "INSERT INTO entity_version (tenant_id, entity_type, entity_id, version_no, semver,
         manifest_hash, content_ref, status, created_by, parent_id)
         VALUES ($1, 'golden_set', $2, $3, $4, $5, $6, 'locked', $7, $8)
         RETURNING id, tenant_id, entity_type, entity_id, version_no, semver, manifest_hash,
                   status, created_by, created_at, parent_id",
    )
    .bind(tenant_id)
    .bind(golden_set_id)
    .bind(new_vno)
    .bind(&new_semver)
    .bind(&manifest_hash)
    .bind(&content)
    .bind(user_id)
    .bind(active_vid)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| anyhow!("publish: insert version: {e:?}"))?;

    // 锁定 golden_set（不可再增删 case）+ 更新 active_version_id
    let _ = status; // draft → locked
    sqlx::query(
        "UPDATE golden_sets SET active_version_id=$3, status='locked', updated_at=NOW()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(golden_set_id)
    .bind(tenant_id)
    .bind(row.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| anyhow!("publish: update gs: {e:?}"))?;

    tx.commit().await.map_err(|e| anyhow!("commit: {e:?}"))?;
    Ok(row)
}

// ───────────────────────── Rubric 管理 ─────────────────────────

pub async fn create_rubric(
    pool: &PgPool,
    tenant_id: i64,
    user_id: i64,
    req: CreateRubricReq,
) -> Result<RubricRow> {
    sqlx::query_as::<_, RubricRow>(
        "INSERT INTO rubrics (tenant_id, name, status, created_by)
         VALUES ($1, $2, 'draft', $3)
         RETURNING id, tenant_id, name, status, active_version_id, created_by, created_at, updated_at",
    )
    .bind(tenant_id)
    .bind(req.name)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("create_rubric: {e:?}"))
}

pub async fn list_rubrics(pool: &PgPool, tenant_id: i64) -> Result<Vec<RubricRow>> {
    sqlx::query_as::<_, RubricRow>(
        "SELECT id, tenant_id, name, status, active_version_id, created_by, created_at, updated_at
         FROM rubrics WHERE tenant_id=$1 ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list_rubrics: {e:?}"))
}

pub async fn publish_rubric_version(
    pool: &PgPool,
    tenant_id: i64,
    rubric_id: i64,
    user_id: i64,
    req: PublishRubricReq,
) -> Result<EntityVersionRow> {
    let bump = req.bump_type.as_deref().unwrap_or("patch");
    let mut tx = pool.begin().await.map_err(|e| anyhow!("tx: {e:?}"))?;

    let rb: Option<(Option<i64>,)> = sqlx::query_as(
        "SELECT active_version_id FROM rubrics WHERE id=$1 AND tenant_id=$2",
    )
    .bind(rubric_id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| anyhow!("publish rubric: load: {e:?}"))?;
    let (active_vid,) = rb.ok_or_else(|| anyhow!("rubric not found"))?;

    let (prev_semver, prev_vno): (Option<String>, Option<i32>) = match active_vid {
        Some(vid) => {
            let r: Option<(String, i32)> = sqlx::query_as(
                "SELECT semver, version_no FROM entity_version WHERE id=$1",
            )
            .bind(vid)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| anyhow!("publish rubric: load prev: {e:?}"))?;
            match r {
                Some((s, v)) => (Some(s), Some(v)),
                None => (None, None),
            }
        }
        None => (None, None),
    };
    let new_semver = bump_semver(prev_semver.as_deref(), bump);
    let new_vno = prev_vno.unwrap_or(0) + 1;

    let content = req.criteria.to_string();
    let manifest_hash = sha256_hex(&content);

    let row = sqlx::query_as::<_, EntityVersionRow>(
        "INSERT INTO entity_version (tenant_id, entity_type, entity_id, version_no, semver,
         manifest_hash, content_ref, status, created_by, parent_id)
         VALUES ($1, 'rubric', $2, $3, $4, $5, $6, 'locked', $7, $8)
         RETURNING id, tenant_id, entity_type, entity_id, version_no, semver, manifest_hash,
                   status, created_by, created_at, parent_id",
    )
    .bind(tenant_id)
    .bind(rubric_id)
    .bind(new_vno)
    .bind(&new_semver)
    .bind(&manifest_hash)
    .bind(&content)
    .bind(user_id)
    .bind(active_vid)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| anyhow!("publish rubric: insert: {e:?}"))?;

    sqlx::query(
        "UPDATE rubrics SET active_version_id=$3, status='locked', updated_at=NOW()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(rubric_id)
    .bind(tenant_id)
    .bind(row.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| anyhow!("publish rubric: update: {e:?}"))?;

    tx.commit().await.map_err(|e| anyhow!("commit: {e:?}"))?;
    Ok(row)
}

// ───────────────────────── 评分器（确定性规则层） ─────────────────────────

/// 逐 case 评分：must_hit_points 命中率 + must_not 违规 + acceptable_answers 匹配。
/// Judge LLM 逐点评分留 M21。
pub fn score_case(expected: &Value, output: &str) -> Value {
    let must_hit = expected
        .get("must_hit_points")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.as_str())
                .map(|point| {
                    let hit = output.contains(point);
                    json!({"point": point, "hit": hit})
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let hit_count = must_hit.iter().filter(|h| h["hit"].as_bool() == Some(true)).count();
    let total_points = must_hit.len();
    let must_hit_rate = if total_points > 0 {
        hit_count as f64 / total_points as f64
    } else {
        1.0
    };

    let must_not_violations: Vec<String> = expected
        .get("must_not")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.as_str())
                .filter(|forbidden| output.contains(forbidden))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let acceptable_match = match expected.get("acceptable_answers").and_then(|v| v.as_array()) {
        Some(arr) if !arr.is_empty() => arr
            .iter()
            .filter_map(|a| a.as_str())
            .any(|a| output.contains(a)),
        _ => true, // 无可接受答案约束 → 跳过此判定
    };

    let passed = must_not_violations.is_empty() && must_hit_rate == 1.0 && acceptable_match;
    json!({
        "must_hit_points": must_hit,
        "must_hit_rate": must_hit_rate,
        "must_not_violations": must_not_violations,
        "acceptable_match": acceptable_match,
        "passed": passed,
    })
}

// ───────────────────────── 批量评测运行 ─────────────────────────

/// 启动批量评测运行：冻结快照 → 逐 case 驱动 agent turn → 评分 → 聚合。
pub async fn start_run(
    pool: &PgPool,
    base_url: &str,
    jwt: &JwtIssuer,
    claims: &Claims,
    req: StartRunReq,
) -> Result<i64> {
    // 1. 加载 golden_set 的 locked 版本（不可变快照）
    let gs: Option<(Option<i64>,)> = sqlx::query_as(
        "SELECT active_version_id FROM golden_sets WHERE id=$1 AND tenant_id=$2",
    )
    .bind(req.golden_set_id)
    .bind(claims.tid)
    .fetch_optional(pool)
    .await
    .context("start_run: load gs")?;
    let (gs_vid,) = gs.ok_or_else(|| anyhow!("golden_set not found"))?;
    let gs_vid = gs_vid.ok_or_else(|| anyhow!("golden_set has no published version"))?;

    let gv: (String, String, String) = sqlx::query_as(
        "SELECT semver, manifest_hash, content_ref FROM entity_version WHERE id=$1 AND status='locked'",
    )
    .bind(gs_vid)
    .fetch_one(pool)
    .await
    .context("start_run: load gs version")?;
    let (gs_semver, gs_hash, gs_content) = gv;

    // 2. 加载 rubric 的 locked 版本（可选）
    let (rb_vid, rb_semver, rb_hash): (Option<i64>, Option<String>, Option<String>) =
        match req.rubric_id {
            Some(rid) => {
                let rb: Option<(Option<i64>,)> = sqlx::query_as(
                    "SELECT active_version_id FROM rubrics WHERE id=$1 AND tenant_id=$2",
                )
                .bind(rid)
                .bind(claims.tid)
                .fetch_optional(pool)
                .await?;
                match rb {
                    Some((Some(vid),)) => {
                        let r: (String, String) = sqlx::query_as(
                            "SELECT semver, manifest_hash FROM entity_version WHERE id=$1 AND status='locked'",
                        )
                        .bind(vid)
                        .fetch_one(pool)
                        .await?;
                        (Some(vid), Some(r.0), Some(r.1))
                    }
                    _ => (None, None, None),
                }
            }
            None => (None, None, None),
        };

    // 2b. 加载 judge_config 的 locked 版本（可选，M21）
    let (jc_vid, jc_semver, jc_hash, jc_content): (Option<i64>, Option<String>, Option<String>, Option<String>) =
        match req.judge_config_id {
            Some(jid) => {
                let jc: Option<(Option<i64>,)> = sqlx::query_as(
                    "SELECT active_version_id FROM judge_configs WHERE id=$1 AND tenant_id=$2",
                )
                .bind(jid)
                .bind(claims.tid)
                .fetch_optional(pool)
                .await?;
                match jc {
                    Some((Some(vid),)) => {
                        let j: (String, String, String) = sqlx::query_as(
                            "SELECT semver, manifest_hash, content_ref FROM entity_version WHERE id=$1 AND status='locked'",
                        )
                        .bind(vid)
                        .fetch_one(pool)
                        .await?;
                        (Some(vid), Some(j.0), Some(j.1), Some(j.2))
                    }
                    _ => (None, None, None, None),
                }
            }
            None => (None, None, None, None),
        };

    // 3. 构造 Manifest → snapshot_hash
    let manifest = json!({
        "golden_set": {"id": req.golden_set_id, "version_id": gs_vid, "semver": gs_semver, "manifest_hash": gs_hash},
        "rubric": {"version_id": rb_vid, "semver": rb_semver, "manifest_hash": rb_hash},
        "judge": {"version_id": jc_vid, "semver": jc_semver, "manifest_hash": jc_hash},
        "agent": {"thread_id": req.thread_id.to_string()},
    });
    let snapshot_hash = sha256_hex(&manifest.to_string());

    // 4. INSERT eval_batch_runs(running)
    let run_row: (i64,) = sqlx::query_as(
        "INSERT INTO eval_batch_runs (tenant_id, golden_set_id, golden_set_version_id,
         rubric_version_id, judge_version_id, thread_id, snapshot_manifest, snapshot_hash, trigger_type, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'running') RETURNING id",
    )
    .bind(claims.tid)
    .bind(req.golden_set_id)
    .bind(gs_vid)
    .bind(rb_vid)
    .bind(jc_vid)
    .bind(req.thread_id)
    .bind(&manifest)
    .bind(&snapshot_hash)
    .bind(req.trigger_type.as_deref().unwrap_or("manual"))
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("start_run: insert run: {e:?}"))?;
    let run_id = run_row.0;

    // 5. 逐 case 驱动 agent turn → 评分 → 落库
    let token = jwt
        .issue(claims.clone())
        .map_err(|e| anyhow!("mint service jwt: {e:?}"))?;
    let auth_hdr = format!("Bearer {token}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    // 反序列化 content_ref 取 cases 快照
    let content_val: Value = serde_json::from_str(&gs_content)
        .map_err(|e| anyhow!("start_run: parse content_ref: {e:?}"))?;
    let cases_arr = content_val
        .get("cases")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut passed_count = 0u32;
    let mut hit_rate_sum = 0.0f64;
    let mut violation_count = 0u32;
    let mut judge_score_sum = 0.0f64;
    let total = cases_arr.len() as u32;

    for case in &cases_arr {
        let case_id = case["id"].as_i64().unwrap_or(0);
        let case_key = case["case_key"].as_str().unwrap_or("").to_string();
        let input = case["input_json"].get("user_query")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let expected = &case["expected_json"];

        let (turn_id, output) = match run_case_turn(&client, base_url, &auth_hdr, req.thread_id, input).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(case_key = %case_key, error = %e, "eval: turn failed, recording as failed");
                let scores = json!({"passed": false, "error": e.to_string()});
                let _ = sqlx::query(
                    "INSERT INTO eval_case_results (run_id, case_id, case_key, scores)
                     VALUES ($1, $2, $3, $4) ON CONFLICT (run_id, case_id) DO NOTHING",
                )
                .bind(run_id)
                .bind(case_id)
                .bind(&case_key)
                .bind(&scores)
                .execute(pool)
                .await;
                continue;
            }
        };
        let out_str = output.unwrap_or_default();
        let det_scores = score_case(expected, &out_str);
        let det_hit_rate = det_scores["must_hit_rate"].as_f64().unwrap_or(0.0);
        let det_violations = det_scores["must_not_violations"]
            .as_array()
            .map(|a| a.len() as u32)
            .unwrap_or(0);
        let det_passed = det_scores["passed"].as_bool() == Some(true);

        // M21: 叠加 Judge LLM 评分（若 run 关联 judge_config）
        let judge_scores = if let Some(ref jc) = jc_content {
            match judge_score(jc, None, input, &out_str).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(case_key = %case_key, error = %e, "judge: failed, skipping judge layer");
                    json!({"error": e.to_string(), "overall_score": 0.0, "passed": false})
                }
            }
        } else {
            json!(null)
        };

        let judge_passed = judge_scores["passed"].as_bool() == Some(true);
        let combined_passed = if jc_content.is_some() { det_passed && judge_passed } else { det_passed };
        let judge_overall = judge_scores["overall_score"].as_f64().unwrap_or(0.0);

        let scores = if jc_content.is_some() {
            json!({
                "deterministic": det_scores,
                "judge": judge_scores,
                "passed": combined_passed,
            })
        } else {
            det_scores
        };

        if combined_passed {
            passed_count += 1;
        }
        hit_rate_sum += det_hit_rate;
        violation_count += det_violations;
        if jc_content.is_some() {
            judge_score_sum += judge_overall;
        }

        sqlx::query(
            "INSERT INTO eval_case_results (run_id, case_id, case_key, turn_id, agent_output, scores)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (run_id, case_id) DO UPDATE SET turn_id=$4, agent_output=$5, scores=$6",
        )
        .bind(run_id)
        .bind(case_id)
        .bind(&case_key)
        .bind(turn_id)
        .bind(&out_str)
        .bind(&scores)
        .execute(pool)
        .await
        .map_err(|e| anyhow!("start_run: insert result: {e:?}"))?;
    }

    // 6. 聚合 → UPDATE eval_batch_runs(completed)
    let accuracy = if total > 0 { passed_count as f64 / total as f64 } else { 0.0 };
    let avg_hit_rate = if total > 0 { hit_rate_sum / total as f64 } else { 0.0 };
    let avg_judge_score = if total > 0 && jc_content.is_some() { judge_score_sum / total as f64 } else { 0.0 };
    let aggregate = json!({
        "total_cases": total,
        "passed": passed_count,
        "accuracy": accuracy,
        "avg_must_hit_rate": avg_hit_rate,
        "must_not_violation_count": violation_count,
        "avg_judge_score": avg_judge_score,
        "judge_enabled": jc_content.is_some(),
    });
    sqlx::query(
        "UPDATE eval_batch_runs SET status='completed', aggregate=$3, finished_at=NOW()
         WHERE id=$1",
    )
    .bind(run_id)
    .bind(claims.tid)
    .bind(&aggregate)
    .execute(pool)
    .await
    .map_err(|e| anyhow!("start_run: finalize: {e:?}"))?;

    Ok(run_id)
}

/// 驱动一次 turn（阻塞至 completed），SIMULATE_APPROVAL 下自动 resolve 审批。
/// 复用 M18 orchestrator 的 HTTP self-call 模式。返回 (turn_id, agent_output)。
async fn run_case_turn(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    thread_id: Uuid,
    input: &str,
) -> Result<(i64, Option<String>)> {
    let url = format!("{base_url}/v1/threads/{thread_id}/turns");
    let body = json!({ "input": input });
    let turn_fut = client.post(&url).header("Authorization", auth).json(&body).send();
    tokio::pin!(turn_fut);

    let turn_id: i64;
    loop {
        tokio::select! {
            r = &mut turn_fut => {
                let resp = r.map_err(|e| anyhow!("turn http: {e}"))?;
                let st = resp.status();
                let v: Value = resp.json().await.map_err(|e| anyhow!("turn json: {e}"))?;
                if !st.is_success() {
                    return Err(anyhow!("turn failed: {st} body={v}"));
                }
                turn_id = v["turn_id"].as_i64().unwrap_or(0);
                if v["status"].as_str() != Some("completed") {
                    return Err(anyhow!("turn not completed: {v}"));
                }
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                let _ = resolve_pending(client, base_url, auth, thread_id).await;
            }
        }
    }

    // 读取 agent 输出（最后一条 agentMessage）
    let items_url = format!("{base_url}/v1/threads/{thread_id}/items");
    let resp = client
        .get(&items_url)
        .header("Authorization", auth)
        .send()
        .await
        .map_err(|e| anyhow!("items http: {e}"))?;
    let items: Vec<Value> = resp.json().await.unwrap_or_default();
    // 取最后一条 agentMessage；若无（如 SIMULATE 模式仅产 item/completed），
    // 回退到最后一条 item/completed 的 content_ref。
    let output = items.iter().rev().find_map(|it| {
        if it["item_type"].as_str() == Some("agentMessage") {
            it["content_ref"].as_str().map(|s| s.to_string())
        } else {
            None
        }
    }).or_else(|| {
        items.iter().rev().find_map(|it| {
            if it["item_type"].as_str() == Some("item/completed") {
                it["content_ref"].as_str().map(|s| s.to_string())
            } else {
                None
            }
        })
    });
    Ok((turn_id, output))
}

async fn resolve_pending(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    thread_id: Uuid,
) -> Result<()> {
    let url = format!("{base_url}/v1/threads/{thread_id}/approvals");
    let resp = client
        .get(&url)
        .header("Authorization", auth)
        .send()
        .await
        .map_err(|e| anyhow!("approvals http: {e}"))?;
    let arr: Vec<Value> = resp.json().await.unwrap_or_default();
    for ap in arr {
        if ap["status"].as_str() == Some("pending") {
            if let Some(aid) = ap["id"].as_i64() {
                let _ = client
                    .post(format!("{base_url}/v1/approvals/{aid}/resolve"))
                    .header("Authorization", auth)
                    .json(&json!({"decision":"approve"}))
                    .send()
                    .await;
            }
        }
    }
    Ok(())
}

pub async fn list_runs(pool: &PgPool, tenant_id: i64, limit: i64) -> Result<Vec<EvalBatchRunRow>> {
    sqlx::query_as::<_, EvalBatchRunRow>(
        "SELECT id, tenant_id, golden_set_id, golden_set_version_id, rubric_version_id,
                judge_version_id, thread_id, snapshot_manifest, snapshot_hash, trigger_type, status,
                baseline_run_id, gate_result, aggregate, started_at, finished_at
         FROM eval_batch_runs WHERE tenant_id=$1 ORDER BY started_at DESC LIMIT $2",
    )
    .bind(tenant_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list_runs: {e:?}"))
}

pub async fn get_run(pool: &PgPool, tenant_id: i64, run_id: i64) -> Result<(EvalBatchRunRow, Vec<EvalCaseResultRow>)> {
    let run = sqlx::query_as::<_, EvalBatchRunRow>(
        "SELECT id, tenant_id, golden_set_id, golden_set_version_id, rubric_version_id,
                judge_version_id, thread_id, snapshot_manifest, snapshot_hash, trigger_type, status,
                baseline_run_id, gate_result, aggregate, started_at, finished_at
         FROM eval_batch_runs WHERE id=$1 AND tenant_id=$2",
    )
    .bind(run_id)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("get_run: {e:?}"))?;
    let results = sqlx::query_as::<_, EvalCaseResultRow>(
        "SELECT id, run_id, case_id, case_key, turn_id, agent_output, scores, created_at
         FROM eval_case_results WHERE run_id=$1 ORDER BY id ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("get_run: results: {e:?}"))?;
    Ok((run, results))
}

// ───────────────────────── 基线对比 ─────────────────────────

#[derive(Serialize)]
pub struct CompareResult {
    pub incomparable: bool,
    pub baseline_version_id: i64,
    pub run_version_id: i64,
    pub diffs: Vec<Value>,
}

/// 对比两 run：同 golden_set 版本才可比（不同版本标记不可直接对比）。
/// 按 case_key JOIN 两次 eval_case_results → case 级 diff（pass↔fail 变化）。
pub async fn compare_runs(
    pool: &PgPool,
    tenant_id: i64,
    run_id: i64,
    baseline_run_id: i64,
) -> Result<CompareResult> {
    let run: Option<(i64,)> = sqlx::query_as(
        "SELECT golden_set_version_id FROM eval_batch_runs WHERE id=$1 AND tenant_id=$2",
    )
    .bind(run_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .context("compare: load run")?;
    let (run_vid,) = run.ok_or_else(|| anyhow!("run not found"))?;
    let baseline: Option<(i64,)> = sqlx::query_as(
        "SELECT golden_set_version_id FROM eval_batch_runs WHERE id=$1 AND tenant_id=$2",
    )
    .bind(baseline_run_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .context("compare: load baseline")?;
    let (base_vid,) = baseline.ok_or_else(|| anyhow!("baseline run not found"))?;

    let incomparable = run_vid != base_vid;

    // 按 case_key JOIN 两次结果（scores->>'passed' 返回 TEXT，需 ::boolean cast 匹配 Option<bool>）
    let rows: Vec<(String, Option<bool>, Option<bool>)> = sqlx::query_as(
        "SELECT a.case_key,
                (a.scores->>'passed')::boolean AS baseline_passed,
                (b.scores->>'passed')::boolean AS run_passed
         FROM eval_case_results a
         JOIN eval_case_results b ON a.case_key=b.case_key AND a.run_id=$1 AND b.run_id=$2
         ORDER BY a.case_key",
    )
    .bind(baseline_run_id)
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("compare: join: {e:?}"))?;
    let diffs: Vec<Value> = rows
        .iter()
        .map(|(key, bp, rp)| {
            let b = bp.unwrap_or(false);
            let r = rp.unwrap_or(false);
            json!({
                "case_key": key,
                "baseline_passed": b,
                "run_passed": r,
                "delta": if b == r { "same" } else if r { "improved" } else { "regressed" },
            })
        })
        .collect();

    Ok(CompareResult {
        incomparable,
        baseline_version_id: base_vid,
        run_version_id: run_vid,
        diffs,
    })
}

// ───────────────────────── Judge LLM 评分层（M21） ─────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct JudgeConfigRow {
    pub id: i64,
    pub tenant_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub active_version_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct CreateJudgeConfigReq {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct PublishJudgeConfigReq {
    pub bump_type: Option<String>,
    /// {prompt_template, dimensions:[{name,weight,description}], judge_model, temperature}
    pub config: Value,
}

pub async fn create_judge_config(
    pool: &PgPool,
    tenant_id: i64,
    req: CreateJudgeConfigReq,
) -> Result<JudgeConfigRow> {
    sqlx::query_as::<_, JudgeConfigRow>(
        "INSERT INTO judge_configs (tenant_id, name, description, status)
         VALUES ($1, $2, $3, 'draft')
         RETURNING id, tenant_id, name, description, status, active_version_id, created_at, updated_at",
    )
    .bind(tenant_id)
    .bind(&req.name)
    .bind(req.description)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("create judge_config: {e:?}"))
}

pub async fn list_judge_configs(pool: &PgPool, tenant_id: i64) -> Result<Vec<JudgeConfigRow>> {
    sqlx::query_as::<_, JudgeConfigRow>(
        "SELECT id, tenant_id, name, description, status, active_version_id, created_at, updated_at
         FROM judge_configs WHERE tenant_id=$1 ORDER BY id",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list judge_configs: {e:?}"))
}

/// 添加评分维度（仅 draft 状态）。
pub async fn add_judge_dimension(
    pool: &PgPool,
    tenant_id: i64,
    jc_id: i64,
    dimension: Value, // {name, weight, description}
) -> Result<()> {
    let jc: (String,) = sqlx::query_as(
        "SELECT status FROM judge_configs WHERE id=$1 AND tenant_id=$2",
    )
    .bind(jc_id)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("add dimension: load: {e:?}"))?;
    if jc.0 != "draft" {
        return Err(anyhow!("judge_config is not draft, cannot add dimension"));
    }
    // 维度存到 description 字段的 JSON 数组（MVP，不建独立表）
    let mut dims: Vec<Value> = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT description FROM judge_configs WHERE id=$1",
    )
    .bind(jc_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("add dimension: load desc: {e:?}"))?
    .0
    .and_then(|s| serde_json::from_str(&s).ok())
    .unwrap_or_default();
    dims.push(dimension);
    let serialized = serde_json::to_string(&dims).unwrap_or_else(|_| "[]".to_string());
    sqlx::query("UPDATE judge_configs SET description=$3, updated_at=NOW() WHERE id=$1 AND tenant_id=$2")
        .bind(jc_id)
        .bind(tenant_id)
        .bind(&serialized)
        .execute(pool)
        .await
        .map_err(|e| anyhow!("add dimension: update: {e:?}"))?;
    Ok(())
}

pub async fn publish_judge_config_version(
    pool: &PgPool,
    tenant_id: i64,
    jc_id: i64,
    user_id: i64,
    req: PublishJudgeConfigReq,
) -> Result<EntityVersionRow> {
    let bump = req.bump_type.as_deref().unwrap_or("patch");
    let mut tx = pool.begin().await.map_err(|e| anyhow!("tx: {e:?}"))?;

    let jc: Option<(Option<i64>,)> = sqlx::query_as(
        "SELECT active_version_id FROM judge_configs WHERE id=$1 AND tenant_id=$2",
    )
    .bind(jc_id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| anyhow!("publish judge: load: {e:?}"))?;
    let (active_vid,) = jc.ok_or_else(|| anyhow!("judge_config not found"))?;

    let (prev_semver, prev_vno): (Option<String>, Option<i32>) = match active_vid {
        Some(vid) => {
            let r: Option<(String, i32)> = sqlx::query_as(
                "SELECT semver, version_no FROM entity_version WHERE id=$1",
            )
            .bind(vid)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| anyhow!("publish judge: load prev: {e:?}"))?;
            match r {
                Some((s, v)) => (Some(s), Some(v)),
                None => (None, None),
            }
        }
        None => (None, None),
    };
    let new_semver = bump_semver(prev_semver.as_deref(), bump);
    let new_vno = prev_vno.unwrap_or(0) + 1;

    let content = req.config.to_string();
    let manifest_hash = sha256_hex(&content);

    let row = sqlx::query_as::<_, EntityVersionRow>(
        "INSERT INTO entity_version (tenant_id, entity_type, entity_id, version_no, semver,
         manifest_hash, content_ref, status, created_by, parent_id)
         VALUES ($1, 'judge_config', $2, $3, $4, $5, $6, 'locked', $7, $8)
         RETURNING id, tenant_id, entity_type, entity_id, version_no, semver, manifest_hash,
                   status, created_by, created_at, parent_id",
    )
    .bind(tenant_id)
    .bind(jc_id)
    .bind(new_vno)
    .bind(&new_semver)
    .bind(&manifest_hash)
    .bind(&content)
    .bind(user_id)
    .bind(active_vid)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| anyhow!("publish judge: insert: {e:?}"))?;

    sqlx::query(
        "UPDATE judge_configs SET active_version_id=$3, status='locked', updated_at=NOW()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(jc_id)
    .bind(tenant_id)
    .bind(row.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| anyhow!("publish judge: update: {e:?}"))?;

    tx.commit().await.map_err(|e| anyhow!("commit: {e:?}"))?;
    Ok(row)
}

/// Judge LLM 评分：给定 judge_config content + rubric + case input + agent_output，
/// 调用 dashscope chat API 逐维度评分。SIMULATE_JUDGE=1 返回合成评分。
pub async fn judge_score(
    judge_content: &str,
    _rubric_content: Option<&str>,
    case_input: &str,
    agent_output: &str,
) -> Result<Value> {
    let cfg: Value = serde_json::from_str(judge_content)
        .map_err(|e| anyhow!("judge: parse config: {e:?}"))?;
    let prompt_template = cfg["prompt_template"].as_str().unwrap_or(
        "你是临床研究评测专家。根据评分维度对 Agent 回复逐项打分（0.0-1.0），并给出理由。返回纯 JSON。"
    );
    let dimensions = cfg["dimensions"].as_array().cloned().unwrap_or_default();
    let env_model = std::env::var("NEXUS_MODEL").ok();
    let judge_model = cfg["judge_model"].as_str()
        .or_else(|| env_model.as_deref())
        .unwrap_or("deepseek-v4-pro");
    let temperature = cfg["temperature"].as_f64().unwrap_or(0.0);

    // SIMULATE 模式：返回合成评分
    if std::env::var("NEXUS_SIMULATE_JUDGE").ok().as_deref() == Some("1") {
        let dims_out: Vec<Value> = dimensions.iter().map(|d| {
            let name = d["name"].as_str().unwrap_or("dimension");
            json!({"name": name, "score": 0.75, "rationale": "simulated score"})
        }).collect();
        let overall = if dims_out.is_empty() { 0.75 } else { 0.75 };
        return Ok(json!({
            "dimensions": dims_out,
            "overall_score": overall,
            "passed": overall >= 0.6,
            "judge_model": judge_model,
            "simulated": true,
        }));
    }

    // 真实模式：调 dashscope chat completions
    let upstream = std::env::var("NEXUS_UPSTREAM_MODEL_URL")
        .map_err(|_| anyhow!("NEXUS_UPSTREAM_MODEL_URL not set"))?;
    let key = std::env::var("NEXUS_MODEL_KEY")
        .map_err(|_| anyhow!("NEXUS_MODEL_KEY not set"))?;
    let url = format!("{}/v1/chat/completions", upstream.trim_end_matches('/'));

    let dims_desc = if dimensions.is_empty() {
        "overall_quality".to_string()
    } else {
        dimensions.iter().filter_map(|d| d["name"].as_str()).collect::<Vec<_>>().join(", ")
    };
    let user_msg = format!(
        "## 评分维度\n{dims_desc}\n\n## Case 输入\n{case_input}\n\n## Agent 回复\n{agent_output}\n\n\
         请对每个维度返回 JSON：{{\"dimensions\":[{{\"name\":\"...\",\"score\":0.0,\"rationale\":\"...\"}}],\"overall_score\":0.0,\"passed\":true/false}}"
    );

    let body = json!({
        "model": judge_model,
        "messages": [
            {"role": "system", "content": prompt_template},
            {"role": "user", "content": user_msg},
        ],
        "temperature": temperature,
        "stream": false,
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    let resp = client.post(&url)
        .header("Authorization", format!("Bearer {key}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("judge: http: {e:?}"))?;
    let resp_json: Value = resp.json().await
        .map_err(|e| anyhow!("judge: parse resp: {e:?}"))?;
    let content = resp_json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}")
        .to_string();

    // 容错解析：提取首个 JSON 对象
    let parsed = extract_json_object(&content).unwrap_or_else(|| json!({}));
    let overall = parsed["overall_score"].as_f64().unwrap_or(0.0);
    let passed = parsed["passed"].as_bool().unwrap_or(overall >= 0.6);
    Ok(json!({
        "dimensions": parsed["dimensions"].clone(),
        "overall_score": overall,
        "passed": passed,
        "judge_model": judge_model,
        "raw": content,
    }))
}

/// 从文本中提取首个 JSON 对象（容错 LLM 输出可能含 markdown 代码块）。
fn extract_json_object(text: &str) -> Option<Value> {
    // 去 markdown 代码块
    let cleaned = text.replace("```json", "").replace("```", "");
    let trimmed = cleaned.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Some(v);
    }
    // 找首个 { 到匹配 }
    let start = trimmed.find('{')?;
    let mut depth = 0i32;
    let mut end = 0usize;
    for (i, c) in trimmed[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => { depth -= 1; if depth == 0 { end = start + i + 1; break; } }
            _ => {}
        }
    }
    if depth == 0 && end > start {
        serde_json::from_str(&trimmed[start..end]).ok()
    } else {
        None
    }
}

// ───────────────────────── 失败样本回流（M21） ─────────────────────────

#[derive(Deserialize)]
pub struct CaseFromTurnReq {
    pub turn_id: i64,
    pub case_key: String,
    pub expected_json: Value,
    pub user_query: Option<String>,
}

/// 从生产 turn 的 agent_output 创建新 golden case（仅 draft golden set）。
/// 形成防退化闭环：失败样本 → 新 case → 未来 run 防退化。
pub async fn case_from_turn(
    pool: &PgPool,
    base_url: &str,
    jwt: &JwtIssuer,
    claims: &Claims,
    gs_id: i64,
    req: CaseFromTurnReq,
) -> Result<i64> {
    // 校验 golden_set draft
    let gs: (String,) = sqlx::query_as(
        "SELECT status FROM golden_sets WHERE id=$1 AND tenant_id=$2",
    )
    .bind(gs_id)
    .bind(claims.tid)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("case_from_turn: load gs: {e:?}"))?;
    if gs.0 != "draft" {
        return Err(anyhow!("golden_set is not draft, cannot add cases"));
    }

    // 查 thread_id + 经 HTTP self-call 取 items（turn 的 agent_output）
    let turn: (String,) = sqlx::query_as(
        "SELECT thread_id::text FROM turns WHERE id=$1",
    )
    .bind(req.turn_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("case_from_turn: load turn: {e:?}"))?;
    let thread_id: Uuid = turn.0.parse().map_err(|_| anyhow!("invalid thread_id"))?;

    let token = jwt.issue(claims.clone())
        .map_err(|e| anyhow!("case_from_turn: mint jwt: {e:?}"))?;
    let auth_hdr = format!("Bearer {token}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let url = format!("{base_url}/v1/threads/{thread_id}/items");
    let resp = client.get(&url)
        .header("Authorization", &auth_hdr)
        .send()
        .await
        .map_err(|e| anyhow!("case_from_turn: http: {e:?}"))?;
    let items: Vec<Value> = resp.json().await
        .map_err(|e| anyhow!("case_from_turn: parse items: {e:?}"))?;

    // 提取 agent_output（最后一条 agentMessage，回退 item/completed）
    let agent_output = items.iter().rev().find_map(|it| {
        if it["item_type"].as_str() == Some("agentMessage") {
            it["content_ref"].as_str().map(|s| s.to_string())
        } else { None }
    }).or_else(|| {
        items.iter().rev().find_map(|it| {
            if it["item_type"].as_str() == Some("item/completed") {
                it["content_ref"].as_str().map(|s| s.to_string())
            } else { None }
        })
    }).unwrap_or_default();

    let input_json = json!({
        "user_query": req.user_query.as_deref().unwrap_or(""),
        "trace_turn_id": req.turn_id,
    });
    let source = format!("trace:{}", req.turn_id);

    let row: (i64,) = sqlx::query_as(
        "INSERT INTO golden_set_cases (golden_set_id, tenant_id, case_key, source, input_json, expected_json)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(gs_id)
    .bind(claims.tid)
    .bind(&req.case_key)
    .bind(&source)
    .bind(&input_json)
    .bind(&req.expected_json)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("case_from_turn: insert: {e:?}"))?;

    tracing::info!(case_id = row.0, turn_id = req.turn_id, agent_output_len = agent_output.len(), "case_from_turn: imported");
    Ok(row.0)
}

// ───────────────────────── 单测 ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn semver_bump() {
        assert_eq!(bump_semver(None, "patch"), "1.0.0");
        assert_eq!(bump_semver(Some("1.0.0"), "patch"), "1.0.1");
        assert_eq!(bump_semver(Some("1.0.1"), "minor"), "1.1.0");
        assert_eq!(bump_semver(Some("1.1.0"), "major"), "2.0.0");
        assert_eq!(bump_semver(Some("garbage"), "patch"), "1.0.0");
    }

    #[test]
    fn score_case_hit_and_violation() {
        let expected = json!({
            "must_hit_points": ["肌酐界值", "排除结论"],
            "must_not": ["医疗处置建议"],
            "acceptable_answers": [],
        });
        // 全命中 + 无违规 → passed
        let s = score_case(&expected, "引用方案肌酐界值48，给出排除结论");
        assert_eq!(s["passed"], true);
        assert_eq!(s["must_hit_rate"], 1.0);
        // 缺一个命中点 → not passed
        let s = score_case(&expected, "引用方案肌酐界值48");
        assert_eq!(s["passed"], false);
        assert!((s["must_hit_rate"].as_f64().unwrap() - 0.5).abs() < 1e-9);
        // 命中违规 → not passed
        let s = score_case(&expected, "引用方案肌酐界值48，给出排除结论，给出医疗处置建议");
        assert_eq!(s["passed"], false);
        assert!(!s["must_not_violations"].as_array().unwrap().is_empty());
    }

    #[test]
    fn score_case_no_constraints() {
        let expected = json!({});
        let s = score_case(&expected, "anything");
        // 无约束 → 全通过（空 must_hit 视为 1.0，无 must_not，无 acceptable）
        assert_eq!(s["passed"], true);
    }

    #[test]
    fn sha256_deterministic() {
        let a = sha256_hex("hello");
        let b = sha256_hex("hello");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert_ne!(sha256_hex("hello"), sha256_hex("world"));
    }

    #[test]
    fn extract_json_object_plain() {
        let v = extract_json_object(r#"{"overall_score": 0.8, "passed": true}"#).unwrap();
        assert_eq!(v["overall_score"], 0.8);
    }

    #[test]
    fn extract_json_object_markdown_wrapped() {
        let v = extract_json_object("```json\n{\"dimensions\":[],\"overall_score\":0.5,\"passed\":false}\n```").unwrap();
        assert_eq!(v["passed"], false);
    }

    #[test]
    fn extract_json_object_embedded() {
        let v = extract_json_object("here is my eval: {\"overall_score\": 0.9} done").unwrap();
        assert_eq!(v["overall_score"], 0.9);
    }

    #[tokio::test]
    async fn judge_score_simulate() {
        unsafe { std::env::set_var("NEXUS_SIMULATE_JUDGE", "1"); }
        let cfg = r#"{"prompt_template":"test","dimensions":[{"name":"accuracy","weight":0.5},{"name":"safety","weight":0.5}]}"#;
        let v = judge_score(cfg, None, "query", "response").await.unwrap();
        assert_eq!(v["simulated"], true);
        assert_eq!(v["overall_score"], 0.75);
        assert_eq!(v["passed"], true);
        let dims = v["dimensions"].as_array().unwrap();
        assert_eq!(dims.len(), 2);
        unsafe { std::env::remove_var("NEXUS_SIMULATE_JUDGE"); }
    }
}
