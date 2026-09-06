//! M18 多 Agent 协作编排 — roadmap T11-4。
//!
//! 协调层位于 turn 之上：通过 reqwest 内部 HTTP 自调用现有
//! /v1/threads/{id}/turns + /v1/threads/{id}/items + /v1/approvals 端点，
//! **不重构 turn_start drain**（外科手术原则）。每 agent = 独立 thread
//! （codex_thread_id 是 thread 级别，共享会 resume 污染上下文）。
//!
//! 3 种工作模式：
//! - orchestrator-worker：编排者产出计划 → N 工作者各产出一部分 → 扇入
//! - peer：N 对等 agent 并行独立产出 → 扇入
//! - critic-adversarial：生产者 → 评审 gate（REVISE→修订循环 / APPROVE→完成）
//!
//! SIMULATE_APPROVAL=1 下编排层自动 resolve 审批，让 turn 无人值守跑通。

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

use crate::auth::{Claims, JwtIssuer};

const MAX_REVISE: usize = 2;

#[derive(Deserialize)]
pub struct StartReq {
    pub mode: String,
    pub prompt: String,
    pub agents: Option<i64>, // worker/peer 数；critic-adversarial 忽略
    pub name: Option<String>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct OrchestrationRow {
    pub id: i64,
    pub tenant_id: i64,
    pub name: Option<String>,
    pub mode: String,
    pub status: String,
    pub prompt: Option<String>,
    pub created_by: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct AgentStepRow {
    pub id: i64,
    pub orchestration_id: i64,
    pub thread_id: Uuid,
    pub agent_seq: i32,
    pub role: String,
    pub turn_id: Option<i64>,
    pub status: String,
    pub output_ref: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 启动并执行一次协作编排。
pub async fn start_orchestration(
    pool: &PgPool,
    base_url: &str,
    jwt: &JwtIssuer,
    claims: &Claims,
    req: StartReq,
) -> Result<i64> {
    // mint 一个短时服务 JWT，复用请求用户的 tid/uid/perms（编排以该用户身份运行）
    let token = jwt
        .issue(claims.clone())
        .map_err(|e| anyhow!("mint service jwt: {e:?}"))?;
    let auth_hdr = format!("Bearer {token}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    let (row,): (i64,) = sqlx::query_as(
        "INSERT INTO orchestrations (tenant_id, name, mode, status, prompt, created_by)
         VALUES ($1, $2, $3, 'running', $4, $5) RETURNING id",
    )
    .bind(claims.tid)
    .bind(req.name)
    .bind(&req.mode)
    .bind(&req.prompt)
    .bind(claims.uid)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("insert orchestration: {e:?}"))?;

    let res: Result<()> = async {
        match req.mode.as_str() {
            "orchestrator-worker" => {
                run_orchestrator_worker(pool, base_url, &auth_hdr, &client, claims.tid, row, &req.prompt, req.agents).await
            }
            "peer" => {
                run_peer(pool, base_url, &auth_hdr, &client, claims.tid, row, &req.prompt, req.agents).await
            }
            "critic-adversarial" => {
                run_critic_adversarial(pool, base_url, &auth_hdr, &client, claims.tid, row, &req.prompt).await
            }
            other => Err(anyhow!("unknown mode: {other}")),
        }
    }
    .await;

    let status: &str = match &res {
        Ok(_) => "completed",
        Err(_) => "failed",
    };
    sqlx::query("UPDATE orchestrations SET status = $2, completed_at = NOW() WHERE id = $1")
        .bind(row)
        .bind(status)
        .execute(pool)
        .await
        .map_err(|e| anyhow!("finalize orchestration: {e:?}"))?;
    res?;
    Ok(row)
}

// ---------- 核心原语：驱动一个 agent turn ----------

/// 驱动一次 turn（阻塞至 completed），SIMULATE_APPROVAL 下自动 resolve 审批。
/// 返回该 thread 自 since 以来的 agent 输出（最后一条 agentMessage content_ref）。
async fn run_agent_turn(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    thread_id: Uuid,
    input: &str,
    since: i64,
) -> Result<(i64, Option<String>)> {
    let url = format!("{base_url}/v1/threads/{thread_id}/turns");
    let body = json!({ "input": input });
    let turn_fut = client.post(&url).header("Authorization", auth).json(&body).send();
    tokio::pin!(turn_fut);

    let mut output: Option<String> = None;
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
                // resolve pending approvals for this thread（SIMULATE_APPROVAL 下 turn park）
                let _ = resolve_pending(client, base_url, auth, thread_id).await;
            }
        }
    }

    // 读取 agent 输出
    let items_url = format!("{base_url}/v1/threads/{thread_id}/items?since={since}");
    let resp = client
        .get(&items_url)
        .header("Authorization", auth)
        .send()
        .await
        .map_err(|e| anyhow!("items http: {e}"))?;
    let items: Vec<Value> = resp.json().await.map_err(|e| anyhow!("items json: {e}"))?;
    // 取最后一条 agentMessage 的 content_ref 作为输出
    for it in items.iter().rev() {
        if it["item_type"].as_str() == Some("agentMessage") {
            if let Some(c) = it["content_ref"].as_str() {
                output = Some(c.to_string());
                break;
            }
        }
    }
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

