import React from "react";
import { api, type Kb, type KbDoc, type KbHit } from "../api";
import { Card, Table, Button, Pill, Empty, ErrBar, Modal, Field, useAsync, fmtTime } from "../ui";

export default function KnowledgeBase() {
  const kbs = useAsync<Kb[]>(() => api.kbs(), []);
  const [sel, setSel] = React.useState<number | null>(null);
  const docs = useAsync<KbDoc[]>(() => sel ? api.kbDocs(sel) : Promise.resolve([]), [sel]);

  return (
    <div>
      <Card title="知识库" action={<CreateKb onCreated={() => kbs.reload()} />}>
        <ErrBar err={kbs.err} />
        {kbs.data && kbs.data.length === 0 && <Empty>暂无知识库</Empty>}
        {kbs.data && kbs.data.length > 0 && (
          <Table cols={[
            { key: "id", label: "ID", render: (k: Kb) => k.id },
            { key: "name", label: "名称", render: (k: Kb) => k.name },
            { key: "desc", label: "描述", render: (k: Kb) => k.description || "—" },
            { key: "act", label: "", render: (k: Kb) => <Button className="sm" onClick={() => setSel(k.id)}>管理 →</Button> },
          ]} rows={kbs.data} />
        )}
      </Card>
      {sel != null && (
        <KbDetail kbId={sel} docs={docs.data || []} onReload={docs.reload} />
      )}
    </div>
  );
}

function CreateKb({ onCreated }: { onCreated: () => void }) {
  const [open, setOpen] = React.useState(false);
  const [name, setName] = React.useState("");
  const [desc, setDesc] = React.useState("");
  const [err, setErr] = React.useState<string | null>(null);
  return <>
    <Button variant="primary" className="sm" onClick={() => setOpen(true)}>+ 新建知识库</Button>
    {open && <Modal title="新建知识库" onClose={() => setOpen(false)}>
      <Field label="名称"><input className="input" value={name} onChange={(e) => setName(e.target.value)} /></Field>
      <Field label="描述"><input className="input" value={desc} onChange={(e) => setDesc(e.target.value)} /></Field>
      <ErrBar err={err} />
      <Button variant="primary" onClick={async () => { try { await api.kbCreate(name, desc || undefined); setOpen(false); setName(""); setDesc(""); onCreated(); } catch (e: any) { setErr(String(e?.message || e)); } }}>创建</Button>
    </Modal>}
  </>;
}

function KbDetail({ kbId, docs, onReload }: { kbId: number; docs: KbDoc[]; onReload: () => void }) {
  const [ingest, setIngest] = React.useState(false);
  const [q, setQ] = React.useState("");
  const [kw, setKw] = React.useState("");
  const [hits, setHits] = React.useState<KbHit[] | null>(null);
  const [err, setErr] = React.useState<string | null>(null);

  async function search() {
    if (!q.trim()) return;
    try { setErr(null); const r = await api.kbSearch(kbId, q, kw || undefined, 5); setHits(r); }
    catch (e: any) { setErr(String(e?.message || e)); }
  }

  return (
    <Card title={`文档管理 (KB #${kbId})`} action={
      <div className="row">
        <Button variant="primary" className="sm" onClick={() => setIngest(true)}>+ 摄入文档</Button>
        <Button className="sm" onClick={onReload}>刷新</Button>
      </div>}>
      <ErrBar err={err} />
      <div className="row" style={{ marginBottom: 14 }}>
        <input className="input" style={{ flex: 2 }} placeholder="语义搜索 query…" value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") search(); }} />
        <input className="input" style={{ flex: 1 }} placeholder="关键词 (可选)" value={kw} onChange={(e) => setKw(e.target.value)} />
        <Button variant="primary" onClick={search}>搜索</Button>
      </div>
      {hits && (
        <div style={{ marginBottom: 16 }}>
          <div className="muted" style={{ fontSize: 12, marginBottom: 6 }}>召回结果 ({hits.length})</div>
          {hits.length === 0 && <Empty>无命中</Empty>}
          {hits.map((h) => (
            <div key={h.id} style={{ padding: 10, background: "var(--panel2)", borderRadius: 8, marginBottom: 8, border: "1px solid var(--bd)" }}>
              <div className="between"><b style={{ fontSize: 13 }}>{h.title}</b><Pill tone="ok">score {h.score.toFixed(4)}</Pill></div>
              <div className="muted" style={{ fontSize: 11 }}>{h.source_uri}</div>
              <div style={{ fontSize: 12, marginTop: 6 }}>{h.snippet}</div>
            </div>
          ))}
        </div>
      )}
      <Table cols={[
        { key: "id", label: "ID", render: (d: KbDoc) => d.id },
        { key: "title", label: "标题", render: (d: KbDoc) => d.title },
        { key: "tokens", label: "Tokens", render: (d: KbDoc) => d.tokens || "—" },
        { key: "hash", label: "Hash", className: "mono", render: (d: KbDoc) => (d.content_hash || "—").slice(0, 10) },
        { key: "created", label: "摄入时间", render: (d: KbDoc) => fmtTime(d.created_at) },
        { key: "act", label: "", render: (d: KbDoc) => <Button className="sm danger" onClick={async () => { await api.kbDeleteDoc(kbId, d.id); onReload(); }}>删除</Button> },
      ]} rows={docs} />
      {ingest && <IngestModal kbId={kbId} onClose={() => setIngest(false)} onDone={onReload} />}
    </Card>
  );
}

function IngestModal({ kbId, onClose, onDone }: { kbId: number; onClose: () => void; onDone: () => void }) {
  const [title, setTitle] = React.useState("");
  const [content, setContent] = React.useState("");
  const [uri, setUri] = React.useState("");
  const [err, setErr] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  return (
    <Modal title="摄入文档" onClose={onClose}>
      <Field label="标题"><input className="input" value={title} onChange={(e) => setTitle(e.target.value)} /></Field>
      <Field label="内容"><textarea className="input" value={content} onChange={(e) => setContent(e.target.value)} /></Field>
      <Field label="来源 URI（可选）"><input className="input" value={uri} onChange={(e) => setUri(e.target.value)} /></Field>
      <ErrBar err={err} />
      <Button variant="primary" disabled={busy} onClick={async () => {
        setBusy(true); setErr(null);
        try { await api.kbIngest(kbId, { title, content, source_uri: uri || undefined }); onClose(); onDone(); }
        catch (e: any) { setErr(String(e?.message || e)); }
        finally { setBusy(false); }
      }}>{busy ? "摄入中（embedding）…" : "摄入"}</Button>
    </Modal>
  );
}
