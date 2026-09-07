-- M20: 评测中心 — 版本化 Golden Set + Rubric + 运行冻结快照（roadmap 评测体系 P1 MVP）
-- 核心思想：所有实体一律版本化、不可变，运行时绑定到具体版本集合（快照）。
-- 纯增量：不触碰 M12 eval_cases/eval_runs 旧表（保留兼容），新表独立。

-- ① 版本化实体骨架（六类共用，M20 激活 golden_set + rubric，其余建表留后续里程碑）
CREATE TABLE IF NOT EXISTS entity_version (
    id             BIGSERIAL PRIMARY KEY,
    tenant_id      BIGINT NOT NULL REFERENCES tenants(id),
    entity_type    TEXT NOT NULL CHECK (entity_type IN
        ('golden_set','rubric','agent_release','judge_config','eval_pipeline','policy')),
    entity_id      BIGINT NOT NULL,
    version_no     INT NOT NULL,
    semver         TEXT NOT NULL,
    manifest_hash  CHAR(64) NOT NULL,
    content_ref    TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'locked' CHECK (status IN ('draft','locked')),
    created_by     BIGINT REFERENCES users(id),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    parent_id      BIGINT REFERENCES entity_version(id),
    UNIQUE(entity_type, entity_id, version_no)
);
CREATE INDEX IF NOT EXISTS idx_entity_version_tenant_type
    ON entity_version(tenant_id, entity_type, status);

-- ② Golden Set 逻辑实体
CREATE TABLE IF NOT EXISTS golden_sets (
    id                BIGSERIAL PRIMARY KEY,
    tenant_id         BIGINT NOT NULL REFERENCES tenants(id),
    name              TEXT NOT NULL,
    description      TEXT,
    status           TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','locked','archived')),
    active_version_id BIGINT,
    created_by        BIGINT REFERENCES users(id),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_golden_sets_tenant ON golden_sets(tenant_id, status);

-- ③ Case 池（draft 状态可增删；publish 时快照全文入 entity_version.content_ref）
CREATE TABLE IF NOT EXISTS golden_set_cases (
    id             BIGSERIAL PRIMARY KEY,
    golden_set_id  BIGINT NOT NULL REFERENCES golden_sets(id) ON DELETE CASCADE,
    tenant_id      BIGINT NOT NULL REFERENCES tenants(id),
    case_key       TEXT NOT NULL,
    domain         TEXT,
    difficulty     TEXT,
    source         TEXT,
    input_json     JSONB NOT NULL,
    expected_json  JSONB NOT NULL,
    phi_check_status TEXT DEFAULT 'pending',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(golden_set_id, case_key)
);
CREATE INDEX IF NOT EXISTS idx_gs_cases_gs ON golden_set_cases(golden_set_id);

-- ④ Rubric 逻辑实体（criteria 存在 entity_version.content_ref JSON 内）
CREATE TABLE IF NOT EXISTS rubrics (
    id                BIGSERIAL PRIMARY KEY,
    tenant_id         BIGINT NOT NULL REFERENCES tenants(id),
    name              TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','locked','archived')),
    active_version_id BIGINT,
    created_by        BIGINT REFERENCES users(id),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_rubrics_tenant ON rubrics(tenant_id, status);

-- ⑤ 批量评测运行（一批 GoldenSet 全部 case，冻结快照）
CREATE TABLE IF NOT EXISTS eval_batch_runs (
    id                    BIGSERIAL PRIMARY KEY,
    tenant_id             BIGINT NOT NULL REFERENCES tenants(id),
    golden_set_id         BIGINT NOT NULL REFERENCES golden_sets(id),
    golden_set_version_id BIGINT NOT NULL REFERENCES entity_version(id),
    rubric_version_id     BIGINT REFERENCES entity_version(id),
    thread_id             UUID,
    snapshot_manifest     JSONB NOT NULL,
    snapshot_hash         CHAR(64) NOT NULL,
    trigger_type          TEXT NOT NULL DEFAULT 'manual',
    status               TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running','completed','failed')),
    baseline_run_id       BIGINT REFERENCES eval_batch_runs(id),
    gate_result           TEXT,
    aggregate             JSONB,
    started_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at           TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_eval_batch_tenant_time
    ON eval_batch_runs(tenant_id, started_at DESC);

-- ⑥ 逐 case 结果（WORM：不提供 UPDATE API，只 INSERT）
CREATE TABLE IF NOT EXISTS eval_case_results (
    id            BIGSERIAL PRIMARY KEY,
    run_id        BIGINT NOT NULL REFERENCES eval_batch_runs(id) ON DELETE CASCADE,
    case_id       BIGINT NOT NULL REFERENCES golden_set_cases(id),
    case_key      TEXT NOT NULL,
    turn_id       BIGINT,
    agent_output  TEXT,
    scores        JSONB NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(run_id, case_id)
);
CREATE INDEX IF NOT EXISTS idx_eval_case_results_run ON eval_case_results(run_id);
