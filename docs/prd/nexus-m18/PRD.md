# M18 多 Agent 协作编排 PRD

## 背景
Nexus 已具备单 Agent turn 闭环（M0-M9）、并发 turn 池（M5）、warm pool（M15）。剩余 roadmap T11-4 多 Agent 协作编排尚未实现。用户要求完成此项。

## 目标
在 turn 之上构建协作编排层，支持 3 种工作模式，复用现有 turn/items/approval 端点，不重构 turn_start drain（外科手术原则）。

## 功能需求
1. **3 种协作模式**
   - orchestrator-worker：编排者先产出计划 → N 个工作者各产出一部分 → 扇入汇总
   - peer：N 个对等 agent 并行独立产出 → 扇入
   - critic-adversarial：生产者产出 → 评审者 gate（REVISE→修订循环 / APPROVE→完成）
2. 每个 agent = 独立 thread（codex_thread_id 是 thread 级别，共享会污染上下文）
3. agent 间上下文传递：前序 agent 的输出（items.content_ref）作为后序 agent 的 input
4. SIMULATE_APPROVAL=1 下编排层自动 resolve 审批，让 turn 跑通（无人值守）
5. 编排状态机：running → completed/failed
6. 编排记录可查：orchestration + 每步 agent 行

## 非目标
- 不重构 turn_start drain（HTTP 自调用复用现有端点）
- 不引入 graph engine（无状态机框架，纯 Rust async 编排）
- 不实现 DeepTalk 的 collaboration-mode-templates 全部能力（fan-in shellCheck gate 等）

## 验收标准
- AC1 POST /v1/orchestrations {mode,prompt,agents} 创建并执行编排
- AC2 orchestrator-worker 模式串行多 agent，输出含扇入
- AC3 peer 模式并行 agent（pool_size≥agents）
- AC4 critic-adversarial 模式 gate 判定（REVISE→循环 / APPROVE→完成）
- AC5 编排层自动 resolve 审批（SIMULATE_APPROVAL=1 下 turn completed）
- AC6 GET /v1/orchestrations/{id} 返回编排+agent 步骤
- AC7 零回归（M3 审批闭环/M5 并发池/M15 warm pool 不退化）
