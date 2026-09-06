import React from "react";
import { api, type Skill, type SkillVersion } from "../api";
import { Card, Table, Button, Pill, Empty, ErrBar, Modal, Field, useAsync, fmtTime } from "../ui";

export default function Skills() {
  const list = useAsync<Skill[]>(() => api.skills(), []);
  const [sel, setSel] = React.useState<number | null>(null);
  const [create, setCreate] = React.useState(false);
  return (
    <div>
      <Card title={`技能市场 (${list.data?.length || 0})`} action={<Button variant="primary" className="sm" onClick={() => setCreate(true)}>+ 新建技能</Button>}>
        <ErrBar err={list.err} />
        {list.data && list.data.length === 0 && <Empty>暂无技能</Empty>}
        {list.data && list.data.length > 0 && (
          <Table cols={[
            { key: "id", label: "ID", render: (s: Skill) => s.id },
            { key: "name", label: "名称", render: (s: Skill) => <b>{s.name}</b> },
            { key: "desc", label: "描述", render: (s: Skill) => s.description || "—" },
            { key: "status", label: "状态", render: (s: Skill) => <Pill tone={s.status === "published" ? "ok" : s.status === "archived" ? "danger" : "warn"}>{s.status}</Pill> },
            { key: "active", label: "激活版本", render: (s: Skill) => s.active_version_id || "—" },
            { key: "act", label: "", render: (s: Skill) => <Button className="sm" onClick={() => setSel(s.id)}>管理 →</Button> },
          ]} rows={list.data} />
        )}
      </Card>
      {sel != null && <SkillDetail id={sel} onClose={() => setSel(null)} onChanged={list.reload} />}
      {create && <CreateSkill onClose={() => setCreate(false)} onDone={list.reload} />}
    </div>
  );
}

function SkillDetail({ id, onClose, onChanged }: { id: number; onClose: () => void; onChanged: () => void }) {
  const skill = useAsync<Skill>(() => api.skill(id), [id]);
  const vers = useAsync<SkillVersion[]>(() => api.skillVersions(id), [id]);
  const [pub, setPub] = React.useState(false);
  return (
    <Card title={`技能 #${id}`} action={<Button className="sm" onClick={onClose}>← 返回</Button>}>
      <ErrBar err={skill.err} />
      <div className="row" style={{ marginBottom: 12 }}>
        <Button variant="primary" className="sm" onClick={() => setPub(true)}>+ 发布版本</Button>
        <Button variant="danger" className="sm" onClick={async () => { if (confirm("删除此技能？")) { await api.deleteSkill(id); onChanged(); onClose(); } }}>删除</Button>
      </div>
      <h4 style={{ color: "var(--mut)", fontSize: 12, margin: "10px 0 6px" }}>版本历史</h4>
      {vers.data && vers.data.length === 0 && <Empty>暂无版本</Empty>}
      {vers.data && vers.data.length > 0 && (
        <Table cols={[
          { key: "id", label: "版本ID", render: (v: SkillVersion) => v.id },
          { key: "version", label: "版本号", render: (v: SkillVersion) => <Pill tone="info">{v.version}</Pill> },
          { key: "checksum", label: "Checksum", className: "mono", render: (v: SkillVersion) => (v.checksum || "—").slice(0, 10) },
          { key: "content", label: "内容引用", className: "mono", render: (v: SkillVersion) => (v.content_ref || "—").slice(0, 20) },
          { key: "created", label: "发布时间", render: (v: SkillVersion) => fmtTime(v.created_at) },
          { key: "act", label: "", render: (v: SkillVersion) => v.id === skill.data?.active_version_id ? <Pill tone="ok">当前</Pill> : <Button className="sm" onClick={async () => { await api.rollbackSkill(id, v.id); skill.reload(); }}>回滚到此</Button> },
        ]} rows={vers.data} />
      )}
      {pub && <PubModal id={id} onClose={() => setPub(false)} onDone={vers.reload} />}
    </Card>
  );
}

function PubModal({ id, onClose, onDone }: { id: number; onClose: () => void; onDone: () => void }) {
  const [version, setVersion] = React.useState("");
  const [checksum, setChecksum] = React.useState("");
  const [content, setContent] = React.useState("");
  const [err, setErr] = React.useState<string | null>(null);
  return (
    <Modal title="发布版本" onClose={onClose}>
      <Field label="版本号"><input className="input" value={version} onChange={(e) => setVersion(e.target.value)} placeholder="1.0.0" /></Field>
      <Field label="Checksum (可选)"><input className="input" value={checksum} onChange={(e) => setChecksum(e.target.value)} /></Field>
      <Field label="内容引用"><input className="input" value={content} onChange={(e) => setContent(e.target.value)} placeholder="path 或 content ref" /></Field>
      <ErrBar err={err} />
      <Button variant="primary" onClick={async () => { try { await api.publishVersion(id, { version, checksum: checksum || undefined, content_ref: content || undefined }); onClose(); onDone(); } catch (e: any) { setErr(String(e?.message || e)); } }}>发布</Button>
    </Modal>
  );
}

function CreateSkill({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const [name, setName] = React.useState("");
  const [desc, setDesc] = React.useState("");
  const [err, setErr] = React.useState<string | null>(null);
  return (
    <Modal title="新建技能" onClose={onClose}>
      <Field label="名称"><input className="input" value={name} onChange={(e) => setName(e.target.value)} /></Field>
      <Field label="描述"><input className="input" value={desc} onChange={(e) => setDesc(e.target.value)} /></Field>
      <ErrBar err={err} />
      <Button variant="primary" onClick={async () => { try { await api.createSkill({ name, description: desc || undefined }); onClose(); onDone(); } catch (e: any) { setErr(String(e?.message || e)); } }}>创建</Button>
    </Modal>
  );
}