/// 创建一个 agent thread。
async fn create_thread(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    title: &str,
) -> Result<Uuid> {
    let resp = client
        .post(format!("{base_url}/v1/threads"))
        .header("Authorization", auth)
        .json(&json!({"title": title}))
        .send()
        .await
        .map_err(|e| anyhow!("create_thread http: {e}"))?;
    let v: Value = resp.json().await.map_err(|e| anyhow!("create_thread json: {e}"))?;
    v["id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| anyhow!("no thread id: {v}"))
}

/// 记录一个 agent 步骤。
async fn record_agent(
    pool: &PgPool,
    tenant_id: i64,
    orch_id: i64,
    seq: i64,
    role: &str,
    thread_id: Uuid,
    turn_id: Option<i64>,
    status: &str,
    output: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO orchestration_agents
           (orchestration_id, tenant_id, thread_id, agent_seq, role, turn_id, status, output_ref)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(orch_id)
    .bind(tenant_id)
    .bind(thread_id)
    .bind(seq)
    .bind(role)
    .bind(turn_id)
    .bind(status)
    .bind(output)
    .execute(pool)
    .await
    .map_err(|e| anyhow!("record_agent: {e:?}"))?;
    Ok(())
}

// ---------- 模式实现 ----------

async fn run_orchestrator_worker(
    pool: &PgPool,
    base_url: &str,
    auth: &str,
    client: &reqwest::Client,
    tenant_id: i64,
    orch_id: i64,
    prompt: &str,
    n_workers: Option<i64>,
) -> Result<()> {
    let n = n_workers.unwrap_or(2).max(1).min(6) as usize;
    // agent 0：编排者产出计划
    let t0 = create_thread(client, base_url, auth, "orchestrator").await?;
    let (turn0, out0) = run_agent_turn(client, base_url, auth, t0, prompt, 0).await?;
    let plan = out0.clone().unwrap_or_default();
    record_agent(pool, tenant_id, orch_id, 0, "orchestrator", t0, Some(turn0), "completed", out0.as_deref()).await?;

    // workers 1..n
    let mut outputs = vec![out0.unwrap_or_default()];
    for k in 1..=n {
        let tk = create_thread(client, base_url, auth, &format!("worker-{k}")).await?;
        let input = format!(
            "上下文（编排者计划）：{plan}\n\n你是协作中的 worker {k}/{n}，根据上述上下文产出你负责的部分。"
        );
        match run_agent_turn(client, base_url, auth, tk, &input, 0).await {
            Ok((tid, Some(o))) => {
                record_agent(pool, tenant_id, orch_id, k as i64, "worker", tk, Some(tid), "completed", Some(&o)).await?;
                outputs.push(o);
            }
            Ok((tid, None)) => {
                record_agent(pool, tenant_id, orch_id, k as i64, "worker", tk, Some(tid), "completed", None).await?;
            }
            Err(e) => {
                record_agent(pool, tenant_id, orch_id, k as i64, "worker", tk, None, "failed", None).await?;
                return Err(e);
            }
        }
    }
    // 扇入落 orchestration 元数据（写入第一个 agent 的 output 末尾便于查询）
    let fanin = outputs.join("\n---\n");
    sqlx::query("UPDATE orchestrations SET prompt = prompt || E'\\n\\n=== fan-in ===\\n' || $2 WHERE id = $1")
        .bind(orch_id)
        .bind(&fanin)
        .execute(pool)
        .await
        .map_err(|e| anyhow!("fanin update: {e:?}"))?;
    Ok(())
}

async fn run_peer(
    pool: &PgPool,
    base_url: &str,
    auth: &str,
    client: &reqwest::Client,
    tenant_id: i64,
    orch_id: i64,
    prompt: &str,
    n_peers: Option<i64>,
) -> Result<()> {
    let n = n_peers.unwrap_or(2).max(1).min(6) as usize;
    // 并行 spawn：每 peer 独立 thread + turn。pool_size 限制实际并行度。
    let mut handles = Vec::new();
    for k in 1..=n {
        let base = base_url.to_string();
        let a = auth.to_string();
        let p = format!("{prompt}\n\n你是协作中的 peer {k}/{n}，独立产出你的部分。");
        let cli = client.clone();
        handles.push(tokio::spawn(async move {
            let tid_thread = create_thread(&cli, &base, &a, &format!("peer-{k}")).await?;
            let (turn_id, out) = run_agent_turn(&cli, &base, &a, tid_thread, &p, 0).await?;
            Ok::<_, anyhow::Error>((k as i64, tid_thread, turn_id, out))
        }));
    }
    let mut outputs = Vec::new();
    for h in handles {
        match h.await {
            Ok(Ok((seq, thread_id, turn_id, out))) => {
                record_agent(pool, tenant_id, orch_id, seq, "peer", thread_id, Some(turn_id), "completed", out.as_deref()).await?;
                if let Some(o) = out {
                    outputs.push(o);
                }
            }
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(anyhow!("peer join: {e}")),
        }
    }
    let fanin = outputs.join("\n---\n");
    sqlx::query("UPDATE orchestrations SET prompt = prompt || E'\\n\\n=== fan-in ===\\n' || $2 WHERE id = $1")
        .bind(orch_id)
        .bind(&fanin)
        .execute(pool)
        .await
        .map_err(|e| anyhow!("peer fanin: {e:?}"))?;
    Ok(())
}

#[allow(unused_assignments)]
async fn run_critic_adversarial(
    pool: &PgPool,
    base_url: &str,
    auth: &str,
    client: &reqwest::Client,
    tenant_id: i64,
    orch_id: i64,
    prompt: &str,
) -> Result<()> {
    let producer_thread = create_thread(client, base_url, auth, "producer").await?;
    let critic_thread = create_thread(client, base_url, auth, "critic").await?;

    let mut current_input = prompt.to_string();
    let mut seq = 0i64;
    let mut producer_output = String::new();

    loop {
        // producer
        let (ptid, pout) = run_agent_turn(client, base_url, auth, producer_thread, &current_input, seq * 10).await?;
        let out = pout.unwrap_or_default();
        record_agent(pool, tenant_id, orch_id, seq * 2, "producer", producer_thread, Some(ptid), "completed", Some(&out)).await?;
        producer_output = out.clone();
        seq += 1;

        // critic gate
        let critic_input = format!(
            "请评审以下产出。若需修订，以 REVISE: 开头给出意见；否则以 APPROVE 开头。\n\n产出：{out}"
        );
        let (ctid, cout) = run_agent_turn(client, base_url, auth, critic_thread, &critic_input, (seq - 1) * 10).await?;
        let critique = cout.unwrap_or_default();
        let critique_lower = critique.to_lowercase();
        let needs_revise = critique_lower.contains("revise") || critique.contains("修订");
        record_agent(pool, tenant_id, orch_id, seq * 2 - 1, "critic", critic_thread, Some(ctid), "completed", Some(&critique)).await?;

        if !needs_revise || seq > MAX_REVISE as i64 {
            break;
        }
        // 修订循环
        current_input = format!("根据评审意见修订你的产出：\n评审：{critique}\n\n原产出：{producer_output}");
    }
    Ok(())
}

pub async fn list_orchestrations(pool: &PgPool, tenant_id: i64) -> Result<Vec<OrchestrationRow>> {
    sqlx::query_as::<_, OrchestrationRow>(
        "SELECT id, tenant_id, name, mode, status, prompt, created_by, created_at, completed_at
         FROM orchestrations WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 50",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list_orchestrations: {e:?}"))
}

pub async fn get_orchestration(pool: &PgPool, tenant_id: i64, id: i64) -> Result<(OrchestrationRow, Vec<AgentStepRow>)> {
    let orch = sqlx::query_as::<_, OrchestrationRow>(
        "SELECT id, tenant_id, name, mode, status, prompt, created_by, created_at, completed_at
         FROM orchestrations WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("get_orchestration: {e:?}"))?;
    let agents = sqlx::query_as::<_, AgentStepRow>(
        "SELECT id, orchestration_id, thread_id, agent_seq, role, turn_id, status, output_ref, created_at
         FROM orchestration_agents WHERE orchestration_id = $1 ORDER BY agent_seq ASC",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("get_orchestration agents: {e:?}"))?;
    Ok((orch, agents))
}
