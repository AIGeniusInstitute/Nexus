## NexusAgent 

Nexus，企业级 Agent-Native 平台，汇聚企业级知识、技能、工具、系统、租户、数据，「核心引擎+外围管控」架构。

Nexus 项目基本信息：
  - 本地仓库目录：~/Nexus
  - 项目文档目录：~/Nexus/docs
  - 项目架构设计：~/Nexus/docs/architecture
  
  - 两个git远程已配置：
    - origin → git@gitcode.com:AIGeniusInstitute/Nexus.git
    - github → git@github.com:AIGeniusInstitute/Nexus.git

## 工作流程

### 需求开发任务工作流程

0.针对该任务创建工作分支树 worktree
1.生成完整详细的需求 prd 文档 & 每个功能点的验收标准 & 测试用例，写入 docs/prd 目录下，要创建这个需求自己的独立文件夹。
2.设计技术方案，详细开发技术方案文档，写入 docs/tech_solution 目录下，也要创建这个需求自己的独立文件夹
3.方案全面实施，执行编码；执行过程中，把执行状态写到 docs/task_state 目录下，也要创建这个需求自己的独立文件夹。开发环境中基础设施不具备的，请在本机现场安装搭建一个，部署到docker里，应用到开发部署测试流程中，继续完成需求/编码/测试/部署等任务目标。
4.执行测试用例，每个用例步骤要验收通过的截图，遇到问题bug，执行“Issue 修复任务工作流程”，查找运行log和相关代码行，修复好问题，再次验证，循环执行步骤2,3。 直到全部测试和修复全部通过。最终，输出完整测试报告HTML产物，放到docs/test_report 目录下，也要创建这个需求自己的独立文件夹 。
5.合并 worktree 分支到 main 分支，提交并 push 到 main。

### Issue 修复任务工作流程

针对 bug 修复 / 线上事故 / CI 故障等 issue 处理（**不**走 PRD → tech_solution → test_report 那条线，因为不是新需求开发）：

0.针对该任务创建工作分支树 worktree
1.定位根因：必须有证据（日志、API 输出、测试结果），禁止主观判断下结论
2.执行编码修复，与 issue 文档一并 commit。遇到问题bug，查找运行log和相关代码行，修复好问题，再次验证，循环执行，直到全部测试和修复全部通过。
3.把本次 issue 处理经验沉淀到 `docs/issues/{YYYY-MM-DD}-{slug}.md`，文件结构必须包含：
- `## 1. 用户现象`：从用户/外部视角描述看到了什么
- `## 2. 问题描述`：从技术视角简述发生了什么
- `## 3. 根因`：代码层面 / 基础设施层面的具体原因，附外部依据链接
- `## 4. 复现路径`：步骤化，让不熟悉代码的人也能复现
- `## 5. 诊断方法`：能复制粘贴的命令（curl / grep / 内部脚本）
- `## 6. 修复方案`：diff 形式呈现关键改动 + 选型理由
- `## 7. 处理卡住的状态`（如适用）：如何救活已 stuck 的运行态
- `## 8. 经验沉淀 / 预防`：未来怎么避免同类问题、巡检脚本、告警建议
4.合并 worktree 分支到 main 分支，提交并push 到 main

## 工作原则【最高宪法】

YOU MUST Follow The 4 Working Principles:

1. Think Before Coding

Core principle: "Don't assume. Don't hide confusion. Surface tradeoffs."
Before implementing anything non-trivial, the file instructs Claude to state its assumptions explicitly. If there are multiple valid interpretations, present them. If something is unclear, halt and ask.
This principle targets what Karpathy identified as the single most destructive LLM coding behavior: silent assumption-making. Models are trained on massive corpora of human writing, where confident assertion is typically rewarded. The result: when Claude encounters an ambiguous spec, it fills in the gaps with whatever seems plausible — and charges ahead.
The fix isn't complicated. It's forcing a checkpoint before execution.

2. Simplicity First

Core principle: "Minimum code that solves the problem. Nothing speculative."
The file prohibits unrequested features, abstractions for single-use code, unnecessary configurability, and error handling for scenarios that can't actually happen.
There's a self-test embedded in the template: "Would an experienced engineer view this as overengineered?" This is deliberately subjective — it invokes a heuristic judgment rather than a checklist.
The pattern it corrects: LLMs are extraordinarily good at pattern-matching against complex, enterprise-grade code in their training data. When asked to "add a cache," Claude will often produce a full-featured LRU implementation with eviction policies, thread safety, and metrics hooks — because that's what "cache implementation" looks like in most codebases it has seen. That's frequently five times more code than what was needed.
ℹ️ The Simplicity First principle is not a productivity hack — it's a correctness guardrail. Speculative code ships bugs you didn't write but still own.

3. Surgical Changes

Core principle: "Touch only what you must. Clean up only your own mess."
When modifying a file, Claude should not "enhance" surrounding code, reformat things it didn't break, or refactor patterns it disagrees with. There's a sharp distinction drawn between dead code you introduced (clean it up) and pre-existing dead code (flag it, don't touch it).
This principle most closely maps to how good human engineers work on unfamiliar codebases. When you open a PR to fix a bug, you don't simultaneously rewrite the adjacent function because it's "not idiomatic" — you fix the bug, get it reviewed, and leave editorial improvements for a separate ticket.
Claude, left unconstrained, tends to interpret "fix this" as implicit permission to improve the surrounding area. That creates noisy diffs, hidden regressions, and review overhead that cancels the efficiency gains you were trying to capture.

4. Goal-Driven Execution

Core principle: "Define success criteria. Loop until verified."
Every task should be converted into a measurable objective with explicit verification steps before Claude starts writing. The difference between "add a login form" and "add a login form — success when: form renders at /login, submits correctly with valid credentials, shows error state on invalid credentials, and passes the existing auth test suite" is not pedantry. It's the difference between an agent that loops productively and one that declares victory on a half-finished implementation.