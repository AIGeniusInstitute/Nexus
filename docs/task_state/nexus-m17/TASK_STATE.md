# 任务状态 — Nexus M17 Skills 市场

## 任务清单
| ID | 任务 | 状态 |
|---|---|---|
| T17-1 | migration（skills 加治理字段） | ✅ |
| T17-2 | skills.rs（CRUD+publish+rollback+delete） | ✅ |
| T17-3 | http_server.rs 6 路由+handler | ✅ |
| T17-4 | lib.rs+db.rs+main.rs 接线 | ✅ |

## 验证
- cargo check 0e0w
- cargo test 32/32 零回归
- e2e：纳入完整系统测试统一验证

## 状态
全量完成，待合并 main + push。
